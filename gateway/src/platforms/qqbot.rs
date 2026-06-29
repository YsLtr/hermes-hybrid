//! QQBot platform adapter
//!
//! Connects to QQ Bot Official API v2 via WebSocket and REST API.

use futures_util::{SinkExt, StreamExt};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
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
    #[serde(default)]
    pub markdown_support: bool,
    #[serde(default = "default_true")]
    pub c2c_streaming: bool,
    #[serde(default = "default_true")]
    pub progress_coalesce: bool,
    #[serde(default = "default_true")]
    pub metadata_footer: bool,
    #[serde(default = "default_true")]
    pub notify_on_stream_end: bool,
    #[serde(default = "default_max_progress_messages")]
    pub max_progress_messages: usize,
}

fn default_true() -> bool {
    true
}

fn default_max_progress_messages() -> usize {
    2
}

// Message types
const MSG_TYPE_TEXT: i64 = 0;
const MSG_TYPE_MARKDOWN: i64 = 2;
const MSG_TYPE_INPUT_NOTIFY: i64 = 6;

// Typing indicator config
const TYPING_INPUT_SECONDS: i64 = 60;
const TYPING_DEBOUNCE_SECONDS: u64 = 50;

/// WebSocket state
#[derive(Debug, Clone, Default)]
struct WsState {
    session_id: Option<String>,
    last_seq: Option<i64>,
}

/// C2C stream state
#[derive(Debug, Clone, Default)]
struct C2cStreamState {
    id: Option<String>,
    index: u64,
    active: bool,
    msg_type: Option<i64>,
}

/// Progress state
#[derive(Debug, Clone, Default)]
struct ProgressState {
    sent_count: usize,
    last_sent_idx: usize,
    last_line: Option<String>,
}

/// Stream notice state
#[derive(Debug, Clone)]
struct StreamNoticeState {
    last_sent: Option<Instant>,
    recent_sent: Vec<Instant>,
}

impl Default for StreamNoticeState {
    fn default() -> Self {
        Self {
            last_sent: None,
            recent_sent: Vec::new(),
        }
    }
}

/// Platform turn metadata
#[derive(Debug, Clone, Default)]
pub struct PlatformTurnMetadata {
    pub model: Option<String>,
    pub provider: Option<String>,
    pub ttft_ms: Option<u64>,
    pub total_ms: Option<u64>,
    pub tool_count: Option<usize>,
}

/// QQBot adapter
pub struct QQBotAdapter {
    config: QQBotConfig,
    http_client: HttpClient,
    inbound_tx: mpsc::Sender<InboundMessage>,
    access_token: Arc<tokio::sync::RwLock<Option<String>>>,
    ws_state: Arc<tokio::sync::Mutex<WsState>>,
    last_msg_id: Arc<tokio::sync::Mutex<HashMap<String, String>>>,
    typing_sent_at: Arc<tokio::sync::Mutex<HashMap<String, Instant>>>,
    c2c_stream_state: Arc<tokio::sync::Mutex<HashMap<String, C2cStreamState>>>,
    progress_state: Arc<tokio::sync::Mutex<HashMap<String, ProgressState>>>,
    notice_state: Arc<tokio::sync::Mutex<StreamNoticeState>>,
    turn_metadata: Arc<tokio::sync::Mutex<HashMap<String, PlatformTurnMetadata>>>,
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
            last_msg_id: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            typing_sent_at: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            c2c_stream_state: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            progress_state: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            notice_state: Arc::new(tokio::sync::Mutex::new(StreamNoticeState::default())),
            turn_metadata: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
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

        // Intents: C2C (1<<25) + Guild @mentions (1<<30) + DMs (1<<12) + Guild messages (1<<26)
        let intents = (1u64 << 25) | (1u64 << 30) | (1u64 << 12) | (1u64 << 26);

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

        let msg_id = d["id"].as_str().map(|s| s.to_string());

        // Store message ID for typing indicator
        if let Some(ref id) = msg_id {
            self.last_msg_id
                .lock()
                .await
                .insert(chat_id.clone(), id.clone());
        }

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

    fn looks_like_group_chat(chat_id: &str) -> bool {
        let id = chat_id.trim().to_ascii_lowercase();
        id.starts_with("group_") || id.starts_with("grp_") || id.starts_with("qqgroup_")
    }

