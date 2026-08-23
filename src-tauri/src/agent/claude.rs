#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::{
    io::AsyncReadExt,
    process::Command,
    sync::watch,
    time::{sleep, timeout},
};

use crate::{
    db::Database,
    domain::draft::Assignment,
    error::{AppError, AppResult},
};

use super::{
    backend::{
        AgentBackend, AgentTask, AgentTaskResult, AvailabilityReason, BackendStatus, ProbeResult,
    },
    registry::{
        effective_capability_hash, expected_capabilities, m0_tool_registry, CapabilityEntry,
    },
    session::{AgentSession, SessionMode},
};

const PROBE_TIMEOUT: Duration = Duration::from_secs(45);
const TASK_TIMEOUT: Duration = Duration::from_secs(180);
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);
/// 鉴定整批候选的总预算。nvm / fnm 的版本目录可以有几十个，逐个跑 `--version`
/// 会把启动拖成几十秒；超预算就按已有的失败原因收工，不无限等下去。
const QUALIFY_BUDGET: Duration = Duration::from_secs(10);

/// 一次安装资格鉴定的结果。**只回答「有没有合格 CLI」**，不回答就绪度。
#[derive(Debug, Clone)]
struct Qualification {
    executable: Option<PathBuf>,
    version: Option<String>,
    reason: Option<AvailabilityReason>,
}

#[derive(Debug)]
pub struct ClaudeCodeBackend {
    candidates: Vec<PathBuf>,
    helper_path: PathBuf,
    version_timeout: Duration,
    qualification: tokio::sync::OnceCell<Qualification>,
}

impl ClaudeCodeBackend {
    pub fn discover(helper_path: PathBuf) -> Self {
        Self::with_candidates(discover_claude(), helper_path)
    }

    pub fn with_paths(executable: Option<PathBuf>, helper_path: PathBuf) -> Self {
        Self::with_candidates(executable.into_iter().collect(), helper_path)
    }

    pub fn with_candidates(candidates: Vec<PathBuf>, helper_path: PathBuf) -> Self {
        Self {
            candidates,
            helper_path,
            version_timeout: VERSION_TIMEOUT,
            qualification: tokio::sync::OnceCell::new(),
        }
    }

    #[cfg(test)]
    fn with_version_timeout(mut self, timeout: Duration) -> Self {
        self.version_timeout = timeout;
        self
    }

    /// **安装资格：跟随符号链接后是普通文件、有执行权限、`--version` 限时以 0 退出且输出非空**
    /// （[01 §3.5](../../../docs/prd/01-agent-runtime.md)）。
    ///
    /// 修正前这里只有一句 `is_file()`：一个没有执行位的同名残留文件就足以让界面说「已安装」，
    /// 而三种失败原因全塌成一句「未安装」，用户按指引重装也修不好。
    async fn qualify(&self) -> &Qualification {
        self.qualification
            .get_or_init(|| async {
                let started = std::time::Instant::now();
                let mut reason = AvailabilityReason::NotFound;
                for candidate in &self.candidates {
                    match self.qualify_candidate(candidate).await {
                        Ok(version) => {
                            return Qualification {
                                executable: Some(candidate.clone()),
                                version: Some(version),
                                reason: None,
                            };
                        }
                        Err(candidate_reason) => reason = reason.worse_of(candidate_reason),
                    }
                    if started.elapsed() >= QUALIFY_BUDGET {
                        break;
                    }
                }
                Qualification {
                    executable: None,
                    version: None,
                    reason: Some(reason),
                }
            })
            .await
    }

    async fn qualify_candidate(&self, candidate: &Path) -> Result<String, AvailabilityReason> {
        // `canonicalize` 顺带把符号链接跟到底：断链在这里就失败，指向目录的链接被下一步挡住。
        let resolved =
            std::fs::canonicalize(candidate).map_err(|_| AvailabilityReason::NotFound)?;
        let metadata = std::fs::metadata(&resolved).map_err(|_| AvailabilityReason::NotFound)?;
        if !metadata.is_file() {
            return Err(AvailabilityReason::NotExecutable);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 == 0 {
                return Err(AvailabilityReason::NotExecutable);
            }
        }
        let output = timeout(
            self.version_timeout,
            Command::new(candidate)
                .arg("--version")
                .stdin(Stdio::null())
                .output(),
        )
        .await
        .map_err(|_| AvailabilityReason::VersionUnreadable)?
        .map_err(|_| AvailabilityReason::VersionUnreadable)?;
        if !output.status.success() {
            return Err(AvailabilityReason::VersionUnreadable);
        }
        let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if version.is_empty() {
            return Err(AvailabilityReason::VersionUnreadable);
        }
        Ok(version)
    }

    async fn executable(&self) -> AppResult<PathBuf> {
        self.qualify().await.executable.clone().ok_or_else(|| {
            AppError::new(
                "agent.backend_unavailable",
                "未检测到合格的 Claude Code CLI，请先安装或修复后再解析",
            )
        })
    }

