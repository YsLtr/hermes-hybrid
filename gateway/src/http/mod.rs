//! HTTP API server
//!
//! Provides REST API for external platforms to send messages.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::debug;

use crate::router::{InboundMessage, Router as MessageRouter};

/// API server state
pub struct ApiState {
    pub router: Arc<MessageRouter>,
}

/// Create HTTP API router
pub fn create_router(state: Arc<ApiState>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/message", post(handle_message))
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/:session_id", get(get_session))
        .route("/api/sessions/:session_id/interrupt", post(interrupt_session))
        .route("/api/sessions/:session_id/end", post(end_session))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "hermes-gateway",
    }))
}

/// Message request body
#[derive(Debug, Deserialize)]
struct MessageRequest {
    platform: String,
    chat_id: String,
    user_id: String,
    text: String,
    #[serde(default)]
    attachments: Vec<crate::router::Attachment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply_to_message_id: Option<String>,
}

/// Message response
#[derive(Debug, Serialize)]
struct MessageResponse {
    status: String,
    session_id: String,
}

/// Handle incoming message
async fn handle_message(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<MessageRequest>,
) -> Result<Json<MessageResponse>, ApiError> {
    debug!(
        "Received message from platform={}, chat_id={}",
        req.platform, req.chat_id
    );

    let msg = InboundMessage {
        platform: req.platform.clone(),
        chat_id: req.chat_id.clone(),
        user_id: req.user_id,
        text: req.text,
        attachments: req.attachments,
        reply_to_message_id: req.reply_to_message_id,
    };

    state.router.route_inbound(msg).await?;

    let session_id = format!("{}_{}", req.platform, req.chat_id);

    Ok(Json(MessageResponse {
        status: "processing".to_string(),
        session_id,
    }))
}

/// List all sessions
async fn list_sessions(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let sessions = state.router.session_manager().list_sessions().await;

    Ok(Json(serde_json::json!({
        "sessions": sessions,
        "count": sessions.len(),
    })))
}

/// Get session details
async fn get_session(
    State(state): State<Arc<ApiState>>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = state
        .router
        .session_manager()
        .get_session(&session_id)
        .await
        .ok_or(ApiError::NotFound)?;

    Ok(Json(serde_json::json!(session)))
}

/// Interrupt a session
async fn interrupt_session(
    State(state): State<Arc<ApiState>>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.router.interrupt_session(&session_id).await?;

    Ok(Json(serde_json::json!({
        "status": "interrupted",
        "session_id": session_id,
    })))
}

/// End a session
async fn end_session(
    State(state): State<Arc<ApiState>>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.router.end_session(&session_id).await?;

    Ok(Json(serde_json::json!({
        "status": "ended",
        "session_id": session_id,
    })))
}

/// API error type
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Router error: {0}")]
    RouterError(#[from] crate::router::RouterError),

    #[error("Not found")]
    NotFound,

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::RouterError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "Not found".to_string()),
            ApiError::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, e),
        };

        let body = Json(serde_json::json!({
            "error": message,
        }));

        (status, body).into_response()
    }
}
