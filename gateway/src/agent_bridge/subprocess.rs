//! Subprocess management for the Python agent.

use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use hermes_core::errors::GatewayError;
use super::protocol::{JsonRpcRequest, JsonRpcNotification};
use super::BridgeConfig;

/// Handle to the Python agent subprocess.
pub struct AgentSubprocess {
    /// Child process handle
    child: Child,

    /// stdin pipe to the agent (wrapped in Arc<Mutex> for shared async access)
    stdin: Arc<Mutex<ChildStdin>>,

    /// stdout reader from the agent (wrapped in Arc<Mutex> for shared async access)
    stdout_reader: Arc<Mutex<BufReader<ChildStdout>>>,
}

impl AgentSubprocess {
    /// Spawn the Python agent subprocess.
    pub async fn spawn(config: &BridgeConfig) -> Result<Self, GatewayError> {
        let mut cmd = Command::new(&config.python_path);

        // Run the agent module
        cmd.arg("-m").arg(&config.agent_module);

        // Set working directory if specified
        if let Some(ref wd) = config.working_dir {
            cmd.current_dir(wd);
        }

        // Pass environment variables
        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        // Configure pipes
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()); // Inherit stderr for logging

        info!(
            "Spawning Python agent: {} -m {}",
            config.python_path, config.agent_module
        );

        let mut child = cmd.spawn().map_err(|e| {
            GatewayError::Platform(format!("Failed to spawn Python agent: {}", e))
        })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            GatewayError::Platform("Failed to capture agent stdin".to_string())
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            GatewayError::Platform("Failed to capture agent stdout".to_string())
        })?;

        let stdout_reader = BufReader::new(stdout);

        info!("Python agent subprocess spawned successfully");

        Ok(Self {
            child,
            stdin: Arc::new(Mutex::new(stdin)),
            stdout_reader: Arc::new(Mutex::new(stdout_reader)),
        })
    }

    /// Send a JSON-RPC request to the agent.
    pub async fn send_request(&self, request: &JsonRpcRequest) -> Result<(), GatewayError> {
        let json = serde_json::to_string(request).map_err(|e| {
            GatewayError::Platform(format!("Failed to serialize request: {}", e))
        })?;

        self.send_line(&json).await
    }

    /// Send a JSON-RPC notification to the agent.
    pub async fn send_notification(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), GatewayError> {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        };

        let json = serde_json::to_string(&notification).map_err(|e| {
            GatewayError::Platform(format!("Failed to serialize notification: {}", e))
        })?;

        self.send_line(&json).await
    }

    /// Send a line to the agent's stdin.
    async fn send_line(&self, line: &str) -> Result<(), GatewayError> {
        let mut stdin = self.stdin.lock().await;

        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| GatewayError::Platform(format!("Failed to write to agent stdin: {}", e)))?;

        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| GatewayError::Platform(format!("Failed to write newline to agent stdin: {}", e)))?;

        stdin
            .flush()
            .await
            .map_err(|e| GatewayError::Platform(format!("Failed to flush agent stdin: {}", e)))?;

        debug!("Sent to agent: {}", line);
        Ok(())
    }

    /// Get a cloned reference to the stdout reader for the agent.
    pub fn stdout_reader(&self) -> Arc<Mutex<BufReader<ChildStdout>>> {
        Arc::clone(&self.stdout_reader)
    }

    /// Terminate the subprocess.
    pub async fn terminate(mut self) -> Result<(), GatewayError> {
        // Try graceful shutdown first (SIGTERM)
        if let Err(e) = self.child.start_kill() {
            warn!("Failed to send SIGTERM to agent: {}", e);
        }

        // Wait up to 2 seconds for graceful exit
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(2),
            self.child.wait(),
        )
        .await
        {
            Ok(Ok(status)) => {
                info!("Agent exited with status: {}", status);
                Ok(())
            }
            Ok(Err(e)) => {
                error!("Failed to wait for agent exit: {}", e);
                Err(GatewayError::Platform(format!(
                    "Failed to wait for agent exit: {}",
                    e
                )))
            }
            Err(_) => {
                warn!("Agent did not exit gracefully, sending SIGKILL");
                if let Err(e) = self.child.kill().await {
                    error!("Failed to send SIGKILL to agent: {}", e);
                    return Err(GatewayError::Platform(format!(
                        "Failed to kill agent: {}",
                        e
                    )));
                }
                info!("Agent forcefully killed");
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Only run with --ignored (requires Python)
    async fn test_spawn_subprocess() {
        let config = BridgeConfig {
            python_path: "python3".to_string(),
            agent_module: "hermes_cli.agent_bridge".to_string(),
            working_dir: None,
            env_vars: std::collections::HashMap::new(),
            heartbeat_interval_secs: 30,
            request_timeout_secs: 300,
            auto_restart: false,
            max_restart_attempts: 0,
        };

        let result = AgentSubprocess::spawn(&config).await;
        assert!(result.is_ok(), "Failed to spawn subprocess");

        let subprocess = result.unwrap();
        subprocess.terminate().await.unwrap();
    }
}
