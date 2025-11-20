use axum::{
    Router,
    routing::{post, get},
    Json,
};
use serde_json::json;
use tokio::net::TcpListener;

async fn login_handler() -> &'static str {
    "login endpoint"
}

async fn health_handler() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

#[tokio::main]
async fn main() {
    println!("Auth API running on 0.0.0.0:9200");

    let app = Router::new()
        .route("/login", post(login_handler))
        .route("/health", get(health_handler));

    // Axum 0.7 style listener + serve
    let listener = TcpListener::bind("0.0.0.0:9200").await.unwrap();
    axum::serve(listener, app)
        .await
        .unwrap();
}
