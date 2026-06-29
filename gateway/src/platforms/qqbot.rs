//! QQBot platform adapter
//!
//! Connects to QQ Bot Official API v2 via WebSocket and REST API.

use futures_util::{SinkExt, StreamExt};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use crate::router::{Attachment, InboundMessage, OutboundMessage};

/// QQBot configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQBotConfig {
    pub app_id: String,
    pub client_secret: String,
    pub api_base: Option<String>,
    pub sandbox: Option<bool>,
}

/// WebSocket state
#[derive(Debug, Clone, Default)]
struct WsState {
    session_id: Option<String>,
    last_seq: Option<i64>,
}

/// QQBot adapter
pub struct QQBotAdapter {
    config: QQBotConfig,
    http_client: HttpClient,
    inbound_tx: mpsc::Sender<InboundMessage>,
    access_token: Arc<tokio::sync::RwLock<Option<String>>>,
    ws_state: Arc<tokio::sync::Mutex<WsState>>,
}

impl QQBotAdapter {
    pub fn new(
        config: QQBotConfig,
        inbound_tx: mpsc::Sender<InboundMessage>,
    ) -> Self {
        let http_client = HttpClient::builder()
            .user_agent("Hermes-Hybrid-Gateway/0.1.0")
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            http_client,
            inbound_tx,
            access_token: Arc::new(tokio::sync::RwLock::new(None)),
            ws_state: Arc::new(tokio::sync::Mutex::new(WsState::default())),
        }
    }

    /// Start the adapter (connect to WebSocket gateway with auto-reconnect)
    pub async fn start(&self) -> Result<(), QQBotError> {
        info!("🤖 Starting QQBot adapter...");

        loop {
            match self.connect_and_listen().await {
                Ok(_) => {
                    warn!("WebSocket connection closed, reconnecting in 5 seconds...");
                }
                Err(e) => {
                    error!("WebSocket connection error: {}, reconnecting in 10 seconds...", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                    continue;
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }

    /// Connect to WebSocket and listen for messages
    async fn connect_and_listen(&self) -> Result<(), QQBotError> {
        // Re-authenticate before each connection
        self.authenticate().await?;

        // Get WebSocket gateway URL
        let gateway_url = self.get_gateway_url().await?;
        info!("WebSocket gateway URL: {}", gateway_url);

        // Connect to WebSocket
        let (ws_stream, _) = connect_async(&gateway_url)
            .await
            .map_err(|e| QQBotError::WebSocketError(e.to_string()))?;

        info!("✅ Connected to QQ WebSocket gateway");

        let (write, mut read) = ws_stream.split();
        let write = Arc::new(tokio::sync::Mutex::new(write));

        // Heartbeat task - will be started after receiving HELLO
        let write_clone = write.clone();
        let ws_state_clone = self.ws_state.clone();
        let mut heartbeat_interval: Option<tokio::time::Interval> = None;

        // Message handler loop
        let inbound_tx = self.inbound_tx.clone();
        loop {
            tokio::select! {
                // Heartbeat tick
                _ = async {
                    if let Some(interval) = &mut heartbeat_interval {
                        interval.tick().await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {
                    // Send QQ heartbeat (op=1) with last sequence
                    let seq = ws_state_clone.lock().await.last_seq;
                    let heartbeat = serde_json::json!({"op": 1, "d": seq});
                    let msg = Message::Text(serde_json::to_string(&heartbeat).unwrap());
                    if let Err(e) = write_clone.lock().await.send(msg).await {
                        error!("Failed to send heartbeat: {}", e);
                        break;
                    }
                    debug!("Sent heartbeat with seq={:?}", seq);
                }
                // WebSocket messages
                msg = read.next() => {
                    let Some(msg) = msg else {
                        warn!("WebSocket stream ended");
                        break;
                    };

                    match msg {
                        Ok(Message::Text(text)) => {
                            match self.handle_websocket_message(&text, &inbound_tx, &write).await {
                                Ok(Some(interval_ms)) => {
                                    // HELLO received, start heartbeat
                                    let every = std::time::Duration::from_millis((interval_ms as f64 * 0.8) as u64);
                                    heartbeat_interval = Some(tokio::time::interval(every.max(std::time::Duration::from_secs(1))));
                                    info!("Heartbeat started: {}ms interval", interval_ms);
                                }
                                Ok(None) => {
                                    // Normal message processing
                                }
                                Err(e) => {
                                    error!("Failed to handle WebSocket message: {}", e);
                                }
                            }
                        }
                        Ok(Message::Close(_)) => {
                            warn!("WebSocket closed by server");
                            break;
                        }
                        Ok(Message::Ping(data)) => {
                            let _ = write.lock().await.send(Message::Pong(data)).await;
                        }
                        Err(e) => {
                            error!("WebSocket error: {}", e);
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        warn!("WebSocket connection closed");
        Ok(())
    }

    /// Authenticate and get access token
    async fn authenticate(&self) -> Result<(), QQBotError> {
        let api_base = self
            .config
            .api_base
            .as_deref()
            .unwrap_or("https://bots.qq.com");

        let url = format!("{}/app/getAppAccessToken", api_base);

        let body = serde_json::json!({
            "appId": self.config.app_id,
            "clientSecret": self.config.client_secret,
        });

        let resp = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| QQBotError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(QQBotError::AuthError(format!(
                "Failed to authenticate: {} - {}",
                status, text
            )));
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| QQBotError::HttpError(e.to_string()))?;

        let access_token = data["access_token"]
            .as_str()
            .ok_or_else(|| QQBotError::AuthError("No access_token in response".to_string()))?
            .to_string();

        *self.access_token.write().await = Some(access_token.clone());

        info!("✅ QQBot authenticated");
        Ok(())
    }

    /// Get WebSocket gateway URL
    async fn get_gateway_url(&self) -> Result<String, QQBotError> {
        let api_base = self
            .config
            .api_base
            .as_deref()
            .unwrap_or("https://api.sgroup.qq.com");

        let url = format!("{}/gateway", api_base);

        let token = self
            .access_token
            .read()
            .await
            .as_ref()
            .ok_or_else(|| QQBotError::AuthError("No access token".to_string()))?
            .clone();

        let resp = self
            .http_client
            .get(&url)
            .header("Authorization", format!("QQBot {}", token))
            .send()
            .await
            .map_err(|e| QQBotError::HttpError(e.to_string()))?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| QQBotError::HttpError(e.to_string()))?;

        let gateway_url = data["url"]
            .as_str()
            .ok_or_else(|| QQBotError::WebSocketError("No gateway URL in response".to_string()))?
            .to_string();

        Ok(gateway_url)
    }

    /// Handle WebSocket message
    /// Returns Some(interval_ms) when HELLO is received
    async fn handle_websocket_message(
        &self,
        text: &str,
        inbound_tx: &mpsc::Sender<InboundMessage>,
        write: &Arc<tokio::sync::Mutex<futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>>>,
    ) -> Result<Option<u64>, QQBotError> {
        let data: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| QQBotError::ParseError(e.to_string()))?;

        // Update sequence number
        if let Some(seq) = data["s"].as_i64() {
            self.ws_state.lock().await.last_seq = Some(seq);
        }

        // Parse opcode
        let op = data["op"].as_u64();

        match op {
            Some(10) => {
                // HELLO - get heartbeat interval and send Identify
                let interval_ms = data["d"]["heartbeat_interval"]
                    .as_u64()
                    .unwrap_or(30_000);

                info!("Received HELLO, heartbeat interval: {}ms", interval_ms);
                self.send_identify(write).await?;

                Ok(Some(interval_ms))
            }
            Some(0) => {
                // Dispatch - handle events
                let event_type = data["t"].as_str().unwrap_or("UNKNOWN");
                let d = &data["d"];

                match event_type {
                    "READY" => {
                        if let Some(session_id) = d["session_id"].as_str() {
                            self.ws_state.lock().await.session_id = Some(session_id.to_string());
                            info!("✅ QQBot session READY (session_id: {})", session_id);
                        }
                    }
                    "MESSAGE_CREATE" | "C2C_MESSAGE_CREATE" | "GROUP_AT_MESSAGE_CREATE" => {
                        self.handle_message_event(d, inbound_tx).await?;
                    }
                    _ => {
                        debug!("Unhandled event type: {}", event_type);
                    }
                }

                Ok(None)
            }
            Some(11) => {
                // Heartbeat ACK
                debug!("Received heartbeat ACK");
                Ok(None)
            }
            Some(7) | Some(9) => {
                // Reconnect requested
                warn!("QQ gateway requested reconnect (op={})", op.unwrap());
                Err(QQBotError::WebSocketError("Reconnect requested".to_string()))
            }
            _ => {
                debug!("Unknown opcode: {:?}", op);
                Ok(None)
            }
        }
    }

    /// Send Identify message
    async fn send_identify(
        &self,
        write: &Arc<tokio::sync::Mutex<futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>>>,
    ) -> Result<(), QQBotError> {
        let token = self
            .access_token
            .read()
            .await
            .as_ref()
            .ok_or_else(|| QQBotError::AuthError("No access token".to_string()))?
            .clone();

        // Intents: C2C messages (1<<25) + Direct messages (1<<12) + Guild @mentions (1<<30) + Guild messages (1<<9)
        let intents = (1u64 << 25) | (1u64 << 30) | (1u64 << 12) | (1u64 << 9);

        let identify = serde_json::json!({
            "op": 2,
            "d": {
                "token": format!("QQBot {}", token),
                "intents": intents,
                "shard": [0, 1],
                "properties": {
                    "$os": "linux",
                    "$browser": "hermes-hybrid",
                    "$device": "hermes-hybrid"
                }
            }
        });

        let msg = Message::Text(serde_json::to_string(&identify).unwrap());
        write
            .lock()
            .await
            .send(msg)
            .await
            .map_err(|e| QQBotError::WebSocketError(e.to_string()))?;

        info!("Sent Identify message");
        Ok(())
    }

    /// Handle message event
    async fn handle_message_event(
        &self,
        data: &serde_json::Value,
        inbound_tx: &mpsc::Sender<InboundMessage>,
    ) -> Result<(), QQBotError> {
        let d = &data["d"];

        let chat_id = d["author"]["id"]
            .as_str()
            .or_else(|| d["guild_id"].as_str())
            .ok_or_else(|| QQBotError::ParseError("No chat_id".to_string()))?
            .to_string();

        let user_id = d["author"]["id"]
            .as_str()
            .ok_or_else(|| QQBotError::ParseError("No user_id".to_string()))?
            .to_string();

        let text = d["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        // Parse attachments
        let mut attachments = Vec::new();
        if let Some(attachments_array) = d["attachments"].as_array() {
            for att in attachments_array {
                if let Some(url) = att["url"].as_str() {
                    attachments.push(Attachment {
                        url: url.to_string(),
                        mime_type: att["content_type"]
                            .as_str()
                            .unwrap_or("application/octet-stream")
                            .to_string(),
                        filename: att["filename"].as_str().map(|s| s.to_string()),
                    });
                }
            }
        }

        let msg = InboundMessage {
            platform: "qqbot".to_string(),
            chat_id,
            user_id,
            text,
            attachments,
            reply_to_message_id: d["message_reference"]["message_id"]
                .as_str()
                .map(|s| s.to_string()),
        };

        inbound_tx
            .send(msg)
            .await
            .map_err(|e| QQBotError::InternalError(format!("Failed to send message: {}", e)))?;

        Ok(())
    }

    /// Send outbound message
    pub async fn send_message(&self, msg: OutboundMessage) -> Result<(), QQBotError> {
        let api_base = self
            .config
            .api_base
            .as_deref()
            .unwrap_or("https://api.sgroup.qq.com");

        let url = format!("{}/v2/users/{}/messages", api_base, msg.chat_id);

        let token = self
            .access_token
            .read()
            .await
            .as_ref()
            .ok_or_else(|| QQBotError::AuthError("No access token".to_string()))?
            .clone();

        let body = if msg.is_streaming {
            // Use C2C streaming protocol
            serde_json::json!({
                "content": msg.text,
                "msg_type": 0,
                "msg_id": uuid::Uuid::new_v4().to_string(),
            })
        } else {
            serde_json::json!({
                "content": msg.text,
                "msg_type": 0,
            })
        };

        let resp = self
            .http_client
            .post(&url)
            .header("Authorization", format!("QQBot {}", token))
            .json(&body)
            .send()
            .await
            .map_err(|e| QQBotError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(QQBotError::SendError(format!(
                "Failed to send message: {} - {}",
                status, text
            )));
        }

        debug!("Message sent to chat: {}", msg.chat_id);
        Ok(())
    }
}

/// QQBot error types
#[derive(Debug, thiserror::Error)]
pub enum QQBotError {
    #[error("HTTP error: {0}")]
    HttpError(String),

    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    #[error("Authentication error: {0}")]
    AuthError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Send error: {0}")]
    SendError(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}
