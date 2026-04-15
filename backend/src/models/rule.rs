use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Rule {
    pub id: Uuid,
    pub routine_id: Uuid,
    pub text: String,
    pub sort_order: i32,
}

/// Request body for `POST /api/routines/:id/rules`.
#[derive(Debug, Deserialize)]
pub struct CreateRuleRequest {
    pub text: String,
    pub sort_order: Option<i32>,
}

/// Request body for `PUT /api/rules/:id`.
#[derive(Debug, Deserialize)]
pub struct UpdateRuleRequest {
    pub text: Option<String>,
    pub sort_order: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_serializes_all_fields() {
        let rule = Rule {
            id: Uuid::now_v7(),
            routine_id: Uuid::now_v7(),
            text: "No meetings before 10am".to_string(),
            sort_order: 0,
        };
        let json = serde_json::to_value(&rule).unwrap();
        assert!(json["id"].is_string());
        assert!(json["routine_id"].is_string());
        assert_eq!(json["text"], "No meetings before 10am");
        assert_eq!(json["sort_order"], 0);
    }
}
