//! The local panel preview an agent can start, poll and stop.

use super::call::{ToolCall, text};
use crate::service;
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead},
    process::Command,
};

/// How long to wait for the preview panel to print its address.
const PREVIEW_START_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Default)]
pub(super) struct Preview(
    pub(super) Option<std::process::Child>,
    pub(super) Option<String>,
);

impl Drop for Preview {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl ToolCall<'_> {
    pub(super) fn preview_start(&self, preview: &mut Preview) -> Result<Value> {
        let args = self.args;
        let output: String = {
            if preview.0.is_some() {
                bail!("Stop the existing preview first");
            }
            let root = service::output()?.with_extension("preview");
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            std::fs::create_dir_all(root.parent().unwrap())?;
            builder.create(&root)?;
            if args["demo"].as_bool().unwrap_or(false) {
                let fixture = root.join("design/demo");
                std::fs::create_dir_all(&fixture)?;
                std::fs::write(
                    fixture.join("project.json"),
                    r#"{"id":"demo","label":"Example project"}"#,
                )?;
            }
            let binary = std::env::current_exe()?.with_file_name("rupicola-panel");
            let mut child = Command::new(binary)
                .args(["--port", "0", "--db"])
                .arg(root.join("panel.db"))
                .arg("--captures")
                .arg(root.join("captures"))
                .env_remove("RUPICOLA_SETUP_CODE")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .spawn()?;
            let stdout = child.stdout.take().unwrap();
            let (send, receive) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let mut reader = io::BufReader::new(stdout);
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let _ = send.send(line);
                let _ = io::copy(&mut reader, &mut io::sink());
            });
            let url = receive
                .recv_timeout(PREVIEW_START_TIMEOUT)
                .ok()
                .and_then(|line| {
                    line.split_whitespace()
                        .find(|word| word.starts_with("http://127.0.0.1:"))
                        .map(str::to_owned)
                });
            let Some(url) = url else {
                let _ = child.kill();
                let _ = child.wait();
                bail!("Preview did not announce a loopback URL within 15 seconds");
            };
            *preview = Preview(Some(child), Some(url.clone()));
            json!({"url":url,"fixture":args["demo"] == true}).to_string()
        };
        Ok(text(&output))
    }

    pub(super) fn preview_status(preview: &mut Preview) -> Result<Value> {
        let output: String = {
            if let Some(child) = preview.0.as_mut()
                && child.try_wait()?.is_some()
            {
                *preview = Preview::default();
            }
            json!({"url":preview.1}).to_string()
        };
        Ok(text(&output))
    }

    pub(super) fn preview_stop(preview: &mut Preview) -> Value {
        *preview = Preview::default();
        text("Preview stopped")
    }
}