    async fn version(&self) -> AppResult<String> {
        self.qualify().await.version.clone().ok_or_else(|| {
            AppError::new(
                "agent.backend_unavailable",
                "未检测到合格的 Claude Code CLI，请先安装或修复后再解析",
            )
        })
    }

    async fn sealed_command(
        &self,
        session: &AgentSession,
        prompt: &str,
        agent_session_id: Option<&str>,
    ) -> AppResult<Command> {
        if !self.helper_path.is_file() {
            return Err(AppError::new(
                "agent.spawn_failed",
                format!("找不到 MCP helper：{}", self.helper_path.display()),
            ));
        }
        let config = json!({
            "mcpServers": {
                "daybook": {
                    "command": self.helper_path,
                    "args": [],
                    "env": {
                        "DAYBOOK_MCP_SOCKET": session.socket_path(),
                        "DAYBOOK_MCP_TOKEN": session.token(),
                    }
                }
            }
        });
        let allowed_tools = allowed_tool_names().join(",");
        let mut command = Command::new(self.executable().await?);
        seal(&mut command);
        command
            .arg("-p")
            .arg(prompt)
            .arg("--mcp-config")
            .arg(config.to_string())
            .arg("--allowedTools")
            .arg(allowed_tools)
            .current_dir(
                session
                    .socket_path()
                    .parent()
                    .unwrap_or_else(|| Path::new("/tmp")),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command.kill_on_drop(true);
        if let Some(session_id) = agent_session_id {
            command.arg("--session-id").arg(session_id);
        }
        #[cfg(unix)]
        command.as_std_mut().process_group(0);
        Ok(command)
    }

    async fn run_sealed(
        &self,
        session: &AgentSession,
        prompt: &str,
        duration: Duration,
        agent_session_id: Option<&str>,
        mut cancel: watch::Receiver<bool>,
    ) -> AppResult<std::process::Output> {
        let mut command = self
            .sealed_command(session, prompt, agent_session_id)
            .await?;
        let mut child = command
            .spawn()
            .map_err(|error| AppError::new("agent.spawn_failed", error.to_string()))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::new("agent.spawn_failed", "无法读取 agent stdout"))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| AppError::new("agent.spawn_failed", "无法读取 agent stderr"))?;
        let stdout_task = tokio::spawn(async move {
            let mut bytes = Vec::new();
            stdout.read_to_end(&mut bytes).await.map(|_| bytes)
        });
        let stderr_task = tokio::spawn(async move {
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).await.map(|_| bytes)
        });
        let status = tokio::select! {
            status = child.wait() => status
                .map_err(|error| AppError::new("agent.spawn_failed", error.to_string()))?,
            changed = cancel.changed() => {
                if changed.is_ok() && *cancel.borrow() {
                    terminate_child_tree(&mut child).await;
                    let (stdout, stderr) = collect_reader_output(stdout_task, stderr_task).await;
                    return Err(AppError::new("agent.cancelled", "解析已由用户停止")
                        .with_detail(json!({ "stdout": stdout, "stderr": stderr })));
                }
                child.wait().await
                    .map_err(|error| AppError::new("agent.spawn_failed", error.to_string()))?
            }
            _ = sleep(duration) => {
                terminate_child_tree(&mut child).await;
                let (stdout, stderr) = collect_reader_output(stdout_task, stderr_task).await;
                return Err(AppError::new("agent.timeout", "agent 子进程超过硬超时")
                    .with_detail(json!({ "stdout": stdout, "stderr": stderr })));
            }
        };
        let output = std::process::Output {
            status,
            stdout: stdout_task
                .await
                .map_err(|error| AppError::storage(format!("stdout 采集任务失败：{error}")))??,
            stderr: stderr_task
                .await
                .map_err(|error| AppError::storage(format!("stderr 采集任务失败：{error}")))??,
        };
        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(classify_process_failure(&stdout, &stderr));
        }
        Ok(output)
    }

    /// 密封契约 = **实际会传给 CLI 的每一个 flag 与 env**，不是一份手抄的清单。
    ///
    /// 指纹此前是一个手写的 JSON 字面量：改 `sealed_command` 里的 flag，指纹纹丝不动——
    /// 而指纹存在的全部意义就是「密封配置变没变」。现在它从同一条命令读回来，
    /// 加一个 `--dangerously-skip-permissions` 而指纹不变在构造上不可能。
    ///
    /// 每次会话都不同的三样（socket 路径、token、prompt）用固定占位串，
    /// 于是指纹既覆盖全部 flag，又不随机器和会话漂移。
    fn sealed_config_contract(&self) -> Value {
        let mut command = Command::new("<claude>");
        seal(&mut command);
        command
            .arg("-p")
            .arg("<prompt>")
            .arg("--mcp-config")
            .arg(
                json!({
                    "mcpServers": {
                        "daybook": {
                            "command": self.helper_path.file_name().unwrap_or_default(),
                            "args": [],
                            "env": {
                                "DAYBOOK_MCP_SOCKET": "<socket>",
                                "DAYBOOK_MCP_TOKEN": "<token>",
                            }
                        }
                    }
                })
                .to_string(),
            )
            .arg("--allowedTools")
            .arg(allowed_tool_names().join(","));
        let standard = command.as_std();
        let args = standard
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let mut environment = standard
            .get_envs()
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    key.to_string_lossy(),
                    value.unwrap_or_default().to_string_lossy()
                )
            })
            .collect::<Vec<_>>();
        environment.sort();
        json!({ "backend": self.id(), "args": args, "environment": environment })
    }

    fn sealed_config_fingerprint(&self) -> String {
        format!(
            "{:x}",
            Sha256::digest(self.sealed_config_contract().to_string().as_bytes())
        )
    }
}

