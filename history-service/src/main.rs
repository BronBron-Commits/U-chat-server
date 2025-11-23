//! History Service - Message persistence and retrieval
//!
//! Provides REST API for storing and querying chat message history
//! with SQLite database backend.

mod db;
mod handlers;
mod models;

use axum::{
    routing::{get, post},
    Router,
};
use sqlx::sqlite::SqlitePoolOptions;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub db: db::Database,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "history_service=debug,tower_http=debug".into()),
        )
        .init();

    tracing::info!("History Service starting...");

    // Get database path from env or use default
    let db_path = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:history.db?mode=rwc".to_string());

    // Create database connection pool
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_path)
        .await?;

    // Run migrations
    db::run_migrations(&pool).await?;

    let database = db::Database::new(pool);
    let state = AppState { db: database };

    // Build router
    let app = Router::new()
        // Message endpoints
        .route("/messages", post(handlers::store_message))
        .route("/messages", get(handlers::get_messages))
        .route("/messages/{room_id}", get(handlers::get_room_messages))
        .route("/messages/user/{user_id}", get(handlers::get_user_messages))
        .route("/messages/search", get(handlers::search_messages))
        // Health endpoint
        .route("/health", get(handlers::health_check))
        // Add middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3002);

    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("History Service running on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
