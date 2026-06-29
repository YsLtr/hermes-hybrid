//! Type definitions for agent bridge communication.

use serde::{Deserialize, Serialize};

// -----------------------------------------------------------------------------
// Session Management
// -----------------------------------------------------------------------------

/// Configuration for an agent session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub model: Option<String>,
    pub max_turns: Option<u32>,
    pub toolsets: Vec<String>,
    pub personality: Option<String>,
    pub budget: Option<BudgetConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub max_usd: Option<f64>,
    pub max_input_tokens: Option<u64>,
    pub max_output_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartSessionResponse {
    pub status: String,
    pub session_id: String,
    pub loaded_tools: u32,
    pub memory_snapshots: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndSessionResponse {
    pub status: String,
    pub turns_processed: u32,
    pub total_tokens: u64,
}

// -----------------------------------------------------------------------------
// Message Handling
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageAttachment {
    #[serde(rename = "type")]
    pub attachment_type: String, // "image", "audio", "video", "document"
    pub url: String,
    pub caption: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandleMessageResponse {
    pub status: String, // "processing"
    pub message_id: String,
}

// -----------------------------------------------------------------------------
// Notifications
// -----------------------------------------------------------------------------

/// Agent notification sent to the gateway.
#[derive(Debug, Clone)]
pub enum AgentNotification {
    /// Agent started typing (for typing indicator)
    TypingStart {
        session_id: String,
        chat_id: String,
    },

    /// Stream chunk from LLM response
    StreamChunk {
        session_id: String,
        chat_id: String,
        text: String,
        is_final: bool,
    },

    /// Tool execution started
    ToolStarted {
        session_id: String,
        chat_id: String,
        tool_name: String,
        tool_params: serde_json::Value,
    },

    /// Tool execution completed
    ToolCompleted {
        session_id: String,
        chat_id: String,
        tool_name: String,
        success: bool,
        duration_ms: u64,
        result_preview: Option<String>,
    },

    /// Message processing complete (final response)
    MessageComplete {
        session_id: String,
        chat_id: String,
        text: String,
        metadata: MessageMetadata,
    },

    /// Error occurred during processing
    Error {
        session_id: String,
        chat_id: String,
        error_type: String,
        message: String,
        retry_after_secs: Option<u64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    pub model: String,
    pub provider: String,
    pub ttft_ms: Option<u64>,
    pub total_time_ms: u64,
    pub tool_count: u32,
    pub tokens: TokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
}

impl AgentNotification {
    /// Parse a JSON-RPC notification into an AgentNotification.
    pub fn from_rpc(notif: &crate::agent_bridge::protocol::JsonRpcNotification) -> Result<Self, hermes_core::errors::GatewayError> {
        let params = &notif.params;

        match notif.method.as_str() {
            "typing_start" => Ok(Self::TypingStart {
                session_id: params["session_id"]
                    .as_str()
                    .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing session_id".to_string()))?
                    .to_string(),
                chat_id: params["chat_id"]
                    .as_str()
                    .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing chat_id".to_string()))?
                    .to_string(),
            }),

            "stream_chunk" => Ok(Self::StreamChunk {
                session_id: params["session_id"]
                    .as_str()
                    .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing session_id".to_string()))?
                    .to_string(),
                chat_id: params["chat_id"]
                    .as_str()
                    .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing chat_id".to_string()))?
                    .to_string(),
                text: params["text"]
                    .as_str()
                    .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing text".to_string()))?
                    .to_string(),
                is_final: params["is_final"].as_bool().unwrap_or(false),
            }),

            "tool_started" => Ok(Self::ToolStarted {
                session_id: params["session_id"]
                    .as_str()
                    .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing session_id".to_string()))?
                    .to_string(),
                chat_id: params["chat_id"]
                    .as_str()
                    .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing chat_id".to_string()))?
                    .to_string(),
                tool_name: params["tool_name"]
                    .as_str()
                    .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing tool_name".to_string()))?
                    .to_string(),
                tool_params: params["tool_params"].clone(),
            }),

            "tool_completed" => Ok(Self::ToolCompleted {
                session_id: params["session_id"]
                    .as_str()
                    .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing session_id".to_string()))?
                    .to_string(),
                chat_id: params["chat_id"]
                    .as_str()
                    .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing chat_id".to_string()))?
                    .to_string(),
                tool_name: params["tool_name"]
                    .as_str()
                    .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing tool_name".to_string()))?
                    .to_string(),
                success: params["success"].as_bool().unwrap_or(false),
                duration_ms: params["duration_ms"].as_u64().unwrap_or(0),
                result_preview: params["result_preview"].as_str().map(|s| s.to_string()),
            }),

            "message_complete" => {
                let metadata: MessageMetadata = serde_json::from_value(params["metadata"].clone())
                    .map_err(|e| hermes_core::errors::GatewayError::Platform(format!("Invalid metadata: {}", e)))?;

                Ok(Self::MessageComplete {
                    session_id: params["session_id"]
                        .as_str()
                        .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing session_id".to_string()))?
                        .to_string(),
                    chat_id: params["chat_id"]
                        .as_str()
                        .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing chat_id".to_string()))?
                        .to_string(),
                    text: params["text"]
                        .as_str()
                        .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing text".to_string()))?
                        .to_string(),
                    metadata,
                })
            }

            "error" => Ok(Self::Error {
                session_id: params["session_id"]
                    .as_str()
                    .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing session_id".to_string()))?
                    .to_string(),
                chat_id: params["chat_id"]
                    .as_str()
                    .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing chat_id".to_string()))?
                    .to_string(),
                error_type: params["error_type"]
                    .as_str()
                    .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing error_type".to_string()))?
                    .to_string(),
                message: params["message"]
                    .as_str()
                    .ok_or_else(|| hermes_core::errors::GatewayError::Platform("Missing message".to_string()))?
                    .to_string(),
                retry_after_secs: params["retry_after_secs"].as_u64(),
            }),

            _ => Err(hermes_core::errors::GatewayError::Platform(format!(
                "Unknown notification method: {}",
                notif.method
            ))),
        }
    }
}

// -----------------------------------------------------------------------------
// Ping / Heartbeat
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingResponse {
    pub status: String, // "alive"
    pub sessions: u32,
}