/// 密封本身：关掉内置工具、子 agent、外部配置来源与自动记忆。
/// **`sealed_command` 与指纹共用这一处**——两者不可能再分叉。
fn seal(command: &mut Command) {
    command
        .arg("--tools")
        .arg("")
        .arg("--agents")
        .arg("{}")
        .arg("--strict-mcp-config")
        .arg("--setting-sources")
        .arg("")
        .arg("--no-session-persistence")
        .arg("--disable-slash-commands")
        .arg("--output-format")
        .arg("stream-json")
        .arg("--verbose")
        .arg("--include-hook-events")
        .arg("--no-chrome")
        .env("DISABLE_AUTOUPDATER", "1")
        .env("CLAUDE_CODE_DISABLE_AGENT_VIEW", "1")
        .env("CLAUDE_CODE_DISABLE_EXPLORE_PLAN_AGENTS", "1")
        .env("CLAUDE_CODE_MAX_SUBAGENTS_PER_SESSION", "0")
        .env("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1");
}

fn allowed_tool_names() -> Vec<String> {
    m0_tool_registry()
        .into_iter()
        .map(|tool| format!("mcp__daybook__{}", tool.name))
        .collect()
}

async fn collect_reader_output(
    stdout_task: tokio::task::JoinHandle<std::io::Result<Vec<u8>>>,
    stderr_task: tokio::task::JoinHandle<std::io::Result<Vec<u8>>>,
) -> (String, String) {
    let stdout = stdout_task
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    let stderr = stderr_task
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    (
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    )
}

