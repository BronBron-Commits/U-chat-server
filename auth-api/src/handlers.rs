use axum::{Json, extract::State};
use serde::Deserialize;
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};
use serde_json::json;

use crate::services::PasswordService;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub password_service: PasswordService,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

pub async fn login_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> Json<serde_json::Value> {
    println!("AUTH-API: Received login request for {}", payload.username);

    let username = payload.username.clone();
    let conn = state.db.lock().unwrap();

    // Query user with Argon2id password_hash (PHC format includes salt)
    let mut stmt = match conn.prepare(
        "SELECT password_hash, verified, display_name FROM users WHERE username = ?1"
    ) {
        Ok(s) => s,
        Err(e) => {
            println!("AUTH-API: Failed to prepare SQL: {}", e);
            return Json(json!({ "error": "db_error" }));
        }
    };

    let row = stmt.query_row(params![username.clone()], |r| {
        Ok((
            r.get::<_, String>(0)?,  // password_hash (PHC format with embedded salt)
            r.get::<_, i64>(1)?,     // verified
            r.get::<_, String>(2)?,  // display_name
        ))
    });

    let (stored_hash, verified, display_name) = match row {
        Ok(t) => t,
        Err(_) => {
            println!("AUTH-API: User not found");
            return Json(json!({ "error": "User not found" }));
        }
    };

    if verified == 0 {
        println!("AUTH-API: User not verified");
        return Json(json!({ "error": "Not verified" }));
    }

    // Verify password using Argon2id (constant-time comparison)
    match state.password_service.verify_password(&payload.password, &stored_hash) {
        Ok(true) => {
            // Password matches
        }
        Ok(false) => {
            println!("AUTH-API: Invalid password");
            return Json(json!({ "error": "Invalid password" }));
        }
        Err(e) => {
            println!("AUTH-API: Password verification error: {}", e);
            return Json(json!({ "error": "Invalid password" }));
        }
    }

    println!("AUTH-API: Login OK for {} ({})", username, display_name);

    Json(json!({
        "ok": true,
        "user": username,
        "display_name": display_name,
        "token": username
    }))
}