    fn next_msg_seq(seed: &str) -> i64 {
        let base = chrono::Utc::now().timestamp_millis();
        let salt = seed
            .bytes()
            .fold(0_i64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as i64));
        (base.wrapping_add(salt).rem_euclid(65_535)).max(1)
    }

    async fn post_qq_message(
        &self,
        endpoint: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, QQBotError> {
        let token = self
            .access_token
            .read()
            .await
            .as_ref()
            .ok_or_else(|| QQBotError::AuthError("No access token".to_string()))?
            .clone();

        let resp = self
            .http_client
            .post(endpoint)
            .header("Authorization", format!("QQBot {}", token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| QQBotError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(QQBotError::SendError(format!(
                "QQBot API error ({status}): {text}"
            )));
        }

        resp.json()
            .await
            .map_err(|e| QQBotError::HttpError(e.to_string()))
    }

    async fn send_text_inner(
        &self,
        chat_id: &str,
        text: &str,
        append_footer: bool,
        clear_after: bool,
    ) -> Result<(), QQBotError> {
        let api_base = self
            .config
            .api_base
            .as_deref()
            .unwrap_or("https://api.sgroup.qq.com");

        let endpoint = if Self::looks_like_group_chat(chat_id) {
            format!("{}/v2/groups/{}/messages", api_base, chat_id)
        } else {
            format!("{}/v2/users/{}/messages", api_base, chat_id)
        };

        let footer = if append_footer {
            self.format_metadata_footer(chat_id).await
        } else {
            String::new()
        };

        let final_text = if footer.is_empty() {
            text.to_string()
        } else {
            format!("{}{}", text.trim_end(), footer)
        };

        let body = if self.config.markdown_support {
            serde_json::json!({
                "msg_type": MSG_TYPE_MARKDOWN,
                "markdown": { "content": final_text },
                "msg_seq": Self::next_msg_seq(chat_id)
            })
        } else {
            serde_json::json!({
                "msg_type": MSG_TYPE_TEXT,
                "content": final_text,
                "msg_seq": Self::next_msg_seq(chat_id)
            })
        };

        self.post_qq_message(&endpoint, body).await?;

        if clear_after {
            self.clear_turn_state(chat_id).await;
        }

        Ok(())
    }

    async fn clear_stream_progress_state(&self, chat_id: &str) {
        self.progress_state.lock().await.remove(chat_id);
        self.c2c_stream_state.lock().await.remove(chat_id);
    }

    async fn clear_metadata_state(&self, chat_id: &str) {
        self.turn_metadata.lock().await.remove(chat_id);
    }

    async fn clear_turn_state(&self, chat_id: &str) {
        self.clear_stream_progress_state(chat_id).await;
        self.clear_metadata_state(chat_id).await;
    }

    async fn format_metadata_footer(&self, chat_id: &str) -> String {
        if !self.config.metadata_footer {
            return String::new();
        }
        let meta = self.turn_metadata.lock().await.get(chat_id).cloned();
        let Some(meta) = meta else {
            return String::new();
        };

        let mut parts = Vec::new();
        if let Some(model) = meta.model.filter(|s| !s.trim().is_empty()) {
            parts.push(format!("model {}", model));
        }
        if let Some(provider) = meta.provider.filter(|s| !s.trim().is_empty()) {
            parts.push(format!("provider {}", provider));
        }
        if let Some(ttft) = meta.ttft_ms {
            parts.push(format!("ttft {}ms", ttft));
        }
        if let Some(total) = meta.total_ms {
            parts.push(format!("time {:.1}s", total as f64 / 1000.0));
        }
        if let Some(tools) = meta.tool_count {
            if tools > 0 {
                parts.push(format!("tools {}", tools));
            }
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("\n\n---\n`{}`", parts.join(" | "))
        }
    }

    /// Set turn metadata
    pub async fn set_turn_metadata(
        &self,
        chat_id: &str,
        meta: PlatformTurnMetadata,
    ) -> Result<(), QQBotError> {
        self.turn_metadata
            .lock()
            .await
            .insert(chat_id.to_string(), meta);
        Ok(())
    }

    /// Send typing indicator (C2C only)
    pub async fn send_typing(&self, chat_id: &str) -> Result<(), QQBotError> {
        // Only C2C supports typing indicator
        if Self::looks_like_group_chat(chat_id) {
            return Ok(());
        }

        // Need the originating message ID
        let msg_id = {
            let guard = self.last_msg_id.lock().await;
            guard.get(chat_id).cloned()
        };
        let Some(msg_id) = msg_id else {
            debug!("No message ID stored for chat {}, skipping typing indicator", chat_id);
            return Ok(());
        };

        // Debounce - skip if sent recently
        let now = Instant::now();
        {
            let mut guard = self.typing_sent_at.lock().await;
            if let Some(last) = guard.get(chat_id) {
                if now.duration_since(*last) < std::time::Duration::from_secs(TYPING_DEBOUNCE_SECONDS) {
                    return Ok(());
                }
            }
            guard.insert(chat_id.to_string(), now);
        }

        let api_base = self
            .config
            .api_base
            .as_deref()
            .unwrap_or("https://api.sgroup.qq.com");

        let url = format!("{}/v2/users/{}/messages", api_base, chat_id);

        let body = serde_json::json!({
            "msg_type": MSG_TYPE_INPUT_NOTIFY,
            "msg_id": msg_id,
            "input_notify": {
                "input_type": 1,
                "input_second": TYPING_INPUT_SECONDS,
            },
            "msg_seq": Self::next_msg_seq(chat_id),
        });

        match self.post_qq_message(&url, body).await {
            Ok(_) => {
                debug!("Sent typing indicator to chat: {}", chat_id);
                Ok(())
            }
            Err(e) => {
                debug!("QQBot send_typing failed (non-fatal): {}", e);
                Ok(())
            }
        }
    }

    /// Send C2C stream chunk
    pub async fn send_stream_chunk(
        &self,
        chat_id: &str,
        text: &str,
        reply_to: Option<&str>,
        final_chunk: bool,
    ) -> Result<bool, QQBotError> {
        if !self.config.c2c_streaming || Self::looks_like_group_chat(chat_id) {
            return Ok(false);
        }

        let mut content = text.chars().take(4096).collect::<String>();
        if final_chunk {
            let footer = self.format_metadata_footer(chat_id).await;
            if !footer.is_empty() {
                content = format!("{}{}", content.trim_end(), footer);
            }
            if !content.ends_with('\n') {
                content.push('\n');
            }
        }
        if content.trim().is_empty() && !final_chunk {
            return Ok(false);
        }

        let mut states = self.c2c_stream_state.lock().await;
        let state = states.entry(chat_id.to_string()).or_default();

        let mut stream_payload = serde_json::json!({
            "state": if final_chunk { 10 } else { 1 },
            "index": state.index,
            "reset": false
        });
        if let Some(id) = &state.id {
            stream_payload["id"] = serde_json::Value::String(id.clone());
        }

        let use_markdown = self.config.markdown_support && state.msg_type != Some(0);
        let mut body = if use_markdown {
            serde_json::json!({
                "msg_type": MSG_TYPE_MARKDOWN,
                "markdown": { "content": content },
                "msg_seq": Self::next_msg_seq(chat_id),
                "stream": stream_payload
            })
        } else {
            serde_json::json!({
                "msg_type": MSG_TYPE_TEXT,
                "content": content,
                "msg_seq": Self::next_msg_seq(chat_id),
                "stream": stream_payload
            })
        };

        if let Some(reply_to) = reply_to.filter(|s| !s.trim().is_empty()) {
            body["msg_id"] = serde_json::Value::String(reply_to.to_string());
        }

        let api_base = self
            .config
            .api_base
            .as_deref()
            .unwrap_or("https://api.sgroup.qq.com");
        let endpoint = format!("{}/v2/users/{}/messages", api_base, chat_id);

        debug!(
            chat_id = %chat_id,
            final_chunk,
            content_chars = content.chars().count(),
            stream_state = if final_chunk { 10 } else { 1 },
            msg_type = if use_markdown { 2 } else { 0 },
            stream_id = state.id.as_deref().unwrap_or(""),
            stream_index = state.index,
            "QQBot sending native stream chunk"
        );
        drop(states);

        let mut actual_msg_type = if use_markdown { MSG_TYPE_MARKDOWN } else { MSG_TYPE_TEXT };
        let result = self.post_qq_message(&endpoint, body.clone()).await;
        let data = match result {
            Ok(data) => data,
            Err(err) if use_markdown => {
                let lowered = err.to_string().to_ascii_lowercase();
                if lowered.contains("markdown") || lowered.contains("not allowed") {
                    warn!("QQBot stream markdown rejected, locking stream to plain text");
                    body.as_object_mut().map(|obj| {
                        obj.remove("markdown");
                        obj.insert("msg_type".to_string(), serde_json::json!(MSG_TYPE_TEXT));
                        obj.insert("content".to_string(), serde_json::json!(content));
                    });
                    actual_msg_type = MSG_TYPE_TEXT;
                    let data = self.post_qq_message(&endpoint, body).await?;
                    let mut states = self.c2c_stream_state.lock().await;
                    states.entry(chat_id.to_string()).or_default().msg_type = Some(MSG_TYPE_TEXT);
                    data
                } else {
                    return Err(err);
                }
            }
            Err(err) => return Err(err),
        };

        let mut states = self.c2c_stream_state.lock().await;
        let state = states.entry(chat_id.to_string()).or_default();
        if final_chunk {
            *state = C2cStreamState::default();
        } else {
            state.msg_type = Some(actual_msg_type);
            state.id = data
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .or_else(|| Some(uuid::Uuid::new_v4().to_string()));
            state.index = state.index.saturating_add(1);
            state.active = true;
        }
        drop(states);

        if final_chunk {
            self.clear_stream_progress_state(chat_id).await;
            if !self.config.notify_on_stream_end {
                self.clear_metadata_state(chat_id).await;
            }
        }

        Ok(true)
    }

    /// Send progress card
    pub async fn send_progress_card(
        &self,
        chat_id: &str,
        lines: &[String],
    ) -> Result<(), QQBotError> {
        if !self.config.progress_coalesce || lines.is_empty() {
            return Ok(());
        }

        let mut state_map = self.progress_state.lock().await;
        let state = state_map.entry(chat_id.to_string()).or_default();
        if state.sent_count >= self.config.max_progress_messages {
            return Ok(());
        }

        let mut fresh = Vec::new();
        for line in lines.iter().skip(state.last_sent_idx) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if state.last_line.as_deref() == Some(trimmed) {
                state.last_sent_idx += 1;
                continue;
            }
            fresh.push(trimmed.to_string());
            state.last_line = Some(trimmed.to_string());
            state.last_sent_idx += 1;
        }
        if fresh.is_empty() {
            return Ok(());
        }
        state.sent_count += 1;
        drop(state_map);

        let body = format!("**Progress**\n{}", fresh.join("\n"));
        self.send_text_inner(chat_id, &body, false, false).await
    }

    /// Send stream end notice
    pub async fn send_stream_end_notice(&self, chat_id: &str) -> Result<(), QQBotError> {
        if !self.config.notify_on_stream_end {
            return Ok(());
        }

        let now = Instant::now();
        let mut state = self.notice_state.lock().await;
        if state
            .last_sent
            .map(|last| now.duration_since(last) < std::time::Duration::from_secs(3))
            .unwrap_or(false)
        {
            return Ok(());
        }
        state
            .recent_sent
            .retain(|t| now.duration_since(*t) < std::time::Duration::from_secs(300));
        if state.recent_sent.len() >= 3 {
            return Ok(());
        }
        state.last_sent = Some(now);
        state.recent_sent.push(now);
        drop(state);

        let footer = self.format_metadata_footer(chat_id).await;
        let line = if footer.is_empty() {
            "completed".to_string()
        } else {
            format!(
                "completed {}",
                footer.replace('\n', " ").replace("---", "").trim()
            )
        };
        self.send_text_inner(chat_id, &line, false, true).await
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

    /// Maintenance cleanup to prevent unbounded HashMap growth
    pub async fn maintenance_prune(&self) {
        const MAX_TRACKED_CHATS: usize = 512;

        let mut progress = self.progress_state.lock().await;
        if progress.len() > MAX_TRACKED_CHATS {
            progress.clear();
        }
        drop(progress);

        let mut stream = self.c2c_stream_state.lock().await;
        if stream.len() > MAX_TRACKED_CHATS {
            stream.clear();
        }
        drop(stream);

        let mut meta = self.turn_metadata.lock().await;
        if meta.len() > MAX_TRACKED_CHATS {
            meta.clear();
        }
        drop(meta);

        let mut last_msg = self.last_msg_id.lock().await;
        if last_msg.len() > MAX_TRACKED_CHATS {
            last_msg.clear();
        }
        drop(last_msg);

        let mut typing = self.typing_sent_at.lock().await;
        if typing.len() > MAX_TRACKED_CHATS {
            typing.clear();
        }
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
