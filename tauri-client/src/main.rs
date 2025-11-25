//! Unhidra Desktop Client
//!
//! A secure chat desktop application built with Tauri

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app.get_window("main").unwrap();
                window.open_devtools();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            connect_to_server,
            send_message,
            get_channels,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}

/// Connect to Unhidra server
#[tauri::command]
async fn connect_to_server(server_url: String, token: String) -> Result<String, String> {
    // TODO: Implement WebSocket connection to gateway
    Ok(format!("Connected to {}", server_url))
}

/// Send a message to a channel
#[tauri::command]
async fn send_message(channel_id: String, content: String) -> Result<(), String> {
    // TODO: Implement message sending with E2EE
    Ok(())
}

/// Get list of channels
#[tauri::command]
async fn get_channels() -> Result<Vec<String>, String> {
    // TODO: Fetch channels from API
    Ok(vec![])
}
