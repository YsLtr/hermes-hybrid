//! Platform adapters
//!
//! Integrations with messaging platforms (QQBot, Telegram, etc.)

pub mod qqbot;

pub use qqbot::QQBotAdapter;
