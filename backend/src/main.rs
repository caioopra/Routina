use std::collections::HashMap;
use std::sync::Arc;

use planner_backend::ai::provider::LlmProvider;
use planner_backend::{ai::claude::ClaudeProvider, ai::gemini::GeminiProvider, config, db, routes};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file if present
    dotenvy::dotenv().ok();

    // Initialize tracing — use JSON format when LOG_FORMAT=json.
    let env_filter = EnvFilter::from_default_env();
    match std::env::var("LOG_FORMAT").as_deref() {
        Ok("json") => {
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(env_filter)
                .init();
        }
        _ => {
            tracing_subscriber::fmt().with_env_filter(env_filter).init();
        }
    }
    tracing::info!(
        log_format = std::env::var("LOG_FORMAT").as_deref().unwrap_or("text"),
        "structured logging initialized"
    );

    // Load config
    let config = config::Config::from_env().expect("Missing required environment variables");
    let addr = config.server_addr();

    // Connect to database
    tracing::info!("Connecting to database...");
    let pool = db::create_pool(&config.database_url).await?;

    // Run migrations
    tracing::info!("Running migrations...");
    db::run_migrations(&pool).await?;

    // Build LLM providers map — include whichever keys are configured.
    let mut providers: HashMap<String, Arc<dyn LlmProvider>> = HashMap::new();

    match &config.llm_gemini_api_key {
        Some(key) => {
            tracing::info!(
                "GeminiProvider initialised with model {}",
                config.llm_gemini_model
            );
            let p = Arc::new(GeminiProvider::new(
                key.clone(),
                config.llm_gemini_model.clone(),
            )) as Arc<dyn LlmProvider>;
            providers.insert("gemini".to_string(), p);
        }
        None => {
            tracing::warn!("LLM_GEMINI_API_KEY not set — Gemini provider unavailable");
        }
    }

    match &config.llm_claude_api_key {
        Some(key) => {
            tracing::info!(
                "ClaudeProvider initialised with model {}",
                config.llm_claude_model
            );
            let p = Arc::new(ClaudeProvider::new(
                key.clone(),
                config.llm_claude_model.clone(),
            )) as Arc<dyn LlmProvider>;
            providers.insert("claude".to_string(), p);
        }
        None => {
            tracing::warn!("LLM_CLAUDE_API_KEY not set — Claude provider unavailable");
        }
    }

    if providers.is_empty() {
        tracing::warn!(
            "No LLM providers configured — chat endpoint will return 503 until at least one key is provided"
        );
    }

    // Build CORS layer
    let cors = CorsLayer::new()
        .allow_origin(
            config
                .cors_origin
                .parse::<axum::http::HeaderValue>()
                .unwrap(),
        )
        .allow_methods(Any)
        .allow_headers(Any);

    // Build router
    let app = routes::create_router_with_providers(pool, config, providers)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    // Start server
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Server listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
