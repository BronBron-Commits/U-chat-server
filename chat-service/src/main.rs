//! Chat Service - Real-time messaging with group chat support
//!
//! Features:
//! - Group chat creation and management
//! - Real-time message broadcasting
//! - Message persistence with SQLite
//! - User membership management

mod db;
mod handlers;
mod models;
mod state;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "chat_service=debug,tower_http=debug".into()),
        )
        .init();

    tracing::info!("Chat Service starting...");

    // Get database path from env or use default
    let db_path = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:chat.db?mode=rwc".to_string());

    // Create database connection pool
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_path)
        .await?;

    // Run migrations
    db::run_migrations(&pool).await?;

    let state = Arc::new(AppState::new(pool));

    // Build router
    let app = Router::new()
        // Group endpoints
        .route("/groups", post(handlers::create_group))
        .route("/groups", get(handlers::list_groups))
        .route("/groups/{group_id}", get(handlers::get_group))
        .route("/groups/{group_id}", put(handlers::update_group))
        .route("/groups/{group_id}", delete(handlers::delete_group))
        // Group membership
        .route("/groups/{group_id}/members", get(handlers::list_members))
        .route("/groups/{group_id}/members", post(handlers::add_member))
        .route(
            "/groups/{group_id}/members/{user_id}",
            delete(handlers::remove_member),
        )
        .route("/groups/{group_id}/join", post(handlers::join_group))
        .route("/groups/{group_id}/leave", post(handlers::leave_group))
        // Messages
        .route("/groups/{group_id}/messages", post(handlers::send_message))
        .route("/groups/{group_id}/messages", get(handlers::get_messages))
        // User's groups
        .route("/users/{user_id}/groups", get(handlers::get_user_groups))
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
        .unwrap_or(3001);

    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("Chat Service running on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
