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
    backend::{AgentBackend, AgentTask, AgentTaskResult, BackendStatus, ProbeResult},
    registry::{
        effective_capability_hash, expected_capabilities, m0_tool_registry, CapabilityEntry,
    },
    session::{AgentSession, SessionMode},
};

const PROBE_TIMEOUT: Duration = Duration::from_secs(45);
const TASK_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone)]
pub struct ClaudeCodeBackend {
    executable: Option<PathBuf>,
    helper_path: PathBuf,
}

impl ClaudeCodeBackend {
    pub fn discover(helper_path: PathBuf) -> Self {
        Self {
            executable: discover_claude(),
            helper_path,
        }
    }

    pub fn with_paths(executable: Option<PathBuf>, helper_path: PathBuf) -> Self {
        Self {
            executable,
            helper_path,
        }
    }

    fn executable(&self) -> AppResult<&Path> {
        self.executable.as_deref().ok_or_else(|| {
            AppError::new(
                "agent.backend_unavailable",
                "未检测到 Claude Code CLI，请先安装后再解析",
            )
        })
    }

    async fn version(&self) -> AppResult<String> {
        let output = timeout(
            Duration::from_secs(5),
            Command::new(self.executable()?)
                .arg("--version")
                .stdin(Stdio::null())
                .output(),
        )
        .await
        .map_err(|_| AppError::new("agent.backend_unavailable", "Claude Code 版本探测超时"))?
        .map_err(|error| AppError::new("agent.backend_unavailable", error.to_string()))?;
        if !output.status.success() {
            return Err(AppError::new(
                "agent.backend_unavailable",
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    fn sealed_command(
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
        let allowed_tools = m0_tool_registry()
            .into_iter()
            .map(|tool| format!("mcp__daybook__{}", tool.name))
            .collect::<Vec<_>>()
            .join(",");
        let mut command = Command::new(self.executable()?);
        command
            .arg("-p")
            .arg(prompt)
            .arg("--tools")
            .arg("")
            .arg("--agents")
            .arg("{}")
            .arg("--strict-mcp-config")
            .arg("--mcp-config")
            .arg(config.to_string())
            .arg("--setting-sources")
            .arg("")
            .arg("--no-session-persistence")
            .arg("--disable-slash-commands")
            .arg("--allowedTools")
            .arg(allowed_tools)
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose")
            .arg("--include-hook-events")
            .arg("--no-chrome")
            .env("DISABLE_AUTOUPDATER", "1")
            .env("CLAUDE_CODE_DISABLE_AGENT_VIEW", "1")
            .env("CLAUDE_CODE_DISABLE_EXPLORE_PLAN_AGENTS", "1")
            .env("CLAUDE_CODE_MAX_SUBAGENTS_PER_SESSION", "0")
            .env("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1")
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
        let mut command = self.sealed_command(session, prompt, agent_session_id)?;
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
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(classify_process_error(&stderr));
        }
        Ok(output)
    }

    fn sealed_config_fingerprint(&self) -> String {
        let contract = json!({
            "backend": self.id(),
            "flags": [
                "-p", "--tools=", "--agents={}",
                "--strict-mcp-config", "--setting-sources=",
                "--no-session-persistence", "--disable-slash-commands",
                "--output-format=stream-json", "--verbose", "--include-hook-events",
                "--no-chrome"
            ],
            "environment": {
                "DISABLE_AUTOUPDATER": "1",
                "CLAUDE_CODE_DISABLE_AGENT_VIEW": "1",
                "CLAUDE_CODE_DISABLE_EXPLORE_PLAN_AGENTS": "1",
                "CLAUDE_CODE_MAX_SUBAGENTS_PER_SESSION": "0",
                "CLAUDE_CODE_DISABLE_AUTO_MEMORY": "1"
            },
            "allowedTools": m0_tool_registry()
                .into_iter()
                .map(|tool| format!("mcp__daybook__{}", tool.name))
                .collect::<Vec<_>>(),
            "helper": self.helper_path.file_name().and_then(|name| name.to_str()),
        });
        format!("{:x}", Sha256::digest(contract.to_string().as_bytes()))
    }
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

    fn status(&self) -> BackendStatus {
        BackendStatus {
            backend_id: self.id().to_owned(),
            executable: self.executable.clone(),
            version: None,
            available: self.executable.is_some(),
            authenticated: None,
            error_code: self
                .executable
                .is_none()
                .then(|| "agent.backend_unavailable".to_owned()),
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

fn classify_process_error(stderr: &str) -> AppError {
    let normalized = stderr.to_ascii_lowercase();
    if normalized.contains("not logged in")
        || normalized.contains("authentication")
        || normalized.contains("login required")
    {
        return AppError::new(
            "agent.not_authenticated",
            "Claude Code 已安装但尚未登录，请先在终端完成登录",
        )
        .with_detail(json!({ "stderr": stderr.trim() }));
    }
    if normalized.contains("quota")
        || normalized.contains("usage limit")
        || normalized.contains("rate limit")
    {
        return AppError::new("agent.quota_exhausted", "Claude Code 当前额度不足")
            .with_detail(json!({ "stderr": stderr.trim() }));
    }
    AppError::new(
        "agent.spawn_failed",
        if stderr.trim().is_empty() {
            "Claude Code 子进程异常退出".to_owned()
        } else {
            stderr.trim().to_owned()
        },
    )
    .with_detail(json!({ "stderr": stderr.trim() }))
}

fn discover_claude() -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut candidates = std::env::split_paths(&path)
        .map(|directory| directory.join("claude"))
        .collect::<Vec<_>>();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        candidates.push(home.join(".local/bin/claude"));
        candidates.push(home.join(".claude/local/claude"));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin/claude"));
    candidates.push(PathBuf::from("/usr/local/bin/claude"));
    candidates.into_iter().find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod agent {
    use super::*;

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

    #[test]
    fn status_does_not_require_credentials() {
        let backend = ClaudeCodeBackend::with_paths(None, PathBuf::from("daybook-mcp"));
        let status = backend.status();
        assert!(!status.available);
        assert_eq!(
            status.error_code.as_deref(),
            Some("agent.backend_unavailable")
        );
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
