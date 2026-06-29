//! Agent Bridge - JSON-RPC communication with Python agent subprocess
//!
//! This module implements bidirectional JSON-RPC 2.0 communication between the
//! Rust gateway and the Python agent over stdin/stdout pipes.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐                        ┌──────────────┐
//! │ Rust Gateway│                        │ Python Agent │
//! └──────┬──────┘                        └──────┬───────┘
//!        │                                      │
//!        │  JSON-RPC Request (stdin)            │
//!        ├─────────────────────────────────────>│
//!        │                                      │
//!        │  JSON-RPC Response/Notification (stdout)
//!        │<─────────────────────────────────────┤
//! ```
//!
//! # Protocol
//!
//! See `docs/agent_bridge_protocol.md` for full specification.

pub mod protocol;
pub mod subprocess;
pub mod types;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, error, info, warn};

use hermes_core::errors::GatewayError;
use protocol::{JsonRpcRequest, JsonRpcResponse, JsonRpcNotification};
use subprocess::AgentSubprocess;
pub use types::*;

/// Agent bridge handle for managing the Python agent subprocess.
///
/// Maintains:
/// - Subprocess lifecycle (spawn, monitor, restart)
/// - Request/response routing (JSON-RPC id tracking)
/// - Notification broadcasting
/// - Multiple concurrent sessions
pub struct AgentBridge {
    /// Subprocess handle
    subprocess: Arc<RwLock<Option<AgentSubprocess>>>,

    /// Pending requests waiting for responses (id -> oneshot sender)
    pending_requests: Arc<RwLock<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,

    /// Next JSON-RPC request ID
    next_id: Arc<RwLock<u64>>,

    /// Notification subscribers (session_id -> channel)
    notification_subscribers: Arc<RwLock<HashMap<String, mpsc::UnboundedSender<AgentNotification>>>>,

    /// Bridge configuration
    config: BridgeConfig,
}

/// Configuration for the agent bridge.
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// Path to Python executable (default: "python3")
    pub python_path: String,

    /// Python agent module to run (default: "hermes_cli.agent_bridge")
    pub agent_module: String,

    /// Working directory for the agent process
    pub working_dir: Option<String>,

    /// Environment variables to pass to the agent
    pub env_vars: HashMap<String, String>,

    /// Heartbeat interval in seconds (default: 30)
    pub heartbeat_interval_secs: u64,

    /// Request timeout in seconds (default: 300)
    pub request_timeout_secs: u64,

    /// Auto-restart on crash (default: true)
    pub auto_restart: bool,

    /// Max restart attempts (default: 3)
    pub max_restart_attempts: u32,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            python_path: "python3".to_string(),
            agent_module: "hermes_cli.agent_bridge".to_string(),
            working_dir: None,
            env_vars: HashMap::new(),
            heartbeat_interval_secs: 30,
            request_timeout_secs: 300,
            auto_restart: true,
            max_restart_attempts: 3,
        }
    }
}

