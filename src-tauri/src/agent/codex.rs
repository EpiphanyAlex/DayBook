use std::{path::PathBuf, sync::Arc, time::Duration};

use async_trait::async_trait;
use tokio::{process::Command, sync::watch, time::timeout};

use crate::{
    db::Database,
    error::{AppError, AppResult},
};

use serde_json::json;

use super::{
    backend::{
        AgentBackend, AgentTask, AgentTaskResult, AvailabilityReason, BackendStatus,
        ModelCandidate, ProbeResult,
    },
    rpc::RpcClient,
    selection::ModelSelection,
};

#[derive(Debug, Default)]
struct Inspection {
    authenticated: Option<bool>,
    quota: &'static str,
    models: Option<Vec<ModelCandidate>>,
}

/// 0.154.0 的协议查询只证明部分配置可见，不能证明当前 turn 的完整有效能力面。
/// 因此这里可报告安装资格，但绝不创建解析 session 或把假 CLI 成功映射成 ready。
#[derive(Debug)]
pub struct CodexBackend {
    candidates: Vec<PathBuf>,
    _model_selection: ModelSelection,
}

impl CodexBackend {
    pub fn discover(model_selection: ModelSelection) -> Self {
        let mut candidates = std::env::var_os("PATH")
            .map(|paths| {
                std::env::split_paths(&paths)
                    .map(|directory| directory.join("codex"))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            candidates.extend([
                home.join(".local/bin/codex"),
                home.join(".npm-global/bin/codex"),
                home.join(".volta/bin/codex"),
            ]);
        }
        candidates.extend([
            PathBuf::from("/opt/homebrew/bin/codex"),
            PathBuf::from("/usr/local/bin/codex"),
        ]);
        Self {
            candidates,
            _model_selection: model_selection,
        }
    }

    #[cfg(test)]
    fn with_executable(path: PathBuf) -> Self {
        Self {
            candidates: vec![path],
            _model_selection: ModelSelection::Auto,
        }
    }

    async fn inspect(path: &std::path::Path) -> Inspection {
        let mut result = Inspection {
            quota: "unknown",
            ..Inspection::default()
        };
        let Ok(mut rpc) = RpcClient::spawn(path) else {
            return result;
        };
        if rpc.initialize().await.is_ok() {
            if let Ok(account) = rpc
                .request("account/read", json!({"refreshToken":false}))
                .await
            {
                result.authenticated = if account
                    .get("account")
                    .is_some_and(|value| value.is_object())
                {
                    Some(true)
                } else if account
                    .get("requiresOpenaiAuth")
                    .and_then(|value| value.as_bool())
                    == Some(true)
                    && account.get("account").is_some_and(|value| value.is_null())
                {
                    Some(false)
                } else {
                    None
                };
            }
            if let Ok(limits) = rpc.request("account/rateLimits/read", json!({})).await {
                result.quota = match limits
                    .get("ordinaryUsageAllowed")
                    .and_then(|value| value.as_bool())
                {
                    Some(true) => "available",
                    Some(false) => "exhausted",
                    None => "unknown",
                };
            }
            let mut cursor: Option<String> = None;
            let mut models = Vec::new();
            let mut complete = false;
            for _ in 0..4 {
                let response = rpc.request("model/list", json!({"cursor":cursor})).await;
                let Ok(response) = response else {
                    break;
                };
                let Some(data) = response.get("data").and_then(|value| value.as_array()) else {
                    break;
                };
                for item in data {
                    let (Some(model_id), Some(display_name)) = (
                        item.get("model").and_then(|value| value.as_str()),
                        item.get("displayName").and_then(|value| value.as_str()),
                    ) else {
                        continue;
                    };
                    let supports_images = item
                        .get("inputModalities")
                        .and_then(|value| value.as_array())
                        .map(|values| values.iter().any(|value| value.as_str() == Some("image")));
                    models.push(ModelCandidate {
                        model_id: model_id.to_owned(),
                        display_name: display_name.to_owned(),
                        supports_images,
                    });
                }
                cursor = response
                    .get("nextCursor")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned);
                if cursor.is_none() {
                    complete = true;
                    break;
                }
            }
            if complete {
                result.models = Some(models);
            }
        }
        rpc.close().await;
        result
    }
}

