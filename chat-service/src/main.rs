//! Chat Service - Real-time messaging with group chat support
//!
//! Features:
//! - Group chat creation and management
//! - Real-time message broadcasting
//! - Message persistence with SQLite
//! - User membership management

mod auth;
mod db;
mod handlers;
mod models;
mod state;

use axum::{
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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

    // Initialize JWT token service
    auth::init_token_service();

    // Get database URL from env
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://unhidra:password@localhost:5432/unhidra".to_string());

    // Create database connection pool (PostgreSQL)
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    tracing::info!("Connected to PostgreSQL database");

    // Build API router with authentication-required endpoints
    let api_router = Router::new()
        // Channel routes
        .route("/channels", post(handlers::channels::create_channel))
        .route("/channels", get(handlers::channels::list_channels))
        .route("/channels/:channel_id", get(handlers::channels::get_channel))
        .route("/channels/:channel_id/members", post(handlers::channels::add_member))
        .route("/channels/:channel_id/read", post(handlers::channels::mark_as_read))
        // Thread routes
        .route("/threads", post(handlers::threads::create_thread))
        .route("/threads/:thread_id", get(handlers::threads::get_thread))
        .route("/threads/:thread_id/replies", get(handlers::threads::list_thread_replies))
        .route("/threads/:thread_id/participants", post(handlers::threads::add_thread_participant))
        .route("/threads/:thread_id/read", post(handlers::threads::mark_thread_read))
        // File routes
        .route("/files", post(handlers::files::upload_file))
        .route("/files/:file_id", get(handlers::files::download_file))
        .route("/files/:file_id", delete(handlers::files::delete_file))
        .route("/channels/:channel_id/files", get(handlers::files::list_channel_files))
        .with_state(pool.clone());

    // Main app with health check (no auth required)
    let app = Router::new()
        .route("/health", get(health_check))
        .nest("/api", api_router)
        // Add middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

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

/// Health check endpoint
async fn health_check() -> (StatusCode, Json<Value>) {
    (
        StatusCode::OK,
        Json(json!({
            "status": "healthy",
            "service": "chat-service",
            "version": env!("CARGO_PKG_VERSION")
        })),
    )
}