impl AgentBridge {
    /// Create a new agent bridge with the given configuration.
    pub fn new(config: BridgeConfig) -> Self {
        Self {
            subprocess: Arc::new(RwLock::new(None)),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(1)),
            notification_subscribers: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Start the agent bridge (spawns the Python subprocess).
    pub async fn start(&self) -> Result<(), GatewayError> {
        let mut subprocess_guard = self.subprocess.write().await;

        if subprocess_guard.is_some() {
            warn!("Agent bridge already started");
            return Ok(());
        }

        info!(
            "Starting agent bridge: {} -m {}",
            self.config.python_path, self.config.agent_module
        );

        let mut subprocess = AgentSubprocess::spawn(&self.config).await?;

        // Clone Arc references for the reader task
        let pending_requests = Arc::clone(&self.pending_requests);
        let notification_subscribers = Arc::clone(&self.notification_subscribers);
        let subprocess_arc = Arc::clone(&self.subprocess);

        // Spawn reader task to handle stdout from the agent
        let mut reader = subprocess.stdout_reader();
        tokio::spawn(async move {
            loop {
                match reader.read_line().await {
                    Ok(Some(line)) => {
                        if let Err(e) = Self::handle_agent_message(
                            &line,
                            &pending_requests,
                            &notification_subscribers,
                        ).await {
                            error!("Failed to handle agent message: {}", e);
                        }
                    }
                    Ok(None) => {
                        warn!("Agent stdout closed");
                        break;
                    }
                    Err(e) => {
                        error!("Failed to read from agent stdout: {}", e);
                        break;
                    }
                }
            }

            // Mark subprocess as dead
            let mut guard = subprocess_arc.write().await;
            *guard = None;
        });

        *subprocess_guard = Some(subprocess);

        info!("Agent bridge started successfully");
        Ok(())
    }

    /// Stop the agent bridge (terminates the subprocess).
    pub async fn stop(&self) -> Result<(), GatewayError> {
        let mut subprocess_guard = self.subprocess.write().await;

        if let Some(subprocess) = subprocess_guard.take() {
            info!("Stopping agent bridge");

            // Send shutdown notification
            if let Err(e) = subprocess.send_notification("shutdown", serde_json::json!({})).await {
                warn!("Failed to send shutdown notification: {}", e);
            }

            // Give agent 5 seconds to gracefully shutdown
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

            // Terminate subprocess
            subprocess.terminate().await?;

            info!("Agent bridge stopped");
        }

        Ok(())
    }

    /// Subscribe to agent notifications for a session.
    pub async fn subscribe(&self, session_id: String) -> mpsc::UnboundedReceiver<AgentNotification> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut subs = self.notification_subscribers.write().await;
        subs.insert(session_id.clone(), tx);
        debug!("Subscribed to notifications for session: {}", session_id);
        rx
    }

    /// Unsubscribe from agent notifications for a session.
    pub async fn unsubscribe(&self, session_id: &str) {
        let mut subs = self.notification_subscribers.write().await;
        subs.remove(session_id);
        debug!("Unsubscribed from notifications for session: {}", session_id);
    }

    /// Send a JSON-RPC request to the agent and wait for response.
    async fn send_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, GatewayError> {
        let subprocess_guard = self.subprocess.read().await;
        let subprocess = subprocess_guard
            .as_ref()
            .ok_or_else(|| GatewayError::Platform("Agent subprocess not running".to_string()))?;

        // Allocate request ID
        let id = {
            let mut next_id = self.next_id.write().await;
            let id = *next_id;
            *next_id += 1;
            id
        };

        // Create oneshot channel for response
        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self.pending_requests.write().await;
            pending.insert(id, tx);
        }