async fn terminate_child_tree(child: &mut tokio::process::Child) {
    let process_group_id = child.id();
    #[cfg(unix)]
    if let Some(process_group_id) = process_group_id {
        // SAFETY: `sealed_command` creates a fresh process group whose id is the child pid.
        // A negative pid targets that group only. SIGTERM gives the CLI and helper a short
        // opportunity to flush their log sinks before the hard-kill fallback below.
        unsafe {
            libc::kill(-(process_group_id as i32), libc::SIGTERM);
        }
    }

    if timeout(Duration::from_secs(2), child.wait()).await.is_ok() {
        return;
    }

    #[cfg(unix)]
    if let Some(process_group_id) = process_group_id {
        // SAFETY: this is the same dedicated process group targeted above. If the child exited
        // during the grace period, ESRCH is harmless and the direct-child fallback remains safe.
        unsafe {
            libc::kill(-(process_group_id as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

#[async_trait]
impl AgentBackend for ClaudeCodeBackend {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    async fn status(&self) -> BackendStatus {
        let qualification = self.qualify().await;
        match (&qualification.executable, &qualification.version) {
            (Some(executable), Some(version)) => {
                BackendStatus::qualified(self.id(), executable.clone(), version.clone())
            }
            _ => BackendStatus::unqualified(
                self.id(),
                qualification.reason.unwrap_or(AvailabilityReason::NotFound),
            ),
        }
    }

    async fn probe_cache_key(&self) -> AppResult<String> {
        Ok(format!("{}:{}", self.id(), self.version().await?))
    }

    async fn probe(&self, database: Arc<Database>) -> AppResult<ProbeResult> {
        let backend_version = self.version().await?;
        let session = AgentSession::start(database, SessionMode::Probe).await?;
        let (_cancel_tx, cancel_rx) = watch::channel(false);
        let output = self
            .run_sealed(
                &session,
                "这是能力探测。只调用一次 mcp__daybook__list_pending_sources，然后回复 PROBE_DONE。不得调用其他能力。",
                PROBE_TIMEOUT,
                None,
                cancel_rx,
            )
            .await?;
        let stdout = String::from_utf8(output.stdout)
            .map_err(|error| AppError::storage(format!("探测输出不是 UTF-8：{error}")))?;
        let manifest = parse_capability_manifest(&stdout)?;
        let expected = expected_capabilities();
        if manifest != expected {
            return Err(AppError::new(
                "agent.tool_surface_unsealed",
                "Claude Code 的有效能力清单超出 Daybook M0 工具面",
            )
            .with_detail(json!({ "expected": expected, "actual": manifest })));
        }
        Ok(ProbeResult {
            backend_id: self.id().to_owned(),
            backend_version,
            effective_capability_hash: effective_capability_hash(&manifest),
            sealed_config_fingerprint: self.sealed_config_fingerprint(),
            manifest,
        })
    }

    async fn run_task(
        &self,
        database: Arc<Database>,
        task: AgentTask,
        cancel: watch::Receiver<bool>,
    ) -> AppResult<AgentTaskResult> {
        let session = AgentSession::start(
            database,
            SessionMode::Task(Assignment {
                source_id: task.source_id,
                attempt_id: task.attempt_id,
            }),
        )
        .await?;
        let output = self
            .run_sealed(
                &session,
                &task.task_prompt,
                TASK_TIMEOUT,
                Some(&task.agent_session_id),
                cancel,
            )
            .await?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let metadata = parse_result_metadata(&stdout);
        Ok(AgentTaskResult {
            success: output.status.success() && session.is_complete(),
            model_id: metadata.model_id,
            agent_session_id: metadata.session_id,
            stdout,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            trace_events: session.trace_events(),
            debug_events: session.debug_events(),
        })
    }
}

#[derive(Default)]
struct ResultMetadata {
    model_id: Option<String>,
    session_id: Option<String>,
}

fn parse_result_metadata(stream: &str) -> ResultMetadata {
    let mut result = ResultMetadata::default();
    for value in stream
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    {
        if result.model_id.is_none() {
            result.model_id =
                string_at(&value, &["model"]).or_else(|| string_at(&value, &["message", "model"]));
        }
        if result.session_id.is_none() {
            result.session_id =
                string_at(&value, &["session_id"]).or_else(|| string_at(&value, &["sessionId"]));
        }
    }
    result
}

fn parse_capability_manifest(stream: &str) -> AppResult<BTreeSet<CapabilityEntry>> {
    let values = stream
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    let init = values
        .iter()
        .find(|value| {
            string_at(value, &["subtype"]).as_deref() == Some("init")
                || string_at(value, &["type"]).as_deref() == Some("init")
        })
        .ok_or_else(|| {
            AppError::new(
                "agent.tool_surface_unsealed",
                "Claude Code 未返回结构化 init 能力清单",
            )
        })?;
    let mut manifest = BTreeSet::new();
    if let Some(tools) = init.get("tools").and_then(Value::as_array) {
        for item in tools {
            let name = item
                .as_str()
                .map(ToOwned::to_owned)
                .or_else(|| string_at(item, &["name"]));
            if let Some(name) = name {
                let provider = name
                    .strip_prefix("mcp__")
                    .and_then(|rest| rest.split("__").next())
                    .unwrap_or("builtin")
                    .to_owned();
                manifest.insert(CapabilityEntry {
                    kind: "tool".to_owned(),
                    provider,
                    name: Some(name),
                    capability: None,
                });
            }
        }
    }
    add_nonempty_collection(init, "plugins", "plugin", &mut manifest);
    add_nonempty_collection(init, "skills", "skill", &mut manifest);
    let agent_invocation_available = manifest.iter().any(|entry| {
        entry.kind == "tool"
            && entry
                .name
                .as_deref()
                .is_some_and(|name| matches!(name, "Agent" | "Task"))
    });
    if agent_invocation_available {
        add_nonempty_collection(init, "agents", "agent", &mut manifest);
    }
    add_nonempty_collection(init, "slash_commands", "slash_command", &mut manifest);
    add_nonempty_collection(init, "memory_paths", "memory", &mut manifest);
    if let Some(mode) =
        string_at(init, &["permissionMode"]).or_else(|| string_at(init, &["permission_mode"]))
    {
        if mode == "bypassPermissions" {
            manifest.insert(CapabilityEntry {
                kind: "permission_mode".to_owned(),
                provider: "claude-code".to_owned(),
                name: None,
                capability: Some(mode),
            });
        }
    }
    for value in &values {
        let event_type = string_at(value, &["type"]).unwrap_or_default();
        if event_type.starts_with("hook_") {
            manifest.insert(CapabilityEntry {
                kind: "hook".to_owned(),
                provider: "claude-code".to_owned(),
                name: None,
                capability: Some(event_type),
            });
        }
    }
    if manifest.is_empty() {
        return Err(AppError::new(
            "agent.tool_surface_unsealed",
            "结构化能力清单为空，无法证明工具面已密封",
        ));
    }
    Ok(manifest)
}

fn add_nonempty_collection(
    value: &Value,
    key: &str,
    kind: &str,
    manifest: &mut BTreeSet<CapabilityEntry>,
) {
    let Some(collection) = value.get(key) else {
        return;
    };
    match collection {
        Value::Array(items) => {
            for item in items {
                manifest.insert(CapabilityEntry {
                    kind: kind.to_owned(),
                    provider: "claude-code".to_owned(),
                    name: None,
                    capability: Some(item.to_string()),
                });
            }
        }
        Value::Object(items) => {
            for (name, item) in items {
                manifest.insert(CapabilityEntry {
                    kind: kind.to_owned(),
                    provider: "claude-code".to_owned(),
                    name: None,
                    capability: Some(format!("{name}:{}", item)),
                });
            }
        }
        _ => {}
    }
}

fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    path.iter()
        .try_fold(value, |current, key| current.get(key))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

/// **失败原因不一定落在 stderr 上。**（2026-08-23 人工验收实测，Claude Code 2.1.241）
///
/// 未登录时子进程退出码是 1、**stderr 是 0 字节**，认证失败只写在 stdout 的 stream-json 里：
/// 一条 `"error":"authentication_failed"` 的事件，加一条 `"is_error":true` /
/// `"result":"Not logged in · Please run /login"` 的终结事件。只读 stderr 的分类器拿到空串，
/// 落到兜底分支 `agent.spawn_failed`，于是「该去登录」和「不知道出了什么事」变成同一句话——
/// [01 §3.5](../../../docs/prd/01-agent-runtime.md) 明令禁止的正是这件事。
///
/// 两个流合成一段信号后交给**同一张词表**判定，避免 stdout 与 stderr 各判各的再分叉。
fn classify_process_failure(stdout: &str, stderr: &str) -> AppError {
    let mut signal = stderr.trim().to_owned();
    for line in stream_json_failure_signals(stdout) {
        if !signal.is_empty() {
            signal.push('\n');
        }
        signal.push_str(&line);
    }
    classify_process_error(&signal)
}

/// 只取两处承载失败原因的字段：任一事件的顶层 `error`，以及 `is_error` 为真的终结事件的
/// `result` 文本。**不能按 `subtype` 判**——认证失败那条终结事件的 `subtype` 仍写着
/// `"success"`，跟着它走会把失败读成成功。整个 stdout 塞进词表同样不行：正常解析的输出里
/// 本来就会出现账目文本，误判概率不受控。
fn stream_json_failure_signals(stdout: &str) -> Vec<String> {
    let mut signals = Vec::new();
    for line in stdout.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(error) = event.get("error").and_then(Value::as_str) {
            signals.push(error.to_owned());
        }
        if event.get("type").and_then(Value::as_str) == Some("result")
            && event.get("is_error").and_then(Value::as_bool) == Some(true)
        {
            if let Some(result) = event.get("result").and_then(Value::as_str) {
                signals.push(result.to_owned());
            }
        }
    }
    signals
}

fn classify_process_error(signal: &str) -> AppError {
    let normalized = signal.to_ascii_lowercase();
    if normalized.contains("not logged in")
        || normalized.contains("authentication")
        || normalized.contains("login required")
    {
        return AppError::new(
            "agent.not_authenticated",
            "Claude Code 已安装但尚未登录，请先在终端完成登录",
        )
        .with_detail(json!({ "failure_signal": signal.trim() }));
    }
    if normalized.contains("quota")
        || normalized.contains("usage limit")
        || normalized.contains("rate limit")
    {
        return AppError::new("agent.quota_exhausted", "Claude Code 当前额度不足")
            .with_detail(json!({ "failure_signal": signal.trim() }));
    }
    AppError::new(
        "agent.spawn_failed",
        if signal.trim().is_empty() {
            "Claude Code 子进程异常退出".to_owned()
        } else {
            signal.trim().to_owned()
        },
    )
    .with_detail(json!({ "failure_signal": signal.trim() }))
}

fn discover_claude() -> Vec<PathBuf> {
    discover_claude_in(
        std::env::var_os("PATH"),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

/// **从 Finder 启动的 `.app` 只继承 `/usr/bin:/bin:/usr/sbin:/sbin`**，所以 `PATH` 那一路
/// 在打包后基本必然落空，只剩下面这些硬编码位置在兜底。用户终端里 `claude` 跑得好好的、
/// 应用却说「未安装」，就是这么来的。
///
/// **不去 spawn 一个登录 shell 问它的 `PATH`**：唯一允许 spawn 的子进程是 agent CLI 本身
/// （[`rust-tauri.md` §2](../../../.claude/rules/rust-tauri.md)），所以只能穷举常见安装位置。
fn discover_claude_in(path: Option<std::ffi::OsString>, home: Option<PathBuf>) -> Vec<PathBuf> {
    let mut candidates = std::env::split_paths(&path.unwrap_or_default())
        .map(|directory| directory.join("claude"))
        .collect::<Vec<_>>();
    if let Some(home) = home {
        for relative in [
            ".local/bin/claude",
            ".claude/local/claude",
            ".npm-global/bin/claude",
            ".npm/bin/claude",
            ".volta/bin/claude",
            ".bun/bin/claude",
            ".yarn/bin/claude",
            ".local/share/pnpm/claude",
        ] {
            candidates.push(home.join(relative));
        }
        candidates.extend(node_version_manager_candidates(&home));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin/claude"));
    candidates.push(PathBuf::from("/usr/local/bin/claude"));
    // **只做枚举，不做筛选**：合格与否由 `qualify_candidate` 判（跟随符号链接 + 执行位 +
    // `--version`）。此前这里 `find(is_file)`，第一个同名残留文件就把真安装挡在了后面。
    candidates
}

/// nvm / fnm / n 把二进制放在带版本号的目录里，路径中间有一层通配。标准库没有 glob，
/// 手动读目录；版本目录按字典序倒排，让较新的版本先命中。
fn node_version_manager_candidates(home: &Path) -> Vec<PathBuf> {
    let roots = [
        home.join(".nvm/versions/node"),
        home.join("Library/Application Support/fnm/node-versions"),
        home.join(".local/share/fnm/node-versions"),
        home.join(".local/state/fnm_multishells"),
        home.join("n/versions/node"),
    ];
    let mut found = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        let mut versions = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        versions.sort();
        versions.reverse();
        for version in versions {
            found.push(version.join("bin/claude"));
            found.push(version.join("installation/bin/claude"));
        }
    }
    found
}

#[cfg(test)]
mod agent {
    use super::*;

    /// **2026-08-23 人工验收抓到的真实样本**（Claude Code 2.1.241，干净 `HOME` 下的密封探测）：
    /// 退出码 1、stderr **0 字节**，失败原因全在 stdout。字段照抄真实输出，只删掉与判定无关的
    /// usage / uuid 噪声。
    const UNAUTHENTICATED_STDOUT: &str = concat!(
        r#"{"type":"system","subtype":"init","tools":["mcp__daybook__read_source"],"apiKeySource":"none"}"#,
        "\n",
        r#"{"type":"assistant","message":{"model":"<synthetic>","role":"assistant","type":"message"},"error":"authentication_failed"}"#,
        "\n",
        r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login","terminal_reason":"api_error"}"#,
        "\n",
    );

    /// [01 §3.5](../../../docs/prd/01-agent-runtime.md)：「已装未登录」必须报 `agent.not_authenticated`。
    /// 修正前分类器只读 stderr，而这份样本的 stderr 是空的——界面因此显示 `agent.spawn_failed`，
    /// 「去登录」和「不知道出了什么事」变成同一句话。
    #[test]
    fn auth_failure_is_classified_from_stream_json() {
        assert_eq!(
            classify_process_failure(UNAUTHENTICATED_STDOUT, "").code,
            "agent.not_authenticated"
        );

        // **不能按 `subtype` 判**：这条终结事件明明 `is_error`，`subtype` 却仍写着 `success`。
        assert!(UNAUTHENTICATED_STDOUT.contains(r#""subtype":"success""#));

        // stderr 那条老路不能因此失效。
        assert_eq!(
            classify_process_failure("", "Not logged in").code,
            "agent.not_authenticated"
        );
        assert_eq!(
            classify_process_failure("", "usage limit reached").code,
            "agent.quota_exhausted"
        );

        // 成功的解析输出里本来就会出现账目文本，不得被当成失败信号。
        let succeeded = concat!(
            r#"{"type":"result","subtype":"success","is_error":false,"result":"已起草 3 条，合计 168.00 AUD"}"#,
            "\n",
        );
        assert_eq!(
            classify_process_failure(succeeded, "").code,
            "agent.spawn_failed"
        );
    }

    #[test]
    fn probe_covers_builtin_tools() {
        let stream = serde_json::json!({
            "type": "system",
            "subtype": "init",
            "tools": ["mcp__daybook__read_source", "Bash"],
            "plugins": [],
            "skills": [],
            "agents": [],
            "slash_commands": [],
            "memory_paths": [],
            "permissionMode": "default"
        })
        .to_string();
        let manifest = parse_capability_manifest(&stream).unwrap();
        assert!(manifest.iter().any(|entry| entry.provider == "builtin"));
    }

    #[test]
    fn probe_covers_non_tool_capabilities() {
        let stream = [
            serde_json::json!({
                "type": "system",
                "subtype": "init",
                "tools": ["mcp__daybook__read_source"],
                "plugins": [],
                "skills": [],
                "agents": [],
                "slash_commands": [],
                "memory_paths": [],
                "permissionMode": "bypassPermissions"
            })
            .to_string(),
            serde_json::json!({ "type": "hook_started" }).to_string(),
        ]
        .join("\n");
        let manifest = parse_capability_manifest(&stream).unwrap();
        assert!(manifest.iter().any(|entry| entry.kind == "hook"));
        assert!(manifest.iter().any(|entry| entry.kind == "permission_mode"));
    }

    #[test]
    fn tool_surface_probe_is_structured() {
        assert_eq!(
            parse_capability_manifest("the model says it has five tools")
                .unwrap_err()
                .code,
            "agent.tool_surface_unsealed"
        );
    }

    #[cfg(unix)]
    fn write_cli(path: &Path, body: &str, executable: bool) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, body).unwrap();
        let mode = if executable { 0o755 } else { 0o644 };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[cfg(unix)]
    async fn qualify_one(candidate: PathBuf) -> BackendStatus {
        ClaudeCodeBackend::with_candidates(vec![candidate], PathBuf::from("daybook-mcp"))
            .with_version_timeout(Duration::from_millis(1000))
            .status()
            .await
    }

    /// **01 §3.5**：候选跟随符号链接后是普通文件、有执行权限，且 `--version` 在限定时间内
    /// 以 0 退出并返回非空版本，才算合格安装。
    ///
    /// 修正前这里只有 `is_file()`——下面第一档（普通但不可执行的文件）会被判成「已安装」，
    /// 而三种失败给的是同一句「未安装 Claude Code」，用户照着指引重装也修不好。
    #[cfg(unix)]
    #[tokio::test]
    async fn installation_qualification_requires_executable_version() {
        let home = tempfile::tempdir().unwrap();

        let missing = home.path().join("absent-claude");
        assert_eq!(
            qualify_one(missing).await.availability_reason.as_deref(),
            Some("not_found")
        );

        let plain = home.path().join("plain-claude");
        write_cli(&plain, "#!/bin/sh\necho 2.1.229\n", false);
        let status = qualify_one(plain).await;
        assert!(!status.available, "没有执行权限的文件不构成合格安装");
        assert_eq!(
            status.availability_reason.as_deref(),
            Some("not_executable")
        );

        let failing = home.path().join("failing-claude");
        write_cli(&failing, "#!/bin/sh\nexit 1\n", true);
        assert_eq!(
            qualify_one(failing).await.availability_reason.as_deref(),
            Some("version_unreadable"),
            "--version 非零退出"
        );

        let slow = home.path().join("slow-claude");
        write_cli(&slow, "#!/bin/sh\nsleep 30\necho 2.1.229\n", true);
        assert_eq!(
            qualify_one(slow).await.availability_reason.as_deref(),
            Some("version_unreadable"),
            "--version 超时"
        );

        let silent = home.path().join("silent-claude");
        write_cli(&silent, "#!/bin/sh\nexit 0\n", true);
        assert_eq!(
            qualify_one(silent).await.availability_reason.as_deref(),
            Some("version_unreadable"),
            "--version 输出为空"
        );

        let good = home.path().join("claude");
        write_cli(&good, "#!/bin/sh\necho 2.1.229\n", true);
        let status = qualify_one(good.clone()).await;
        assert!(status.available, "{status:?}");
        assert_eq!(status.availability_reason, None);
        assert_eq!(status.version.as_deref(), Some("2.1.229"));
        assert_eq!(status.executable, Some(good));
        assert!(!status.ready, "合格安装只是安装资格，不是解析就绪度");
    }

    /// 版本管理器与 `~/.local/bin` 里的 `claude` 常常是符号链接——**跟到底再判**，
    /// 但断链、指向目录、指向不可执行文件各自按对应原因拒绝。
    #[cfg(unix)]
    #[tokio::test]
    async fn installation_qualification_follows_symlinks() {
        let home = tempfile::tempdir().unwrap();
        let real = home.path().join("real-claude");
        write_cli(&real, "#!/bin/sh\necho 2.1.229\n", true);

        let linked = home.path().join("linked-claude");
        std::os::unix::fs::symlink(&real, &linked).unwrap();
        let status = qualify_one(linked.clone()).await;
        assert!(status.available, "指向合格可执行文件的符号链接必须被接受");
        assert_eq!(status.executable, Some(linked));
        assert_eq!(status.version.as_deref(), Some("2.1.229"));

        let dangling = home.path().join("dangling-claude");
        std::os::unix::fs::symlink(home.path().join("gone"), &dangling).unwrap();
        assert_eq!(
            qualify_one(dangling).await.availability_reason.as_deref(),
            Some("not_found")
        );

        let to_directory = home.path().join("directory-claude");
        std::os::unix::fs::symlink(home.path(), &to_directory).unwrap();
        assert_eq!(
            qualify_one(to_directory)
                .await
                .availability_reason
                .as_deref(),
            Some("not_executable")
        );

        let plain = home.path().join("plain");
        write_cli(&plain, "#!/bin/sh\necho 2.1.229\n", false);
        let to_plain = home.path().join("plain-link-claude");
        std::os::unix::fs::symlink(&plain, &to_plain).unwrap();
        assert_eq!(
            qualify_one(to_plain).await.availability_reason.as_deref(),
            Some("not_executable")
        );
    }

    #[tokio::test]
    async fn status_does_not_require_credentials() {
        let backend = ClaudeCodeBackend::with_paths(None, PathBuf::from("daybook-mcp"));
        let status = backend.status().await;
        assert!(!status.available);
        assert_eq!(status.availability_reason.as_deref(), Some("not_found"));
        assert_eq!(
            status.error_code.as_deref(),
            Some("agent.backend_unavailable")
        );
        assert!(!status.ready, "安装资格不等于就绪度");
    }

    fn fingerprint_backend() -> ClaudeCodeBackend {
        ClaudeCodeBackend::with_paths(
            Some(PathBuf::from("/opt/homebrew/bin/claude")),
            PathBuf::from("/Applications/Daybook.app/Contents/MacOS/daybook-mcp"),
        )
    }

    #[test]
    fn fingerprint_is_derived_from_the_real_command() {
        // 指纹此前是手写字面量：改 sealed_command 的 flag 它纹丝不动。
        let contract = fingerprint_backend().sealed_config_contract().to_string();
        for flag in [
            "--tools",
            "--agents",
            "--strict-mcp-config",
            "--setting-sources",
            "--no-session-persistence",
            "--disable-slash-commands",
            "--include-hook-events",
            "--no-chrome",
            "--allowedTools",
            "--mcp-config",
        ] {
            assert!(contract.contains(flag), "{flag} 不在密封指纹的输入里");
        }
        assert!(contract.contains("CLAUDE_CODE_MAX_SUBAGENTS_PER_SESSION=0"));
        for tool in m0_tool_registry() {
            assert!(contract.contains(&format!("mcp__daybook__{}", tool.name)));
        }
    }

    #[test]
    fn fingerprint_moves_when_the_seal_is_loosened() {
        let sealed = fingerprint_backend().sealed_config_contract();
        let mut loosened = Command::new("<claude>");
        seal(&mut loosened);
        loosened.arg("--dangerously-skip-permissions");
        let loosened_args = loosened
            .as_std()
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_ne!(sealed["args"], serde_json::to_value(loosened_args).unwrap());
    }

    #[test]
    fn fingerprint_does_not_drift_with_install_location() {
        // 会话 token、socket 路径与 helper 所在目录都不该进指纹。
        let packaged = fingerprint_backend();
        let development = ClaudeCodeBackend::with_paths(
            Some(PathBuf::from("/usr/local/bin/claude")),
            PathBuf::from("/Users/someone/repo/src-tauri/target/debug/daybook-mcp"),
        );
        assert_eq!(
            packaged.sealed_config_fingerprint(),
            development.sealed_config_fingerprint()
        );
        assert_eq!(packaged.sealed_config_fingerprint().len(), 64);
    }

    #[test]
    fn discovery_survives_the_finder_launched_path() {
        // 打包后的 .app 从 Finder 起来时 PATH 里没有任何 node 版本管理器的目录。
        let home = tempfile::tempdir().unwrap();
        let nvm_bin = home.path().join(".nvm/versions/node/v20.11.0/bin");
        std::fs::create_dir_all(&nvm_bin).unwrap();
        std::fs::write(nvm_bin.join("claude"), b"#!/bin/sh\n").unwrap();

        let finder_path = Some(std::ffi::OsString::from("/usr/bin:/bin:/usr/sbin:/sbin"));
        let candidates = discover_claude_in(finder_path, Some(home.path().to_path_buf()));
        assert!(
            candidates.contains(&nvm_bin.join("claude")),
            "{candidates:?}"
        );
    }

    #[test]
    fn discovery_prefers_the_newest_node_version() {
        let home = tempfile::tempdir().unwrap();
        for version in ["v18.19.0", "v20.11.0"] {
            let bin = home
                .path()
                .join(".nvm/versions/node")
                .join(version)
                .join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            std::fs::write(bin.join("claude"), b"#!/bin/sh\n").unwrap();
        }
        let candidates = discover_claude_in(
            Some(std::ffi::OsString::new()),
            Some(home.path().to_path_buf()),
        );
        let first_nvm = candidates
            .iter()
            .find(|candidate| candidate.to_string_lossy().contains(".nvm"))
            .expect("nvm 位置应当在候选里");
        assert!(
            first_nvm.to_string_lossy().contains("v20.11.0"),
            "{first_nvm:?}"
        );
    }

    /// 枚举出候选**不等于**装了：一个都不合格时状态必须是 `not_found`，
    /// 而不是拿第一个候选路径去冒充安装。
    #[tokio::test]
    async fn discovery_reports_absence_instead_of_guessing() {
        let home = tempfile::tempdir().unwrap();
        let candidates = discover_claude_in(
            Some(std::ffi::OsString::new()),
            Some(home.path().to_path_buf()),
        );
        let backend = ClaudeCodeBackend::with_candidates(candidates, PathBuf::from("daybook-mcp"))
            .with_version_timeout(Duration::from_millis(200));
        let status = backend.status().await;
        assert!(!status.available);
        assert_eq!(status.availability_reason.as_deref(), Some("not_found"));
    }

    #[test]
    fn inert_agent_definitions_are_not_effective_capabilities() {
        let inert = serde_json::json!({
            "type": "system",
            "subtype": "init",
            "tools": ["mcp__daybook__read_source"],
            "plugins": [],
            "skills": [],
            "agents": ["general-purpose"],
            "slash_commands": [],
            "memory_paths": [],
            "permissionMode": "default"
        })
        .to_string();
        assert!(parse_capability_manifest(&inert)
            .unwrap()
            .iter()
            .all(|entry| entry.kind != "agent"));

        let callable = inert.replace(
            "\"mcp__daybook__read_source\"",
            "\"mcp__daybook__read_source\",\"Agent\"",
        );
        assert!(parse_capability_manifest(&callable)
            .unwrap()
            .iter()
            .any(|entry| entry.kind == "agent"));
    }
}
