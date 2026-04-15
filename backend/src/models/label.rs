use serde::{Deserialize, Deserializer, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Deserializer for an optional field where the outer `Option` means "field was
/// present in JSON" and the inner `Option` is the value (which may be `null`).
/// Absent key  → `None`
/// `"icon": null` → `Some(None)`
/// `"icon": "star"` → `Some(Some("star".to_string()))`
fn deserialize_optional_field<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Label {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub color_bg: String,
    pub color_text: String,
    pub color_border: String,
    pub icon: Option<String>,
    pub is_default: bool,
}

/// Public response shape for a label (omits user_id).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LabelResponse {
    pub id: Uuid,
    pub name: String,
    pub color_bg: String,
    pub color_text: String,
    pub color_border: String,
    pub icon: Option<String>,
    pub is_default: bool,
}

impl From<Label> for LabelResponse {
    fn from(l: Label) -> Self {
        Self {
            id: l.id,
            name: l.name,
            color_bg: l.color_bg,
            color_text: l.color_text,
            color_border: l.color_border,
            icon: l.icon,
            is_default: l.is_default,
        }
    }
}

/// Request body for `POST /api/labels`.
#[derive(Debug, Deserialize)]
pub struct CreateLabelRequest {
    pub name: String,
    pub color_bg: String,
    pub color_text: String,
    pub color_border: String,
    pub icon: Option<String>,
}

/// Request body for `PUT /api/labels/:id`.
///
/// `icon` uses double-Option semantics:
/// - absent key → `None` (do not touch the DB column)
/// - `"icon": null` → `Some(None)` (clear the column to NULL)
/// - `"icon": "star"` → `Some(Some("star"))` (set a value)
#[derive(Debug, Deserialize)]
pub struct UpdateLabelRequest {
    pub name: Option<String>,
    pub color_bg: Option<String>,
    pub color_text: Option<String>,
    pub color_border: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub icon: Option<Option<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_label() -> Label {
        Label {
            id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            name: "Work".to_string(),
            color_bg: "#1e3a5f".to_string(),
            color_text: "#93c5fd".to_string(),
            color_border: "#3b82f6".to_string(),
            icon: Some("briefcase".to_string()),
            is_default: false,
        }
    }

    #[test]
    fn label_response_from_label_omits_user_id() {
        let label = make_label();
        let response = LabelResponse::from(label.clone());
        assert_eq!(response.id, label.id);
        assert_eq!(response.name, label.name);
        assert_eq!(response.color_bg, label.color_bg);
        assert_eq!(response.color_text, label.color_text);
        assert_eq!(response.color_border, label.color_border);
        assert_eq!(response.icon, label.icon);
        assert_eq!(response.is_default, label.is_default);
    }

    #[test]
    fn default_label_serializes_is_default_true() {
        let mut label = make_label();
        label.is_default = true;
        let response = LabelResponse::from(label);
        assert!(response.is_default);
    }
}
