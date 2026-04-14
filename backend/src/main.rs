use planner_backend::{config, db, routes};
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
    let app = routes::create_router(pool, config)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    // Start server
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Server listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
