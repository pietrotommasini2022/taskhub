use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use taskhub_core::{engine::Action, TaskHubError};
use tokio::process::Command;
use tokio::time::timeout;

pub struct ShellAction;

#[async_trait]
impl Action for ShellAction {
    fn plugin_id(&self) -> &str { "core" }
    fn action_id(&self) -> &str { "shell" }

    async fn execute(&self, input: Value) -> Result<Value, TaskHubError> {
        // `command` must be an array of strings: ["git", "status"]
        let args: Vec<String> = input["command"]
            .as_array()
            .ok_or_else(|| TaskHubError::Plugin("core/shell: 'command' must be an array".into()))?
            .iter()
            .map(|v| v.as_str().unwrap_or("").to_string())
            .collect();

        if args.is_empty() {
            return Err(TaskHubError::Plugin("core/shell: 'command' cannot be empty".into()));
        }

        let timeout_secs = input["timeout"]
            .as_str()
            .and_then(|t| taskhub_core::workflow::parse_every(t).ok())
            .unwrap_or(30);

        let capture = input["capture"].as_str().unwrap_or("stdout");

        let mut cmd = Command::new(&args[0]);
        cmd.args(&args[1..]);

        if let Some(cwd) = input["cwd"].as_str() {
            let expanded = shellexpand::tilde(cwd).into_owned();
            cmd.current_dir(expanded);
        }

        if let Some(env) = input["env"].as_object() {
            let pairs: HashMap<String, String> = env
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect();
            cmd.envs(pairs);
        }

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let start = std::time::Instant::now();
        let child = cmd.spawn().map_err(|e| TaskHubError::Plugin(e.to_string()))?;

        let output = timeout(Duration::from_secs(timeout_secs), child.wait_with_output())
            .await
            .map_err(|_| TaskHubError::Plugin("core/shell: command timed out".into()))?
            .map_err(|e| TaskHubError::Plugin(e.to_string()))?;

        let duration_ms = start.elapsed().as_millis() as u64;
        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let captured_stdout = matches!(capture, "stdout" | "both");
        let captured_stderr = matches!(capture, "stderr" | "both");

        Ok(serde_json::json!({
            "exit_code": exit_code,
            "stdout": if captured_stdout { stdout } else { String::new() },
            "stderr": if captured_stderr { stderr } else { String::new() },
            "duration_ms": duration_ms,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use taskhub_core::engine::Action;

    #[test]
    fn action_ids() {
        assert_eq!(ShellAction.full_id(), "core/shell");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn echo_works() {
        let out = ShellAction
            .execute(serde_json::json!({"command": ["echo", "hello"]}))
            .await
            .unwrap();
        assert_eq!(out["exit_code"], 0);
        assert!(out["stdout"].as_str().unwrap().contains("hello"));
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn echo_works_windows() {
        let out = ShellAction
            .execute(serde_json::json!({"command": ["cmd", "/C", "echo hello"]}))
            .await
            .unwrap();
        assert_eq!(out["exit_code"], 0);
    }
}
