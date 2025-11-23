//! Build script for Unhidra ESP32 firmware
//!
//! This script:
//! 1. Loads environment variables from .env file
//! 2. Passes configuration to the compiler
//! 3. Integrates with ESP-IDF build system

use std::env;
use std::path::PathBuf;

fn main() {
    // Tell cargo to re-run if these files change
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=sdkconfig.defaults");
    println!("cargo:rerun-if-changed=.env");

    // Load .env file if present (for local development)
    if let Ok(env_path) = find_env_file() {
        if let Ok(content) = std::fs::read_to_string(&env_path) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    let value = value.trim().trim_matches('"');
                    // Only set if not already set in environment
                    if env::var(key).is_err() {
                        println!("cargo:rustc-env={}={}", key, value);
                    }
                }
            }
        }
    }

    // Ensure required environment variables are set
    let required_vars = [
        "WIFI_SSID",
        "WIFI_PASSWORD",
        "DEVICE_API_KEY",
        "DEVICE_ID",
    ];

    let mut missing = Vec::new();
    for var in &required_vars {
        if env::var(var).is_err() {
            missing.push(*var);
        }
    }

    if !missing.is_empty() {
        eprintln!("========================================");
        eprintln!("ERROR: Missing required environment variables:");
        for var in &missing {
            eprintln!("  - {}", var);
        }
        eprintln!("");
        eprintln!("Create a .env file in the firmware directory with:");
        eprintln!("  WIFI_SSID=\"your-wifi-ssid\"");
        eprintln!("  WIFI_PASSWORD=\"your-wifi-password\"");
        eprintln!("  DEVICE_API_KEY=\"your-device-api-key\"");
        eprintln!("  DEVICE_ID=\"device-001\"");
        eprintln!("========================================");
        panic!("Missing required configuration");
    }

    // ESP-IDF integration
    embuild::espidf::sysenv::output();
}

/// Find .env file by searching up directory tree
fn find_env_file() -> Result<PathBuf, ()> {
    let mut dir = env::current_dir().map_err(|_| ())?;

    loop {
        let env_path = dir.join(".env");
        if env_path.exists() {
            return Ok(env_path);
        }

        if !dir.pop() {
            return Err(());
        }
    }
}
