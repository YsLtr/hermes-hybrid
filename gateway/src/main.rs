//! Hermes Hybrid Gateway
//!
//! Rust-based high-performance gateway that communicates with Python agent
//! via JSON-RPC 2.0 over stdin/stdout.

use hermes_gateway::{
    create_http_router, AgentBridge, ApiState, BridgeConfig, InboundMessage, OutboundMessage,
    QQBotAdapter, Router,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    info!("🚀 Hermes Hybrid Gateway starting...");

    // Configure agent bridge
    let mut env_vars = HashMap::new();
    env_vars.insert("PYTHONUNBUFFERED".to_string(), "1".to_string());

    let config = BridgeConfig {
        python_path: std::env::var("PYTHON_PATH").unwrap_or_else(|_| "python3".to_string()),
        agent_module: "hermes_cli.agent_bridge".to_string(),
        working_dir: std::env::var("AGENT_DIR").ok(),
        env_vars,
        heartbeat_interval_secs: 30,
        request_timeout_secs: 300,
        auto_restart: true,
        max_restart_attempts: 3,
    };

    info!(
        "Agent bridge config: python={}, module={}",
        config.python_path, config.agent_module
    );

    // Create and start agent bridge
    let bridge = Arc::new(AgentBridge::new(config));

    if let Err(e) = bridge.start().await {
        error!("Failed to start agent bridge: {}", e);
        std::process::exit(1);
    }

    info!("✅ Agent bridge started successfully");

    // Test ping
    match bridge.ping().await {
        Ok(resp) => {
            info!(
                "Agent ping successful: status={}, sessions={}",
                resp.status, resp.sessions
            );
        }
        Err(e) => {
            error!("Agent ping failed: {}", e);
        }
    }

    // Create message channels
    let (inbound_tx, mut inbound_rx) = mpsc::channel::<InboundMessage>(100);
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<OutboundMessage>(100);

    // Create router
    let router = Arc::new(Router::new(bridge.clone(), outbound_tx.clone()));

    // Start HTTP API server
    let api_state = Arc::new(ApiState {
        router: router.clone(),
    });

    let http_router = create_http_router(api_state);
    let http_addr = std::env::var("HTTP_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse::<SocketAddr>()
        .expect("Invalid HTTP_ADDR");

    let http_server = axum::serve(
        tokio::net::TcpListener::bind(&http_addr)
            .await
            .expect("Failed to bind HTTP server"),
        http_router,
    );

    info!("🌐 HTTP API server listening on {}", http_addr);

    // Spawn HTTP server
    tokio::spawn(async move {
        if let Err(e) = http_server.await {
            error!("HTTP server error: {}", e);
        }
    });

    // Start platform adapters
    let qq_enabled = std::env::var("QQ_ENABLED")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

    if qq_enabled {
        let app_id = std::env::var("QQ_APP_ID").expect("QQ_APP_ID not set");
        let client_secret = std::env::var("QQ_CLIENT_SECRET").expect("QQ_CLIENT_SECRET not set");

        let qq_config = hermes_gateway::platforms::qqbot::QQBotConfig {
            app_id,
            client_secret,
            api_base: std::env::var("QQ_API_BASE").ok(),
            sandbox: std::env::var("QQ_SANDBOX").ok().and_then(|s| s.parse().ok()),
            markdown_support: std::env::var("QQ_MARKDOWN").ok().and_then(|s| s.parse().ok()).unwrap_or(false),
            c2c_streaming: std::env::var("QQ_C2C_STREAMING").ok().and_then(|s| s.parse().ok()).unwrap_or(true),
            progress_coalesce: std::env::var("QQ_PROGRESS").ok().and_then(|s| s.parse().ok()).unwrap_or(true),
            metadata_footer: std::env::var("QQ_METADATA_FOOTER").ok().and_then(|s| s.parse().ok()).unwrap_or(true),
            notify_on_stream_end: std::env::var("QQ_NOTIFY_END").ok().and_then(|s| s.parse().ok()).unwrap_or(true),
            max_progress_messages: std::env::var("QQ_MAX_PROGRESS").ok().and_then(|s| s.parse().ok()).unwrap_or(2),
        };

        let qq_adapter = Arc::new(QQBotAdapter::new(qq_config, inbound_tx.clone()));
        let qq_adapter_clone = qq_adapter.clone();
        let _outbound_tx_clone = outbound_tx.clone();

        // Start QQBot adapter
        tokio::spawn(async move {
            if let Err(e) = qq_adapter_clone.start().await {
                error!("QQBot adapter error: {}", e);
            }
        });

        // QQBot outbound message handler
        let qq_adapter_for_outbound = qq_adapter.clone();
        tokio::spawn(async move {
            while let Some(msg) = outbound_rx.recv().await {
                if msg.platform == "qqbot" {
                    if let Err(e) = qq_adapter_for_outbound.send_message(msg).await {
                        error!("Failed to send QQBot message: {}", e);
                    }
                }
            }
        });

        info!("✅ QQBot adapter started");
    } else {
        info!("⚠️  QQBot adapter disabled (set QQ_ENABLED=true to enable)");
    }

    // Inbound message handler
    tokio::spawn(async move {
        while let Some(msg) = inbound_rx.recv().await {
            info!(
                "Processing inbound message from platform={}, chat_id={}",
                msg.platform, msg.chat_id
            );

            if let Err(e) = router.route_inbound(msg).await {
                error!("Failed to route message: {}", e);
            }
        }
    });

    info!("✅ Gateway running. Press Ctrl+C to stop.");

    // Wait for shutdown signal
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl+C");

    info!("🛑 Shutting down...");

    if let Err(e) = bridge.stop().await {
        error!("Error stopping agent bridge: {}", e);
    }

    info!("✅ Gateway stopped.");
}