#[async_trait]
impl AgentBackend for CodexBackend {
    fn id(&self) -> &'static str {
        "codex"
    }

    async fn status(&self) -> BackendStatus {
        let mut reason = AvailabilityReason::NotFound;
        for path in &self.candidates {
            let resolved = match std::fs::canonicalize(path) {
                Ok(path) => path,
                Err(_) => continue,
            };
            let metadata = match std::fs::metadata(&resolved) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            if !metadata.is_file() {
                reason = reason.worse_of(AvailabilityReason::NotExecutable);
                continue;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o111 == 0 {
                    reason = reason.worse_of(AvailabilityReason::NotExecutable);
                    continue;
                }
            }
            let result = timeout(
                Duration::from_secs(5),
                Command::new(path).arg("--version").output(),
            )
            .await;
            match result {
                Ok(Ok(output)) if output.status.success() && !output.stdout.is_empty() => {
                    let mut status = BackendStatus::qualified(
                        self.id(),
                        path.clone(),
                        String::from_utf8_lossy(&output.stdout).trim().to_owned(),
                    );
                    let inspection = Self::inspect(path).await;
                    status.authenticated = inspection.authenticated;
                    status.quota = inspection.quota.to_owned();
                    status.models = inspection.models;
                    return status;
                }
                _ => reason = reason.worse_of(AvailabilityReason::VersionUnreadable),
            }
        }
        BackendStatus::unqualified(self.id(), reason)
    }

    async fn probe(&self, _database: Arc<Database>) -> AppResult<ProbeResult> {
        Err(AppError::new(
            "agent.tool_surface_unsealed",
            "Codex 尚不能证明完整有效能力面，解析已暂停",
        ))
    }

    async fn run_task(
        &self,
        _database: Arc<Database>,
        _task: AgentTask,
        _cancel: watch::Receiver<bool>,
    ) -> AppResult<AgentTaskResult> {
        Err(AppError::new(
            "agent.tool_surface_unsealed",
            "Codex 权限门槛未通过，不下发来源",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{runtime::AgentRuntime, selection::AgentSelection};

    #[cfg(unix)]
    #[tokio::test]
    async fn read_only_catalog_never_unlocks_parse() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("codex");
        let script = r##"#!/bin/sh
if [ "$1" = "--version" ]; then echo codex-cli-0.154.0; exit 0; fi
read init
printf '%s\n' '{"id":1,"result":{}}'
read initialized
read account
printf '%s\n' '{"id":2,"result":{"requiresOpenaiAuth":true,"account":{"type":"chatgpt"}}}'
read rate
printf '%s\n' '{"id":3,"result":{"ordinaryUsageAllowed":true}}'
read models
printf '%s\n' '{"id":4,"result":{"data":[{"model":"gpt-test","displayName":"Test model","inputModalities":["text","image"]}],"nextCursor":null}}'
sleep 5
"##;
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let backend = CodexBackend::with_executable(path);
        let status = backend.status().await;
        assert!(status.available);
        assert_eq!(status.authenticated, Some(true));
        assert_eq!(status.quota, "available");
        assert_eq!(status.models.as_ref().unwrap()[0].model_id, "gpt-test");
        assert_eq!(
            backend
                .probe(Arc::new(
                    Database::open(directory.path().join("data")).unwrap()
                ))
                .await
                .unwrap_err()
                .code,
            "agent.tool_surface_unsealed"
        );
        let runtime = AgentRuntime::with_selection(
            Arc::new(backend),
            AgentSelection {
                backend_id: "codex".to_owned(),
                model_selection: ModelSelection::Auto,
            },
        );
        let database = Arc::new(Database::open(directory.path().join("data")).unwrap());
        database.set_base_currency("AUD").unwrap();
        assert_eq!(
            runtime
                .parse_source(Arc::clone(&database), "not-an-imported-source".to_owned())
                .await
                .unwrap_err()
                .code,
            "agent.not_ready"
        );
        let attempts = database
            .read(|connection| {
                connection.query_row("SELECT COUNT(*) FROM parse_attempts", [], |row| {
                    row.get::<_, i64>(0)
                })
            })
            .unwrap();
        assert_eq!(attempts, 0);
    }
}
