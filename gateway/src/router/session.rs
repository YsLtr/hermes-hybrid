//! Session management

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Session configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub platform: String,
    pub chat_id: String,
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toolsets: Option<Vec<String>>,
}

/// Active session state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub session_id: String,
    pub config: SessionConfig,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
}

/// Session manager
pub struct SessionManager {
    sessions: DashMap<String, Session>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }

    pub async fn create_session(
        &self,
        session_id: &str,
        config: SessionConfig,
    ) -> Result<(), String> {
        let now = Utc::now();
        let session = Session {
            session_id: session_id.to_string(),
            config,
            created_at: now,
            last_activity: now,
        };

        self.sessions.insert(session_id.to_string(), session);
        Ok(())
    }

    pub async fn has_session(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }

    pub async fn get_session(&self, session_id: &str) -> Option<Session> {
        self.sessions.get(session_id).map(|s| s.clone())
    }

    pub async fn update_activity(&self, session_id: &str) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.last_activity = Utc::now();
        }
    }

    pub async fn remove_session(&self, session_id: &str) {
        self.sessions.remove(session_id);
    }

    pub async fn list_sessions(&self) -> Vec<Session> {
        self.sessions.iter().map(|s| s.value().clone()).collect()
    }

    pub async fn count_sessions(&self) -> usize {
        self.sessions.len()
    }
}
