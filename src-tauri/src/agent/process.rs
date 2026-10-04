use std::{
    io,
    process::{ExitStatus, Output},
    time::Duration,
};

use serde_json::json;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
    sync::watch,
    task::JoinHandle,
    time::{sleep, timeout},
};

use crate::error::{AppError, AppResult};

pub const STDOUT_LIMIT: usize = 16 * 1024 * 1024;
pub const STDERR_LIMIT: usize = 4 * 1024 * 1024;
const DRAIN_DEADLINE: Duration = Duration::from_secs(3);
const TERM_GRACE: Duration = Duration::from_secs(2);

pub async fn run_bounded(
    mut command: Command,
    duration: Duration,
    mut cancel: watch::Receiver<bool>,
) -> AppResult<Output> {
    let mut child = command
        .spawn()
        .map_err(|error| AppError::new("agent.spawn_failed", error.to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::new("agent.spawn_failed", "无法读取 agent stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::new("agent.spawn_failed", "无法读取 agent stderr"))?;
    let (overflow_tx, mut overflow_rx) = watch::channel(false);
    let mut stdout_task = tokio::spawn(read_bounded(stdout, STDOUT_LIMIT, overflow_tx.clone()));
    let mut stderr_task = tokio::spawn(read_bounded(stderr, STDERR_LIMIT, overflow_tx));
    let deadline = sleep(duration);
    tokio::pin!(deadline);
    let mut watch_cancel = true;
    let status: ExitStatus = loop {
        tokio::select! {
            waited = child.wait() => break waited.map_err(|error| AppError::new("agent.spawn_failed", error.to_string()))?,
            changed = cancel.changed(), if watch_cancel => {
                if changed.is_ok() && *cancel.borrow() {
                    terminate_child_tree(&mut child).await;
                    let (stdout, stderr) = collect_after_stop(&child, &mut stdout_task, &mut stderr_task).await;
                    return Err(AppError::new("agent.cancelled", "解析已由用户停止").with_detail(json!({"stdout": String::from_utf8_lossy(&stdout), "stderr": String::from_utf8_lossy(&stderr)})));
                }
                if changed.is_err() { watch_cancel = false; }
            }
            _ = &mut deadline => {
                terminate_child_tree(&mut child).await;
                let (stdout, stderr) = collect_after_stop(&child, &mut stdout_task, &mut stderr_task).await;
                return Err(AppError::new("agent.timeout", "agent 子进程超过硬超时").with_detail(json!({"stdout": String::from_utf8_lossy(&stdout), "stderr": String::from_utf8_lossy(&stderr)})));
            }
            changed = overflow_rx.changed() => {
                if changed.is_ok() && *overflow_rx.borrow() {
                    terminate_child_tree(&mut child).await;
                    stdout_task.abort();
                    stderr_task.abort();
                    return Err(AppError::new("agent.output_limit", "agent 输出超过硬上限"));
                }
            }
        }
    };
    let (stdout, stderr) = match collect(&mut stdout_task, &mut stderr_task).await {
        Some(value) => value,
        None => {
            kill_process_group(&child);
            stdout_task.abort();
            stderr_task.abort();
            return Err(AppError::new(
                "agent.protocol_violation",
                "agent 退出后输出管道未在收尾时限内关闭",
            ));
        }
    };
    if *overflow_rx.borrow() {
        return Err(AppError::new("agent.output_limit", "agent 输出超过硬上限"));
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

async fn read_bounded<R: AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
    overflow: watch::Sender<bool>,
) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut over_limit = false;
    loop {
        let length = reader.read(&mut chunk).await?;
        if length == 0 {
            break;
        }
        if over_limit {
            continue;
        }
        if bytes.len().saturating_add(length) > limit {
            over_limit = true;
            let _ = overflow.send(true);
        } else {
            bytes.extend_from_slice(&chunk[..length]);
        }
    }
    Ok(bytes)
}

async fn collect(
    stdout: &mut JoinHandle<io::Result<Vec<u8>>>,
    stderr: &mut JoinHandle<io::Result<Vec<u8>>>,
) -> Option<(Vec<u8>, Vec<u8>)> {
    timeout(DRAIN_DEADLINE, async {
        let (out, err) = tokio::join!(stdout, stderr);
        Some((out.ok()?.ok()?, err.ok()?.ok()?))
    })
    .await
    .ok()
    .flatten()
}

async fn collect_after_stop(
    child: &Child,
    stdout: &mut JoinHandle<io::Result<Vec<u8>>>,
    stderr: &mut JoinHandle<io::Result<Vec<u8>>>,
) -> (Vec<u8>, Vec<u8>) {
    match collect(stdout, stderr).await {
        Some(output) => output,
        None => {
            kill_process_group(child);
            stdout.abort();
            stderr.abort();
            (Vec::new(), Vec::new())
        }
    }
}

pub async fn terminate_child_tree(child: &mut Child) {
    #[cfg(unix)]
    if let Some(id) = child.id() {
        // SAFETY: callers spawn each CLI in its own process group.
        unsafe {
            libc::kill(-(id as i32), libc::SIGTERM);
        }
    }
    if timeout(TERM_GRACE, child.wait()).await.is_err() {
        kill_process_group(child);
        let _ = child.kill().await;
        let _ = timeout(DRAIN_DEADLINE, child.wait()).await;
    }
}

fn kill_process_group(child: &Child) {
    #[cfg(unix)]
    if let Some(id) = child.id() {
        // SAFETY: callers spawn each CLI in its own process group.
        unsafe {
            libc::kill(-(id as i32), libc::SIGKILL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn script(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fake-cli");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        (directory, path)
    }

    #[cfg(unix)]
    fn command(path: &std::path::Path) -> Command {
        use std::os::unix::process::CommandExt;
        let mut command = Command::new(path);
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        command.as_std_mut().process_group(0);
        command
    }

    #[tokio::test]
    async fn bounded_reader_fails_on_one_extra_byte() {
        let (mut writer, reader) = tokio::io::duplex(32);
        let (tx, rx) = watch::channel(false);
        let task = tokio::spawn(async move { read_bounded(reader, 4, tx).await.unwrap() });
        use tokio::io::AsyncWriteExt;
        writer.write_all(b"12345").await.unwrap();
        drop(writer);
        assert_eq!(task.await.unwrap(), Vec::<u8>::new());
        assert!(*rx.borrow());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancellation_and_hard_timeout_reap_fake_cli() {
        let (_directory, path) = script("sleep 10");
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let running = tokio::spawn(run_bounded(
            command(&path),
            Duration::from_secs(5),
            cancel_rx,
        ));
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel_tx.send(true).unwrap();
        assert_eq!(running.await.unwrap().unwrap_err().code, "agent.cancelled");
        let (_cancel_tx, cancel_rx) = watch::channel(false);
        assert_eq!(
            run_bounded(command(&path), Duration::from_millis(50), cancel_rx)
                .await
                .unwrap_err()
                .code,
            "agent.timeout"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn descendant_holding_pipe_cannot_hang_reap() {
        let (_directory, path) = script("(trap '' TERM; sleep 10) &\nexit 0");
        let (_cancel_tx, cancel_rx) = watch::channel(false);
        let result = timeout(
            Duration::from_secs(5),
            run_bounded(command(&path), Duration::from_secs(5), cancel_rx),
        )
        .await
        .unwrap();
        assert_eq!(result.unwrap_err().code, "agent.protocol_violation");
    }
}
