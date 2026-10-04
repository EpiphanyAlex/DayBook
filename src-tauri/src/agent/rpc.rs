#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::Path,
    process::Stdio,
    time::Duration,
};

use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    time::{timeout, timeout_at, Instant},
};

use crate::error::{AppError, AppResult};

use super::process::terminate_child_tree;

const MAX_LINE: usize = 1024 * 1024;
const MAX_QUEUE_COUNT: usize = 128;
const MAX_QUEUE_BYTES: usize = 4 * 1024 * 1024;
const REQUEST_DEADLINE: Duration = Duration::from_secs(15);
const REVERSE_DEADLINE: Duration = Duration::from_secs(5);
const REAP_DEADLINE: Duration = Duration::from_secs(3);

pub struct RpcClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    outstanding: HashSet<u64>,
    responses: HashMap<u64, (Value, usize)>,
    notifications: VecDeque<(Value, usize)>,
    queue_bytes: usize,
}

impl RpcClient {
    pub fn spawn(executable: &Path) -> AppResult<Self> {
        let mut command = Command::new(executable);
        command
            .arg("app-server")
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.as_std_mut().process_group(0);
        let mut child = command
            .spawn()
            .map_err(|error| AppError::new("agent.spawn_failed", error.to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| protocol("Codex stdin 不可用"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| protocol("Codex stdout 不可用"))?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
            outstanding: HashSet::new(),
            responses: HashMap::new(),
            notifications: VecDeque::new(),
            queue_bytes: 0,
        })
    }

    pub async fn initialize(&mut self) -> AppResult<()> {
        self.request("initialize", json!({"clientInfo":{"name":"daybook","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}})).await?;
        self.write(&json!({"method":"initialized"}), REVERSE_DEADLINE)
            .await
    }

    pub async fn request(&mut self, method: &str, params: Value) -> AppResult<Value> {
        let id = self.send_request(method, params).await?;
        self.wait_response(id).await
    }

    pub async fn send_request(&mut self, method: &str, params: Value) -> AppResult<u64> {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| protocol("RPC 请求 ID 耗尽"))?;
        self.outstanding.insert(id);
        self.write(
            &json!({"id":id,"method":method,"params":params}),
            REVERSE_DEADLINE,
        )
        .await?;
        Ok(id)
    }

    pub async fn wait_response(&mut self, id: u64) -> AppResult<Value> {
        self.wait_response_with_timeout(id, REQUEST_DEADLINE).await
    }

    async fn wait_response_with_timeout(
        &mut self,
        id: u64,
        duration: Duration,
    ) -> AppResult<Value> {
        if !self.outstanding.contains(&id) {
            return Err(protocol("等待未知 RPC ID"));
        }
        let deadline = Instant::now() + duration;
        loop {
            if let Some((response, length)) = self.responses.remove(&id) {
                self.queue_bytes -= length;
                self.outstanding.remove(&id);
                if let Some(error) = response.get("error") {
                    return Err(protocol(&format!(
                        "RPC 返回错误：{}",
                        error
                            .get("code")
                            .and_then(Value::as_i64)
                            .unwrap_or_default()
                    )));
                }
                return response
                    .get("result")
                    .cloned()
                    .ok_or_else(|| protocol("RPC 响应缺少 result"));
            }
            let line = timeout_at(deadline, read_line_bounded(&mut self.stdout))
                .await
                .map_err(|_| AppError::new("agent.timeout", "Codex RPC 响应超时"))??
                .ok_or_else(|| AppError::new("agent.interrupted", "Codex RPC 响应前断流"))?;
            let value: Value =
                serde_json::from_slice(&line).map_err(|_| protocol("Codex RPC 消息不是 JSON"))?;
            let message_id = value.get("id");
            let method = value.get("method").and_then(Value::as_str);
            match (message_id, method) {
                (Some(reverse_id), Some(method)) => {
                    self.enqueue_notification(value.clone(), line.len())?;
                    let code = if method.to_ascii_lowercase().contains("permission")
                        || method.to_ascii_lowercase().contains("approval")
                    {
                        -32000
                    } else {
                        -32601
                    };
                    self.write(&json!({"id":reverse_id,"error":{"code":code,"message":"Daybook rejects reverse requests"}}), REVERSE_DEADLINE).await?;
                }
                (None, Some(_)) => self.enqueue_notification(value, line.len())?,
                (Some(response_id), None) => {
                    let response_id = response_id
                        .as_u64()
                        .ok_or_else(|| protocol("RPC 响应 ID 类型错误"))?;
                    if !self.outstanding.contains(&response_id)
                        || self.responses.contains_key(&response_id)
                    {
                        return Err(protocol("RPC 响应 ID 未匹配或重复"));
                    }
                    self.check_queue(line.len())?;
                    self.queue_bytes += line.len();
                    self.responses.insert(response_id, (value, line.len()));
                }
                _ => return Err(protocol("Codex RPC 消息缺少 method/id")),
            }
        }
    }

    fn check_queue(&self, length: usize) -> AppResult<()> {
        if self.notifications.len() + self.responses.len() >= MAX_QUEUE_COUNT
            || self.queue_bytes.saturating_add(length) > MAX_QUEUE_BYTES
        {
            return Err(protocol("Codex RPC 未处理队列超过硬上限"));
        }
        Ok(())
    }

    fn enqueue_notification(&mut self, value: Value, length: usize) -> AppResult<()> {
        self.check_queue(length)?;
        self.queue_bytes += length;
        self.notifications.push_back((value, length));
        Ok(())
    }

    async fn write(&mut self, value: &Value, duration: Duration) -> AppResult<()> {
        let mut bytes = serde_json::to_vec(value).map_err(|_| protocol("RPC 请求无法编码"))?;
        if bytes.len() > MAX_LINE {
            return Err(protocol("RPC 请求超过单行硬上限"));
        }
        bytes.push(b'\n');
        timeout(duration, self.stdin.write_all(&bytes))
            .await
            .map_err(|_| AppError::new("agent.timeout", "Codex RPC 写入超时"))?
            .map_err(|error| AppError::new("agent.interrupted", error.to_string()))
    }

    pub async fn close(mut self) {
        terminate_child_tree(&mut self.child).await;
        let _ = timeout(REAP_DEADLINE, self.child.wait()).await;
    }
}

async fn read_line_bounded<R: AsyncBufRead + Unpin>(reader: &mut R) -> AppResult<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let available = reader
            .fill_buf()
            .await
            .map_err(|error| AppError::new("agent.interrupted", error.to_string()))?;
        if available.is_empty() {
            if line.is_empty() {
                return Ok(None);
            }
            return Err(protocol("Codex RPC 消息未以换行结束"));
        }
        let length = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |position| position + 1);
        if line.len().saturating_add(length) > MAX_LINE {
            return Err(protocol("Codex RPC 单行超过硬上限"));
        }
        let ended = available[length - 1] == b'\n';
        line.extend_from_slice(&available[..length]);
        reader.consume(length);
        if ended {
            return Ok(Some(line));
        }
    }
}

