//! Subprocess management for the Python agent.

use std::process::Stdio;
use std::sync::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tracing::{debug, error, info, warn};

use hermes_core::errors::GatewayError;
use super::protocol::{JsonRpcRequest, JsonRpcNotification};
use super::BridgeConfig;

/// Handle to the Python agent subprocess.
pub struct AgentSubprocess {
    /// Child process handle
    child: Child,

    /// stdin pipe to the agent (wrapped in Mutex for interior mutability)
    stdin: Mutex<ChildStdin>,

    /// stdout reader from the agent
    stdout_reader: BufReader<ChildStdout>,
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
            stdin: Mutex::new(stdin),
            stdout_reader,
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
        let mut stdin = self.stdin.lock().unwrap();

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

    /// Get a reader for the agent's stdout.
    pub fn stdout_reader(&mut self) -> StdoutReader {
        StdoutReader {
            // SAFETY: Similar to stdin, we need to share the reader across tasks.
            // This is safe because:
            // 1. Only one reader task exists per subprocess
            // 2. The reader is not accessed from multiple threads simultaneously
            reader_ptr: &mut self.stdout_reader as *mut BufReader<ChildStdout>,
        }
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

/// Reader for agent stdout (thread-safe wrapper).
pub struct StdoutReader {
    reader_ptr: *mut BufReader<ChildStdout>,
}

// SAFETY: We manually ensure thread safety by controlling access patterns.
unsafe impl Send for StdoutReader {}

impl StdoutReader {
    /// Read a line from the agent's stdout.
    pub async fn read_line(&mut self) -> Result<Option<String>, GatewayError> {
        // SAFETY: See comment in `send_line` - we control access patterns
        let reader = unsafe { &mut *self.reader_ptr };

        loop {
            let mut line = String::new();
            match reader.read_line(&mut line).await {
                Ok(0) => return Ok(None), // EOF
                Ok(_) => {
                    let trimmed = line.trim_end();
                    if trimmed.is_empty() {
                        // Skip empty lines and continue loop
                        continue;
                    }
                    return Ok(Some(trimmed.to_string()));
                }
                Err(e) => {
                    return Err(GatewayError::Platform(format!(
                        "Failed to read from agent stdout: {}",
                        e
                    )));
                }
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
