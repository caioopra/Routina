use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// A persisted chat session bound to one routine.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Conversation {
    pub id: Uuid,
    pub user_id: Uuid,
    pub routine_id: Option<Uuid>,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Public shape returned by the API (omits `user_id`).
#[derive(Debug, Clone, Serialize)]
pub struct ConversationResponse {
    pub id: Uuid,
    pub routine_id: Option<Uuid>,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Conversation> for ConversationResponse {
    fn from(c: Conversation) -> Self {
        Self {
            id: c.id,
            routine_id: c.routine_id,
            title: c.title,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

/// Request body for `POST /api/conversations`.
#[derive(Debug, Deserialize)]
pub struct CreateConversationRequest {
    pub routine_id: Uuid,
    pub title: Option<String>,
}

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Message {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub role: String,
    pub content: Option<String>,
    pub tool_calls: Option<serde_json::Value>,
    pub tool_call_id: Option<String>,
    pub provider: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_conversation() -> Conversation {
        Conversation {
            id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            routine_id: Some(Uuid::now_v7()),
            title: Some("Test".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn conversation_response_omits_user_id() {
        let c = make_conversation();
        let resp = ConversationResponse::from(c);
        // Serialized JSON must not contain user_id.
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("user_id").is_none());
        assert!(json.get("id").is_some());
        assert!(json.get("routine_id").is_some());
    }

    #[test]
    fn conversation_response_preserves_fields() {
        let c = make_conversation();
        let rid = c.routine_id;
        let resp = ConversationResponse::from(c);
        assert_eq!(resp.routine_id, rid);
        assert_eq!(resp.title.as_deref(), Some("Test"));
    }
}