fn protocol(message: &str) -> AppError {
    AppError::new("agent.protocol_violation", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn script(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fake-codex");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        (directory, path)
    }

    #[tokio::test]
    async fn line_limit_accepts_boundary_and_rejects_one_more() {
        let at_limit = vec![b'x'; MAX_LINE - 1];
        let mut bytes = at_limit.clone();
        bytes.push(b'\n');
        assert_eq!(
            read_line_bounded(&mut BufReader::new(bytes.as_slice()))
                .await
                .unwrap()
                .unwrap()
                .len(),
            MAX_LINE
        );
        bytes.insert(0, b'x');
        assert_eq!(
            read_line_bounded(&mut BufReader::new(bytes.as_slice()))
                .await
                .unwrap_err()
                .code,
            "agent.protocol_violation"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn out_of_order_responses_preserve_notifications_and_deny_reverse_request() {
        let (_directory, path) = script("read first\nread second\nprintf '%s\\n' '{\"method\":\"turn/started\",\"params\":{}}' '{\"id\":99,\"method\":\"commandExecution/requestApproval\",\"params\":{}}'\nread denial\ncase \"$denial\" in *\"-32000\"*) ;; *) exit 3;; esac\nprintf '%s\\n' '{\"id\":2,\"result\":{\"order\":2}}' '{\"id\":1,\"result\":{\"order\":1}}'");
        let mut client = RpcClient::spawn(&path).unwrap();
        let first = client.send_request("first", json!({})).await.unwrap();
        let second = client.send_request("second", json!({})).await.unwrap();
        assert_eq!(client.wait_response(first).await.unwrap()["order"], 1);
        assert_eq!(client.wait_response(second).await.unwrap()["order"], 2);
        assert_eq!(client.notifications.len(), 2);
        client.close().await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn queue_overflow_eof_and_deadline_fail_closed() {
        let (_directory, path) = script("read first\ni=0\nwhile [ \"$i\" -le 128 ]; do printf '{\"method\":\"progress\",\"params\":{\"index\":%s}}\\n' \"$i\"; i=$((i+1)); done\nsleep 5");
        let mut client = RpcClient::spawn(&path).unwrap();
        let id = client.send_request("first", json!({})).await.unwrap();
        assert_eq!(
            client.wait_response(id).await.unwrap_err().code,
            "agent.protocol_violation"
        );
        client.close().await;

        let (_directory, path) = script("read first\nexit 0");
        let mut client = RpcClient::spawn(&path).unwrap();
        let id = client.send_request("first", json!({})).await.unwrap();
        assert_eq!(
            client.wait_response(id).await.unwrap_err().code,
            "agent.interrupted"
        );
        client.close().await;

        let (_directory, path) = script("read first\nsleep 5");
        let mut client = RpcClient::spawn(&path).unwrap();
        let id = client.send_request("first", json!({})).await.unwrap();
        assert_eq!(
            client
                .wait_response_with_timeout(id, Duration::from_millis(30))
                .await
                .unwrap_err()
                .code,
            "agent.timeout"
        );
        client.close().await;
    }
}
