use chrono::{DateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::label::LabelResponse;

#[derive(Debug, Clone, FromRow)]
pub struct Block {
    pub id: Uuid,
    pub routine_id: Uuid,
    pub day_of_week: i16,
    pub start_time: NaiveTime,
    pub end_time: Option<NaiveTime>,
    pub title: String,
    #[sqlx(rename = "type")]
    pub block_type: String,
    pub note: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Full response shape for a block, including labels and subtasks.
#[derive(Debug, Clone, Serialize)]
pub struct BlockResponse {
    pub id: Uuid,
    pub routine_id: Uuid,
    pub day_of_week: i16,
    pub start_time: String,
    pub end_time: Option<String>,
    pub title: String,
    #[serde(rename = "type")]
    pub block_type: String,
    pub note: Option<String>,
    pub sort_order: i32,
    pub labels: Vec<LabelResponse>,
    pub subtasks: Vec<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl BlockResponse {
    pub fn from_block(block: Block, labels: Vec<LabelResponse>) -> Self {
        Self {
            id: block.id,
            routine_id: block.routine_id,
            day_of_week: block.day_of_week,
            start_time: block.start_time.format("%H:%M").to_string(),
            end_time: block.end_time.map(|t| t.format("%H:%M").to_string()),
            title: block.title,
            block_type: block.block_type,
            note: block.note,
            sort_order: block.sort_order,
            labels,
            subtasks: Vec::new(),
            created_at: block.created_at,
            updated_at: block.updated_at,
        }
    }
}

/// Request body for `POST /api/routines/:id/blocks`.
#[derive(Debug, Deserialize)]
pub struct CreateBlockRequest {
    pub day_of_week: i16,
    pub start_time: String,
    pub end_time: Option<String>,
    pub title: String,
    #[serde(rename = "type")]
    pub block_type: String,
    pub note: Option<String>,
    pub sort_order: Option<i32>,
}

/// Request body for `PUT /api/blocks/:id`.
#[derive(Debug, Deserialize)]
pub struct UpdateBlockRequest {
    pub day_of_week: Option<i16>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub block_type: Option<String>,
    pub note: Option<String>,
    pub sort_order: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    fn make_block() -> Block {
        Block {
            id: Uuid::now_v7(),
            routine_id: Uuid::now_v7(),
            day_of_week: 1,
            start_time: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end_time: Some(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
            title: "Morning Work".to_string(),
            block_type: "trabalho".to_string(),
            note: None,
            sort_order: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn block_response_formats_time_as_hhmm() {
        let block = make_block();
        let response = BlockResponse::from_block(block, vec![]);
        assert_eq!(response.start_time, "09:00");
        assert_eq!(response.end_time.as_deref(), Some("10:00"));
    }

    #[test]
    fn block_response_subtasks_empty_by_default() {
        let block = make_block();
        let response = BlockResponse::from_block(block, vec![]);
        assert!(response.subtasks.is_empty());
    }

    #[test]
    fn block_response_without_end_time() {
        let mut block = make_block();
        block.end_time = None;
        let response = BlockResponse::from_block(block, vec![]);
        assert!(response.end_time.is_none());
    }
}
