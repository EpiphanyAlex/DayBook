use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::oneshot,
};
use uuid::Uuid;

use crate::{
    db::Database,
    domain::draft::{Assignment, DraftStore, ToolOutput},
    error::{AppError, AppResult},
};

pub const SOCKET_ENV: &str = "DAYBOOK_MCP_SOCKET";
pub const TOKEN_ENV: &str = "DAYBOOK_MCP_TOKEN";

#[derive(Debug, Clone)]
pub enum SessionMode {
    Probe,
    Task(Assignment),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireRequest {
    token: String,
    tool: String,
    arguments: Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireResponse {
    result: Option<ToolOutput>,
    error: Option<AppError>,
}

#[derive(Debug)]
pub struct AgentSession {
    socket_path: PathBuf,
    socket_directory: PathBuf,
    token: String,
    store: Arc<DraftStore>,
    shutdown: Mutex<Option<oneshot::Sender<()>>>,
}

impl AgentSession {
    pub async fn start(database: Arc<Database>, mode: SessionMode) -> AppResult<Self> {
        let (socket_directory, socket_path) = short_socket_paths();
        fs::create_dir(&socket_directory)?;
        fs::set_permissions(&socket_directory, fs::Permissions::from_mode(0o700))?;
        if socket_path.as_os_str().len() >= 100 {
            return Err(AppError::new(
                "agent.spawn_failed",
                "Unix socket 路径超过 macOS SUN_LEN 安全上限",
            ));
        }
        let listener = UnixListener::bind(&socket_path)
            .map_err(|error| AppError::new("agent.spawn_failed", error.to_string()))?;
        let store = Arc::new(match mode {
            SessionMode::Probe => DraftStore::for_probe(database),
            SessionMode::Task(assignment) => DraftStore::for_task(database, assignment),
        });
        let token = Uuid::new_v4().to_string();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let listener_store = Arc::clone(&store);
        let listener_token = token.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((stream, _)) => {
                                let store = Arc::clone(&listener_store);
                                let token = listener_token.clone();
                                tauri::async_runtime::spawn(async move {
                                    let _ = serve_connection(stream, &token, &store).await;
                                });
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
        });
        Ok(Self {
            socket_path,
            socket_directory,
            token,
            store,
            shutdown: Mutex::new(Some(shutdown_tx)),
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn is_complete(&self) -> bool {
        self.store.is_complete()
    }

    pub fn trace_events(&self) -> Vec<Value> {
        self.store.trace_events()
    }

    pub fn debug_events(&self) -> Vec<Value> {
        self.store.debug_events()
    }
}

fn short_socket_paths() -> (PathBuf, PathBuf) {
    let suffix = Uuid::new_v4().simple().to_string();
    let runtime_root = std::env::var_os("DAYBOOK_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let directory = runtime_root.join(format!("dbk-{}-{suffix}", std::process::id()));
    let socket = directory.join("m.sock");
    (directory, socket)
}

impl Drop for AgentSession {
    fn drop(&mut self) {
        if let Ok(mut shutdown) = self.shutdown.lock() {
            if let Some(sender) = shutdown.take() {
                let _ = sender.send(());
            }
        }
        let _ = fs::remove_file(&self.socket_path);
        let _ = fs::remove_dir(&self.socket_directory);
    }
}

pub async fn forward_to_main(tool: &str, arguments: Value) -> AppResult<ToolOutput> {
    let socket_path = std::env::var(SOCKET_ENV)
        .map_err(|_| AppError::new("agent.spawn_failed", "MCP helper 缺少 socket 环境变量"))?;
    let token = std::env::var(TOKEN_ENV)
        .map_err(|_| AppError::new("agent.spawn_failed", "MCP helper 缺少 session token"))?;
    let stream = UnixStream::connect(socket_path)
        .await
        .map_err(|error| AppError::new("agent.spawn_failed", error.to_string()))?;
    let (read_half, mut write_half) = stream.into_split();
    let request = WireRequest {
        token,
        tool: tool.to_owned(),
        arguments,
    };
    let mut bytes = serde_json::to_vec(&request)
        .map_err(|error| AppError::storage(format!("MCP 请求序列化失败：{error}")))?;
    bytes.push(b'\n');
    write_half.write_all(&bytes).await?;
    write_half.shutdown().await?;
    let mut line = String::new();
    BufReader::new(read_half).read_line(&mut line).await?;
    let response: WireResponse = serde_json::from_str(&line)
        .map_err(|error| AppError::storage(format!("MCP 响应反序列化失败：{error}")))?;
    match (response.result, response.error) {
        (Some(result), None) => Ok(result),
        (None, Some(error)) => Err(error),
        _ => Err(AppError::storage("MCP 响应同时缺少结果与错误")),
    }
}

async fn serve_connection(
    stream: UnixStream,
    expected_token: &str,
    store: &DraftStore,
) -> AppResult<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut line = String::new();
    BufReader::new(read_half).read_line(&mut line).await?;
    let request: WireRequest = serde_json::from_str(&line)
        .map_err(|error| AppError::invalid_argument(format!("UDS 请求不是合法 JSON：{error}")))?;
    let response = if request.token != expected_token {
        WireResponse {
            result: None,
            error: Some(AppError::new(
                "agent.tool_rejected",
                "MCP session token 不匹配",
            )),
        }
    } else {
        match store.handle(&request.tool, request.arguments) {
            Ok(result) => WireResponse {
                result: Some(result),
                error: None,
            },
            Err(error) => WireResponse {
                result: None,
                error: Some(error),
            },
        }
    };
    let mut bytes = serde_json::to_vec(&response)
        .map_err(|error| AppError::storage(format!("UDS 响应序列化失败：{error}")))?;
    bytes.push(b'\n');
    write_half.write_all(&bytes).await?;
    write_half.shutdown().await?;
    Ok(())
}

#[cfg(test)]
mod agent {
    use std::sync::Arc;

    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn uds_path_stays_below_sun_len() {
        let (_, socket_path) = short_socket_paths();
        assert!(socket_path.as_os_str().len() < 100);
    }

    #[tokio::test]
    async fn invalid_session_token_is_rejected() {
        let directory = tempdir().unwrap();
        let database = Arc::new(Database::open(directory.path()).unwrap());
        let store = DraftStore::for_probe(database);
        let (client, server) = UnixStream::pair().unwrap();
        let server_task = tokio::spawn(async move {
            serve_connection(server, "correct", &store).await.unwrap();
        });
        let (read_half, mut write_half) = client.into_split();
        let mut bytes = serde_json::to_vec(&WireRequest {
            token: "wrong".to_owned(),
            tool: "list_pending_sources".to_owned(),
            arguments: serde_json::json!({}),
        })
        .unwrap();
        bytes.push(b'\n');
        write_half.write_all(&bytes).await.unwrap();
        write_half.shutdown().await.unwrap();
        let mut line = String::new();
        BufReader::new(read_half)
            .read_line(&mut line)
            .await
            .unwrap();
        let response: WireResponse = serde_json::from_str(&line).unwrap();
        assert_eq!(response.error.unwrap().code, "agent.tool_rejected");
        server_task.await.unwrap();
    }
}
