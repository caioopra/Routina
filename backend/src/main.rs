use std::sync::Arc;

use planner_backend::{ai::gemini::GeminiProvider, config, db, routes};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file if present
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Load config
    let config = config::Config::from_env().expect("Missing required environment variables");
    let addr = config.server_addr();

    // Connect to database
    tracing::info!("Connecting to database...");
    let pool = db::create_pool(&config.database_url).await?;

    // Run migrations
    tracing::info!("Running migrations...");
    db::run_migrations(&pool).await?;

    // Build LLM provider — optional: if GEMINI_API_KEY is absent the chat
    // endpoint will return 503 rather than crashing the process.
    let llm_provider = match &config.llm_gemini_api_key {
        Some(key) => {
            tracing::info!(
                "GeminiProvider initialised with model {}",
                config.llm_gemini_model
            );
            Some(Arc::new(GeminiProvider::new(
                key.clone(),
                config.llm_gemini_model.clone(),
            ))
                as Arc<dyn planner_backend::ai::provider::LlmProvider>)
        }
        None => {
            tracing::warn!(
                "GEMINI_API_KEY / LLM_GEMINI_API_KEY not set — \
                 chat endpoint will return 503 until a key is provided"
            );
            None
        }
    };

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
    let app = routes::create_router_with_provider(pool, config, llm_provider)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    // Start server
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Server listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
