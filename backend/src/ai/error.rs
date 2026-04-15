use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error ({status}): {body}")]
    Api { status: u16, body: String },

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Missing API key for provider: {0}")]
    MissingApiKey(&'static str),

    #[error("Stream error: {0}")]
    Stream(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_api_key_message() {
        let err = ProviderError::MissingApiKey("gemini");
        assert!(err.to_string().contains("gemini"));
    }

    #[test]
    fn api_error_includes_status_and_body() {
        let err = ProviderError::Api {
            status: 429,
            body: "quota exceeded".to_string(),
        };
        assert!(err.to_string().contains("429"));
        assert!(err.to_string().contains("quota exceeded"));
    }
}
