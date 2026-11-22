//! Problems, fixes and storage, forwarded to the CLI.

use super::call::{ToolCall, cli, text};
use anyhow::Result;
use serde_json::Value;

impl ToolCall<'_> {
    pub(super) fn problem_list(&self) -> Result<Value> {
        let args = self.args;
        let output: String = {
            let mut command = vec!["problem".into(), "list".into(), "--json".into()];
            if let Some(p) = args["project"].as_str() {
                command.push(format!("--project={p}"));
            }
            if let Some(s) = args["status"].as_str() {
                command.push(format!("--status={s}"));
            }
            cli(&command)?
        };
        Ok(text(&output))
    }

    pub(super) fn problem_add(&self) -> Result<Value> {
        let args = self.args;
        let arg = |key: &str| self.arg(key);
        let output: String = {
            let mut command = vec![
                "problem".into(),
                "add".into(),
                format!("--title={}", arg("title")?),
                format!("--diagnosis={}", arg("diagnosis")?),
                format!("--solution={}", arg("solution")?),
            ];
            for (k, flag) in [
                ("project", "--project"),
                ("route", "--route"),
                ("viewport", "--viewport"),
                ("theme", "--theme"),
                ("severity", "--severity"),
                ("rule", "--rule"),
                ("evidence", "--evidence"),
            ] {
                if let Some(v) = args[k].as_str() {
                    command.push(format!("{flag}={v}"));
                }
            }
            cli(&command)?
        };
        Ok(text(&output))
    }

    pub(super) fn problem_triage(&self) -> Result<Value> {
        let args = self.args;
        let arg = |key: &str| self.arg(key);
        let output: String = {
            let mut command = vec![
                "problem".into(),
                "triage".into(),
                arg("id")?.into(),
                format!("--status={}", arg("status")?),
            ];
            for (k, flag) in [
                ("project", "--project"),
                ("comment", "--comment"),
                ("solution", "--solution"),
                ("agent", "--agent"),
            ] {
                if let Some(v) = args[k].as_str() {
                    command.push(format!("{flag}={v}"));
                }
            }
            cli(&command)?
        };
        Ok(text(&output))
    }

    pub(super) fn problem_progress(&self) -> Result<Value> {
        let args = self.args;
        let arg = |key: &str| self.arg(key);
        let output: String = {
            let default_agent = format!("mcp-agent (PID {})", std::process::id());
            let agent = args["agent"].as_str().unwrap_or(&default_agent);
            let mut command = vec![
                "problem".into(),
                "progress".into(),
                arg("id")?.into(),
                format!("--agent={agent}"),
                format!("--activity={}", arg("activity")?),
            ];
            if let Some(p) = args["project"].as_str() {
                command.push(format!("--project={p}"));
            }
            if let Some(st) = args["stage"].as_str() {
                command.push(format!("--stage={st}"));
            }
            if let Some(pct) = args["pct"].as_u64() {
                command.push(format!("--pct={pct}"));
            }
            if let Some(step) = args["step"].as_u64() {
                command.push(format!("--step={step}"));
            }
            if let Some(tot) = args["total_steps"].as_u64() {
                command.push(format!("--total-steps={tot}"));
            }
            cli(&command)?
        };
        Ok(text(&output))
    }

    pub(super) fn fix_list(&self) -> Result<Value> {
        let args = self.args;
        let output: String = {
            let mut command = vec!["fix".into(), "list".into(), "--json".into()];
            if let Some(p) = args["project"].as_str() {
                command.push(format!("--project={p}"));
            }
            if let Some(s) = args["status"].as_str() {
                command.push(format!("--status={s}"));
            }
            cli(&command)?
        };
        Ok(text(&output))
    }

    pub(super) fn fix_record(&self) -> Result<Value> {
        let args = self.args;
        let arg = |key: &str| self.arg(key);
        let output: String = {
            let mut command = vec![
                "fix".into(),
                "record".into(),
                format!("--title={}", arg("title")?),
                format!("--summary={}", arg("summary")?),
                format!("--before-url={}", arg("before_url")?),
                format!("--after-url={}", arg("after_url")?),
            ];
            for (k, flag) in [
                ("project", "--project"),
                ("problem", "--problem"),
                ("route", "--route"),
                ("viewport", "--viewport"),
                ("theme", "--theme"),
                ("diff_url", "--diff-url"),
                ("tactic", "--tactic"),
                ("file", "--file"),
                ("step", "--step"),
            ] {
                if let Some(v) = args[k].as_str() {
                    command.push(format!("{flag}={v}"));
                }
            }
            cli(&command)?
        };
        Ok(text(&output))
    }

    pub(super) fn storage_status(&self) -> Result<Value> {
        let args = self.args;
        let output: String = {
            let mut command = vec!["storage".into(), "status".into(), "--json".into()];
            if let Some(p) = args["project"].as_str() {
                command.push(format!("--project={p}"));
            }
            cli(&command)?
        };
        Ok(text(&output))
    }

    pub(super) fn storage_clean(&self) -> Result<Value> {
        let args = self.args;
        let output: String = {
            let mut command = vec!["storage".into(), "clean".into(), "--json".into()];
            for (k, flag) in [
                ("mode", "--mode"),
                ("project", "--project"),
                ("date", "--date"),
                ("run", "--run"),
            ] {
                if let Some(v) = args[k].as_str() {
                    command.push(format!("{flag}={v}"));
                }
            }
            if let Some(kl) = args["keep_latest"].as_u64() {
                command.push(format!("--keep-latest={kl}"));
            }
            cli(&command)?
        };
        Ok(text(&output))
    }

    pub(super) fn storage_offload(&self) -> Result<Value> {
        let args = self.args;
        let output: String = {
            let mut command = vec!["storage".into(), "offload".into(), "--json".into()];
            for (k, flag) in [("run", "--run"), ("fix", "--fix"), ("project", "--project")] {
                if let Some(v) = args[k].as_str() {
                    command.push(format!("{flag}={v}"));
                }
            }
            cli(&command)?
        };
        Ok(text(&output))
    }
}
