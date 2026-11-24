//! Spawns a worktree's preview server on a free port and waits until it answers `ready`.

use anyhow::{Context, Result, bail};
use rupicola_api::PreviewConfig;
use std::{
    net::{Ipv4Addr, SocketAddr},
    path::Path,
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::AsyncReadExt as _,
    net::TcpListener,
    process::{Child, Command},
    sync::Mutex,
    time::Instant,
};

const PORT_PLACEHOLDER: &str = "{port}";
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const STDERR_TAIL_BYTES: usize = 4096;

/// A running preview server for one worktree, stopped on [`PreviewProcess::stop`] or drop.
pub struct PreviewProcess {
    port: u16,
    child: Child,
    stderr_task: tokio::task::JoinHandle<()>,
}

impl PreviewProcess {
    /// Picks a free loopback port, starts `config.command` inside `worktree_path`, and polls
    /// `config.ready` until it returns 2xx or the timeout / child exit occurs.
    ///
    /// # Errors
    ///
    /// Fails when the port cannot be bound, the command exits early, or the ready check times out.
    pub async fn start(
        worktree_path: &Path,
        repo_root: &Path,
        config: &PreviewConfig,
    ) -> Result<Self> {
        let port = pick_free_port().await?;
        let port_text = port.to_string();
        let command_line = config.command.replace(PORT_PLACEHOLDER, &port_text);
        let ready_url = config.ready.replace(PORT_PLACEHOLDER, &port_text);

        let mut cmd = shell_command(&command_line);
        cmd.current_dir(worktree_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        if repo_root.join("Cargo.toml").exists() && std::env::var_os("CARGO_TARGET_DIR").is_none() {
            cmd.env("CARGO_TARGET_DIR", repo_root.join("target"));
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("spawn preview command `{command_line}`"))?;

        let stderr_tail = Arc::new(Mutex::new(Vec::<u8>::new()));
        let stderr_pipe = child.stderr.take();
        let tail_clone = Arc::clone(&stderr_tail);
        let stderr_task = tokio::spawn(async move {
            let Some(mut reader) = stderr_pipe else {
                return;
            };
            let mut buf = [0u8; 1024];
            loop {
                match reader.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let mut guard = tail_clone.lock().await;
                        guard.extend_from_slice(&buf[..n]);
                        if guard.len() > STDERR_TAIL_BYTES {
                            let drop_n = guard.len() - STDERR_TAIL_BYTES;
                            guard.drain(..drop_n);
                        }
                    }
                }
            }
        });

        let client = reqwest::Client::builder()
            .timeout(PROBE_TIMEOUT)
            .build()
            .context("build preview readiness HTTP client")?;
        let deadline = Instant::now() + Duration::from_secs(config.timeout_secs);

        loop {
            if let Some(status) = child.try_wait().context("poll preview process")? {
                let _ = tokio::time::timeout(Duration::from_millis(200), stderr_task).await;
                let tail = String::from_utf8_lossy(&stderr_tail.lock().await)
                    .trim()
                    .to_owned();
                bail!("preview exited with {status}: {tail}");
            }

            if let Ok(resp) = client.get(&ready_url).send().await
                && resp.status().is_success()
            {
                return Ok(Self {
                    port,
                    child,
                    stderr_task,
                });
            }

            if Instant::now() >= deadline {
                let _ = child.kill().await;
                let _ = tokio::time::timeout(Duration::from_millis(200), stderr_task).await;
                let tail = String::from_utf8_lossy(&stderr_tail.lock().await)
                    .trim()
                    .to_owned();
                bail!(
                    "preview did not become ready at {ready_url} within {}s: {tail}",
                    config.timeout_secs
                );
            }

            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    /// Base URL for the running preview server (`http://127.0.0.1:{port}`).
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://{}:{}/", Ipv4Addr::LOCALHOST, self.port)
    }

    /// Stops the preview child process.
    pub async fn stop(mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
        self.stderr_task.abort();
    }
}

async fn pick_free_port() -> Result<u16> {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .context("bind ephemeral loopback port")?;
    Ok(listener.local_addr()?.port())
}

#[cfg(unix)]
fn shell_command(command_line: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", command_line]);
    cmd
}

#[cfg(windows)]
fn shell_command(command_line: &str) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", command_line]);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt as _;

    #[tokio::test]
    async fn preview_fails_immediately_with_stderr_tail_on_early_exit() {
        let dir = tempfile::tempdir().unwrap();
        let config = PreviewConfig {
            command: "echo 'boom on stderr' >&2; exit 2".to_owned(),
            ready: "http://127.0.0.1:{port}/healthz".to_owned(),
            timeout_secs: 5,
        };
        let err = PreviewProcess::start(dir.path(), dir.path(), &config)
            .await
            .err()
            .unwrap();
        assert!(err.to_string().contains("boom on stderr"), "{err}");
    }

    #[tokio::test]
    async fn preview_waits_for_ready_http_200() {
        let dir = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 512];
                let _ = stream.read(&mut buf).await;
                let _ = stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                    .await;
            }
        });
        let config = PreviewConfig {
            command: "sleep 10".to_owned(),
            ready: format!("http://127.0.0.1:{port}/healthz"),
            timeout_secs: 5,
        };
        let proc = PreviewProcess::start(dir.path(), dir.path(), &config)
            .await
            .unwrap();
        proc.stop().await;
        server.abort();
    }
}
