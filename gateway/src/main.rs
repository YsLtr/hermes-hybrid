//! Hermes Hybrid Gateway
//!
//! Rust-based high-performance gateway that communicates with Python agent
//! via JSON-RPC 2.0 over stdin/stdout.

use hermes_gateway::{AgentBridge, BridgeConfig};
use std::collections::HashMap;
use tracing::{info, error};

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

    info!("Agent bridge config: python={}, module={}", config.python_path, config.agent_module);

    // Create and start agent bridge
    let bridge = AgentBridge::new(config);

    if let Err(e) = bridge.start().await {
        error!("Failed to start agent bridge: {}", e);
        std::process::exit(1);
    }

    info!("✅ Agent bridge started successfully");

    // Test ping
    match bridge.ping().await {
        Ok(resp) => {
            info!("Agent ping successful: status={}, sessions={}", resp.status, resp.sessions);
        }
        Err(e) => {
            error!("Agent ping failed: {}", e);
        }
    }

    // TODO: Start platform adapters (QQBot, Telegram, etc.)
    // TODO: Implement message routing

    info!("Gateway running. Press Ctrl+C to stop.");

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");

    info!("🛑 Shutting down...");

    if let Err(e) = bridge.stop().await {
        error!("Error stopping agent bridge: {}", e);
    }

    info!("✅ Gateway stopped.");
}
