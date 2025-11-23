mod handlers;
mod services;

use axum::{Router, routing::post, Json};
use axum::extract::State;
use serde::{Deserialize, Serialize};
use jsonwebtoken::{encode, EncodingKey, Header};
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    jwt_secret: String,
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
}

#[derive(Serialize)]
struct Claims {
    sub: String,
    exp: usize,
}

async fn login_handler(
    State(app): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> Json<LoginResponse> {
    if payload.username != "user" || payload.password != "password" {
        return Json(LoginResponse { token: "INVALID".to_string() });
    }

    let exp = chrono::Utc::now().timestamp() as usize + 3600;
    let claims = Claims { sub: payload.username.clone(), exp };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(app.jwt_secret.as_bytes()),
    ).unwrap();

    Json(LoginResponse { token })
}

#[tokio::main]
async fn main() {
    let secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "supersecret".into());

    let state = Arc::new(AppState {
        jwt_secret: secret,
    });

    let app = Router::new()
        .route("/login", post(login_handler))
        .with_state(state);

    println!("Auth API running on 0.0.0.0:9200");
    axum::serve(
        tokio::net::TcpListener::bind("0.0.0.0:9200").await.unwrap(),
        app
    )
    .await
    .unwrap();
}
