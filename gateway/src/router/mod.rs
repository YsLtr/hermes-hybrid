//! Message Router
//!
//! Routes messages between platform adapters and the agent bridge.

use crate::agent_bridge::AgentBridge;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

pub mod session;
pub mod stream;

pub use session::{Session, SessionConfig, SessionManager};
pub use stream::{StreamEvent, StreamManager};

/// Message to be routed to the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundMessage {
    pub platform: String,
    pub chat_id: String,
    pub user_id: String,
    pub text: String,
    pub attachments: Vec<Attachment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<String>,
}

/// Attachment in a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub url: String,
    pub mime_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

/// Response from the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub platform: String,
    pub chat_id: String,
    pub text: String,
    pub is_streaming: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Router that manages message flow
pub struct Router {
    agent_bridge: Arc<AgentBridge>,
    session_manager: Arc<SessionManager>,
    stream_manager: Arc<StreamManager>,
    outbound_tx: mpsc::Sender<OutboundMessage>,
}

impl Router {
    pub fn new(
        agent_bridge: Arc<AgentBridge>,
        outbound_tx: mpsc::Sender<OutboundMessage>,
    ) -> Self {
        let session_manager = Arc::new(SessionManager::new());
        let stream_manager = Arc::new(StreamManager::new());

        Self {
            agent_bridge,
            session_manager,
            stream_manager,
            outbound_tx,
        }
    }

    /// Route an inbound message to the agent
    pub async fn route_inbound(
        &self,
        msg: InboundMessage,
    ) -> Result<(), RouterError> {
        let session_id = format!("{}_{}", msg.platform, msg.chat_id);

        debug!("Routing message to session: {}", session_id);

        // Ensure session exists
        if !self.session_manager.has_session(&session_id).await {
            info!("Creating new session: {}", session_id);

            let config = SessionConfig {
                platform: msg.platform.clone(),
                chat_id: msg.chat_id.clone(),
                user_id: msg.user_id.clone(),
                model: None,
                max_turns: None,
                toolsets: None,
            };

            self.session_manager
                .create_session(&session_id, config.clone())
                .await
                .map_err(|e| RouterError::SessionError(e))?;

            // Start session in agent bridge
            let agent_config = crate::agent_bridge::types::SessionConfig {
                model: Some("claude-opus-4".to_string()),
                max_turns: Some(90),
                toolsets: vec![],
                personality: None,
                budget: None,
            };

            self.agent_bridge
                .start_session(
                    session_id.clone(),
                    msg.platform.clone(),
                    msg.chat_id.clone(),
                    msg.user_id.clone(),
                    agent_config,
                )
                .await
                .map_err(|e| RouterError::AgentBridgeError(e.to_string()))?;
        }

        // Convert attachments
        let attachments = msg
            .attachments
            .iter()
            .map(|a| {
                // Determine attachment type based on MIME type
                let attachment_type = if a.mime_type.starts_with("image/") {
                    "image"
                } else if a.mime_type.starts_with("audio/") {
                    "audio"
                } else if a.mime_type.starts_with("video/") {
                    "video"
                } else {
                    "document"
                };

                crate::agent_bridge::types::MessageAttachment {
                    attachment_type: attachment_type.to_string(),
                    url: a.url.clone(),
                    caption: None,
                }
            })
            .collect();

        // Start streaming notifications listener
        let stream_manager = self.stream_manager.clone();
        let outbound_tx = self.outbound_tx.clone();
        let platform = msg.platform.clone();
        let chat_id = msg.chat_id.clone();
        let session_id_clone = session_id.clone();

        tokio::spawn(async move {
            if let Err(e) = stream_manager
                .handle_stream(&session_id_clone, platform, chat_id, outbound_tx)
                .await
            {
                error!("Stream handling error: {}", e);
            }
        });

        // Send message to agent
        self.agent_bridge
            .handle_message(session_id, msg.text, attachments, msg.reply_to_message_id)
            .await
            .map_err(|e| RouterError::AgentBridgeError(e.to_string()))?;

        Ok(())
    }

    /// Interrupt a session
    pub async fn interrupt_session(&self, session_id: &str) -> Result<(), RouterError> {
        info!("Interrupting session: {}", session_id);

        self.agent_bridge
            .interrupt(session_id.to_string(), "user_requested".to_string())
            .await
            .map_err(|e| RouterError::AgentBridgeError(e.to_string()))?;

        Ok(())
    }

    /// End a session
    pub async fn end_session(&self, session_id: &str) -> Result<(), RouterError> {
        info!("Ending session: {}", session_id);

        self.agent_bridge
            .end_session(session_id.to_string())
            .await
            .map_err(|e| RouterError::AgentBridgeError(e.to_string()))?;

        self.session_manager.remove_session(session_id).await;

        Ok(())
    }

    pub fn session_manager(&self) -> &SessionManager {
        &self.session_manager
    }

    pub fn stream_manager(&self) -> &StreamManager {
        &self.stream_manager
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("Agent bridge error: {0}")]
    AgentBridgeError(String),

    #[error("Session error: {0}")]
    SessionError(String),

    #[error("Stream error: {0}")]
    StreamError(String),
}
