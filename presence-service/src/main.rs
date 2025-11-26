use axum::{
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use redis::{aio::Connection, AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use std::env;
use tracing::{error, info};

#[derive(Clone)]
struct AppState {
    redis: Client,
}

#[derive(Deserialize)]
struct PresenceUpdate {
    user: String,
}

#[derive(Serialize)]
struct OnlineResponse {
    users: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let redis_url = env::var("REDIS_URL")?;
    let redis = Client::open(redis_url)?;

    let state = AppState { redis };

    let app = Router::new()
        .route("/presence", post(update_presence))
        .route("/online", get(get_online))
        .with_state(state);

    info!("Presence service running on 0.0.0.0:3002");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3002").await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn update_presence(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(body): Json<PresenceUpdate>,
) -> Result<&'static str, StatusCode> {
    let mut conn = get_conn(&state).await?;
    let key = format!("presence:{}", body.user);
    conn.set_ex(key, "online", 30_u64)
        .await
        .map_err(|err| {
            error!("failed to update presence in redis: {err}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok("ok")
}

async fn get_online(
    axum::extract::State(state): axum::extract::State<AppState>
) -> Result<Json<OnlineResponse>, StatusCode> {
    let mut conn = get_conn(&state).await?;

    let mut iter: redis::AsyncIter<String> = conn
        .scan_match("presence:*")
        .await
        .map_err(|err| {
            error!("failed to scan presence keys: {err}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut keys: Vec<String> = Vec::new();
    while let Some(key) = iter.next_item().await {
        keys.push(key);
    }

    let users = keys
        .into_iter()
        .filter_map(|key| key.strip_prefix("presence:").map(String::from))
        .collect();

    Ok(Json(OnlineResponse { users }))
}

async fn get_conn(state: &AppState) -> Result<Connection, StatusCode> {
    state
        .redis
        .get_async_connection()
        .await
        .map_err(|err| {
            error!("failed to connect to redis: {err}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}
