use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub jwt_expiration_hours: i64,
    pub refresh_token_expiration_days: i64,
    pub host: String,
    pub port: u16,
    pub cors_origin: String,
    pub llm_default_provider: String,
    pub llm_gemini_api_key: Option<String>,
    pub llm_gemini_model: String,
    pub llm_claude_api_key: Option<String>,
    pub llm_claude_model: String,
}

impl Config {
    pub fn from_env() -> Result<Self, env::VarError> {
        Ok(Self {
            database_url: env::var("DATABASE_URL")?,
            jwt_secret: env::var("JWT_SECRET")?,
            jwt_expiration_hours: env::var("JWT_EXPIRATION_HOURS")
                .unwrap_or_else(|_| "24".to_string())
                .parse()
                .unwrap_or(24),
            refresh_token_expiration_days: env::var("REFRESH_TOKEN_EXPIRATION_DAYS")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .unwrap_or(3000),
            cors_origin: env::var("CORS_ORIGIN")
                .unwrap_or_else(|_| "http://localhost:5173".to_string()),
            llm_default_provider: env::var("LLM_DEFAULT_PROVIDER")
                .unwrap_or_else(|_| "gemini".to_string()),
            llm_gemini_api_key: env::var("LLM_GEMINI_API_KEY").ok(),
            llm_gemini_model: env::var("LLM_GEMINI_MODEL")
                .unwrap_or_else(|_| "gemini-2.5-flash-preview-05-20".to_string()),
            llm_claude_api_key: env::var("LLM_CLAUDE_API_KEY").ok(),
            llm_claude_model: env::var("LLM_CLAUDE_MODEL")
                .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string()),
        })
    }

    pub fn server_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_addr() {
        let config = Config {
            database_url: String::new(),
            jwt_secret: String::new(),
            jwt_expiration_hours: 24,
            refresh_token_expiration_days: 30,
            host: "127.0.0.1".to_string(),
            port: 8080,
            cors_origin: String::new(),
            llm_default_provider: "gemini".to_string(),
            llm_gemini_api_key: None,
            llm_gemini_model: String::new(),
            llm_claude_api_key: None,
            llm_claude_model: String::new(),
        };
        assert_eq!(config.server_addr(), "127.0.0.1:8080");
    }
}
