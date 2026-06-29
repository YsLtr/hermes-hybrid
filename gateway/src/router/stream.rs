//! Stream event management
//!
//! Handles streaming notifications from the agent bridge.

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::router::OutboundMessage;

/// Stream event from agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum StreamEvent {
    #[serde(rename = "typing_start")]
    TypingStart {
        session_id: String,
        chat_id: String,
    },

    #[serde(rename = "stream_chunk")]
    StreamChunk {
        session_id: String,
        chat_id: String,
        text: String,
        is_final: bool,
    },

    #[serde(rename = "message_complete")]
    MessageComplete {
        session_id: String,
        chat_id: String,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },

    #[serde(rename = "tool_started")]
    ToolStarted {
        session_id: String,
        chat_id: String,
        tool_name: String,
        tool_params: serde_json::Value,
    },

    #[serde(rename = "tool_completed")]
    ToolCompleted {
        session_id: String,
        chat_id: String,
        tool_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

/// Stream manager
pub struct StreamManager {
    // Future: add stream state tracking
}

impl StreamManager {
    pub fn new() -> Self {
        Self {}
    }

    /// Handle streaming events for a session
    pub async fn handle_stream(
        &self,
        session_id: &str,
        _platform: String,
        _chat_id: String,
        _outbound_tx: mpsc::Sender<OutboundMessage>,
    ) -> Result<(), String> {
        debug!("Stream handler started for session: {}", session_id);

        // Note: In the full implementation, this would subscribe to
        // notifications from the agent bridge and forward them as
        // outbound messages to the platform adapter.
        //
        // For now, the router handles this by parsing notifications
        // directly from the agent bridge.

        Ok(())
    }

    /// Process a stream event notification
    pub async fn process_event(
        &self,
        event: StreamEvent,
        platform: &str,
        outbound_tx: &mpsc::Sender<OutboundMessage>,
    ) -> Result<(), String> {
        match event {
            StreamEvent::TypingStart { chat_id, .. } => {
                debug!("Typing started in chat: {}", chat_id);
                // Platform adapter can show typing indicator
            }

            StreamEvent::StreamChunk {
                chat_id,
                text,
                is_final,
                ..
            } => {
                debug!("Stream chunk for chat {}: {} bytes", chat_id, text.len());

                let msg = OutboundMessage {
                    platform: platform.to_string(),
                    chat_id: chat_id.clone(),
                    text,
                    is_streaming: !is_final,
                    metadata: None,
                };

                outbound_tx
                    .send(msg)
                    .await
                    .map_err(|e| format!("Failed to send stream chunk: {}", e))?;
            }

            StreamEvent::MessageComplete {
                chat_id,
                text,
                metadata,
                ..
            } => {
                debug!("Message complete for chat: {}", chat_id);

                let msg = OutboundMessage {
                    platform: platform.to_string(),
                    chat_id: chat_id.clone(),
                    text,
                    is_streaming: false,
                    metadata,
                };

                outbound_tx
                    .send(msg)
                    .await
                    .map_err(|e| format!("Failed to send complete message: {}", e))?;
            }

            StreamEvent::ToolStarted {
                chat_id,
                tool_name,
                ..
            } => {
                debug!("Tool started in chat {}: {}", chat_id, tool_name);
                // Platform adapter can show tool execution status
            }

            StreamEvent::ToolCompleted {
                chat_id,
                tool_name,
                error,
                ..
            } => {
                if let Some(err) = error {
                    warn!("Tool {} failed in chat {}: {}", tool_name, chat_id, err);
                } else {
                    debug!("Tool {} completed in chat {}", tool_name, chat_id);
                }
            }
        }

        Ok(())
    }
}
