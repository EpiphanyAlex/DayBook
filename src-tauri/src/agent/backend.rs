use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::watch;

use crate::{db::Database, error::AppResult};

use super::registry::CapabilityEntry;

/// 安装资格失败的**稳定**原因（[01 §3.5](../../../docs/prd/01-agent-runtime.md)）。
///
/// 前端按这个枚举给安装或修复指引——**三者不是同一句话**：没找到该去装，
/// 不可执行该去 `chmod`，版本读不出来该去修那个安装本身。**不得解析错误文案**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvailabilityReason {
    NotFound,
    NotExecutable,
    VersionUnreadable,
}

impl AvailabilityReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::NotExecutable => "not_executable",
            Self::VersionUnreadable => "version_unreadable",
        }
    }

    /// 「走得最远的那次失败」优先——它指向用户最可能真正想用的那个安装。
    /// 一个 `PATH` 里根本不存在的候选，不该盖过一个装好了却少了执行位的候选。
    fn rank(self) -> u8 {
        match self {
            Self::NotFound => 0,
            Self::NotExecutable => 1,
            Self::VersionUnreadable => 2,
        }
    }

    pub fn worse_of(self, other: Self) -> Self {
        if other.rank() > self.rank() {
            other
        } else {
            self
        }
    }
}

/// **安装资格与解析就绪度是两件事**（[01 §3.5](../../../docs/prd/01-agent-runtime.md)）。
///
/// `available` 只回答「有没有一个合格的 CLI」，`ready` 只回答「这次生命周期里完整
/// readiness probe 成没成功」。**前端不得从 `available && error_code == null` 反推 `ready`**
/// ——那正是修正前的实现，probe 还没跑完界面就说「已就绪」。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendStatus {
    pub backend_id: String,
    pub executable: Option<PathBuf>,
    pub version: Option<String>,
    pub available: bool,
    pub availability_reason: Option<String>,
    pub authenticated: Option<bool>,
    pub ready: bool,
    pub error_code: Option<String>,
}

impl BackendStatus {
    /// 没有合格候选：`available = false` + 稳定原因 + `agent.backend_unavailable`。
    pub fn unqualified(backend_id: &str, reason: AvailabilityReason) -> Self {
        Self {
            backend_id: backend_id.to_owned(),
            executable: None,
            version: None,
            available: false,
            availability_reason: Some(reason.as_str().to_owned()),
            authenticated: None,
            ready: false,
            error_code: Some("agent.backend_unavailable".to_owned()),
        }
    }

    /// 合格安装。**就绪度不在这里判**——它由 `AgentRuntime` 按最近一次 probe 合成。
    pub fn qualified(backend_id: &str, executable: PathBuf, version: String) -> Self {
        Self {
            backend_id: backend_id.to_owned(),
            executable: Some(executable),
            version: Some(version),
            available: true,
            availability_reason: None,
            authenticated: None,
            ready: false,
            error_code: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub backend_id: String,
    pub backend_version: String,
    pub manifest: std::collections::BTreeSet<CapabilityEntry>,
    pub effective_capability_hash: String,
    pub sealed_config_fingerprint: String,
}

#[derive(Debug, Clone)]
pub struct AgentTask {
    pub source_id: String,
    pub attempt_id: String,
    pub agent_session_id: String,
    pub task_prompt: String,
}

#[derive(Debug, Clone)]
pub struct AgentTaskResult {
    pub success: bool,
    pub model_id: Option<String>,
    pub agent_session_id: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub trace_events: Vec<Value>,
    pub debug_events: Vec<Value>,
}

#[async_trait]
pub trait AgentBackend: Send + Sync {
    fn id(&self) -> &'static str;
    /// **异步**：安装资格里包含「`--version` 限时以 0 退出且输出非空」，那是一次真实的
    /// 子进程调用（[01 §3.5](../../../docs/prd/01-agent-runtime.md)）。实现方自己缓存结果。
    async fn status(&self) -> BackendStatus;
    async fn probe_cache_key(&self) -> AppResult<String> {
        let status = self.status().await;
        Ok(format!(
            "{}:{}",
            self.id(),
            status.version.as_deref().unwrap_or("unknown")
        ))
    }
    async fn probe(&self, database: Arc<Database>) -> AppResult<ProbeResult>;
    async fn run_task(
        &self,
        database: Arc<Database>,
        task: AgentTask,
        cancel: watch::Receiver<bool>,
    ) -> AppResult<AgentTaskResult>;
}
