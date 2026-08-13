use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::watch;

use crate::{db::Database, error::AppResult};

use super::registry::CapabilityEntry;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendStatus {
    pub backend_id: String,
    pub executable: Option<PathBuf>,
    pub version: Option<String>,
    pub available: bool,
    pub authenticated: Option<bool>,
    pub error_code: Option<String>,
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
    fn status(&self) -> BackendStatus;
    async fn probe_cache_key(&self) -> AppResult<String> {
        let status = self.status();
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