        // Send request
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id,
        };

        subprocess.send_request(&request).await?;

        // Wait for response with timeout
        let timeout = tokio::time::Duration::from_secs(self.config.request_timeout_secs);
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(response)) => {
                if let Some(error) = response.error {
                    return Err(GatewayError::Platform(format!(
                        "Agent error {}: {}",
                        error.code, error.message
                    )));
                }
                response.result.ok_or_else(|| {
                    GatewayError::Platform("Agent response missing result".to_string())
                })
            }
            Ok(Err(_)) => Err(GatewayError::Platform(
                "Agent response channel closed".to_string(),
            )),
            Err(_) => {
                // Timeout - remove from pending
                let mut pending = self.pending_requests.write().await;
                pending.remove(&id);
                Err(GatewayError::Platform(format!(
                    "Agent request timed out after {}s: {}",
                    self.config.request_timeout_secs, method
                )))
            }
        }
    }

    /// Handle a message from the agent (response or notification).
    async fn handle_agent_message(
        line: &str,
        pending_requests: &Arc<RwLock<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
        notification_subscribers: &Arc<RwLock<HashMap<String, mpsc::UnboundedSender<AgentNotification>>>>,
    ) -> Result<(), GatewayError> {
        debug!("Received from agent: {}", line);

        // Try to parse as response first
        if let Ok(response) = serde_json::from_str::<JsonRpcResponse>(line) {
            let mut pending = pending_requests.write().await;
            if let Some(tx) = pending.remove(&response.id) {
                let _ = tx.send(response);
            } else {
                warn!("Received response for unknown request ID: {}", response.id);
            }
            return Ok(());
        }

        // Try to parse as notification
        if let Ok(notification) = serde_json::from_str::<JsonRpcNotification>(line) {
            let agent_notif = AgentNotification::from_rpc(&notification)?;

            // Broadcast to relevant subscribers
            let subs = notification_subscribers.read().await;

            // Extract session_id from notification params
            if let Some(session_id) = notification.params.get("session_id").and_then(|v| v.as_str()) {
                if let Some(tx) = subs.get(session_id) {
                    if let Err(e) = tx.send(agent_notif) {
                        warn!("Failed to send notification to subscriber {}: {}", session_id, e);
                    }
                }
            } else {
                warn!("Notification missing session_id: {}", notification.method);
            }

            return Ok(());
        }

        warn!("Failed to parse agent message: {}", line);
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Public API - Session Management
    // -------------------------------------------------------------------------

    /// Start a new agent session.
    pub async fn start_session(
        &self,
        session_id: String,
        platform: String,
        chat_id: String,
        user_id: String,
        config: SessionConfig,
    ) -> Result<StartSessionResponse, GatewayError> {
        let params = serde_json::json!({
            "session_id": session_id,
            "platform": platform,
            "chat_id": chat_id,
            "user_id": user_id,
            "config": config,
        });

        let result = self.send_request("start_session", params).await?;
        serde_json::from_value(result)
            .map_err(|e| GatewayError::Platform(format!("Invalid start_session response: {}", e)))
    }

    /// Handle an inbound message from a user.
    pub async fn handle_message(
        &self,
        session_id: String,
        text: String,
        attachments: Vec<MessageAttachment>,
        reply_to_message_id: Option<String>,
    ) -> Result<HandleMessageResponse, GatewayError> {
        let params = serde_json::json!({
            "session_id": session_id,
            "text": text,
            "attachments": attachments,
            "reply_to_message_id": reply_to_message_id,
        });

        let result = self.send_request("handle_message", params).await?;
        serde_json::from_value(result)
            .map_err(|e| GatewayError::Platform(format!("Invalid handle_message response: {}", e)))
    }

    /// Interrupt an ongoing agent execution.
    pub async fn interrupt(&self, session_id: String, reason: String) -> Result<(), GatewayError> {
        let params = serde_json::json!({
            "session_id": session_id,
            "reason": reason,
        });

        self.send_request("interrupt", params).await?;
        Ok(())
    }

    /// End a session gracefully.
    pub async fn end_session(&self, session_id: String) -> Result<EndSessionResponse, GatewayError> {
        let params = serde_json::json!({
            "session_id": session_id,
        });

        let result = self.send_request("end_session", params).await?;
        serde_json::from_value(result)
            .map_err(|e| GatewayError::Platform(format!("Invalid end_session response: {}", e)))
    }

    /// Ping the agent (heartbeat).
    pub async fn ping(&self) -> Result<PingResponse, GatewayError> {
        let result = self.send_request("ping", serde_json::json!({})).await?;
        serde_json::from_value(result)
            .map_err(|e| GatewayError::Platform(format!("Invalid ping response: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bridge_config_default() {
        let config = BridgeConfig::default();
        assert_eq!(config.python_path, "python3");
        assert_eq!(config.agent_module, "hermes_cli.agent_bridge");
        assert_eq!(config.heartbeat_interval_secs, 30);
        assert!(config.auto_restart);
    }

    #[tokio::test]
    async fn test_bridge_lifecycle() {
        let config = BridgeConfig::default();
        let bridge = AgentBridge::new(config);

        // Bridge should not be started yet
        let subprocess_guard = bridge.subprocess.read().await;
        assert!(subprocess_guard.is_none());
    }
}
