//! Common configuration utilities for Unhidra services

use serde::{Deserialize, Serialize};
use std::env;

/// Common service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Service name for logging and metrics
    pub service_name: String,

    /// Host to bind to
    pub host: String,

    /// Port to listen on
    pub port: u16,

    /// Log level (trace, debug, info, warn, error)
    pub log_level: String,

    /// JWT secret (should be shared across services)
    pub jwt_secret: String,

    /// Database URL (if applicable)
    pub database_url: Option<String>,

    /// Redis URL for caching/pub-sub (if applicable)
    pub redis_url: Option<String>,

    /// Enable metrics endpoint
    pub metrics_enabled: bool,

    /// Metrics endpoint port (usually different from main port)
    pub metrics_port: u16,
}

impl ServiceConfig {
    /// Load configuration from environment variables
    ///
    /// Environment variable mapping:
    /// - `SERVICE_NAME` or default
    /// - `HOST` or `0.0.0.0`
    /// - `PORT` or default
    /// - `LOG_LEVEL` or `info`
    /// - `JWT_SECRET` or `supersecret` (WARNING: must set in production)
    /// - `DATABASE_URL` (optional)
    /// - `REDIS_URL` (optional)
    /// - `METRICS_ENABLED` or `true`
    /// - `METRICS_PORT` or main port + 1000
    pub fn from_env(service_name: &str, default_port: u16) -> Self {
        let port = env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(default_port);

        Self {
            service_name: env::var("SERVICE_NAME").unwrap_or_else(|_| service_name.to_string()),
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port,
            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            jwt_secret: env::var("JWT_SECRET").unwrap_or_else(|_| {
                eprintln!("WARNING: JWT_SECRET not set, using default. DO NOT USE IN PRODUCTION!");
                "supersecret".to_string()
            }),
            database_url: env::var("DATABASE_URL").ok(),
            redis_url: env::var("REDIS_URL").ok(),
            metrics_enabled: env::var("METRICS_ENABLED")
                .map(|v| v.to_lowercase() == "true" || v == "1")
                .unwrap_or(true),
            metrics_port: env::var("METRICS_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(port + 1000),
        }
    }

    /// Get the socket address to bind to
    pub fn socket_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Get the metrics socket address
    pub fn metrics_addr(&self) -> String {
        format!("{}:{}", self.host, self.metrics_port)
    }
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            service_name: "unhidra-service".to_string(),
            host: "0.0.0.0".to_string(),
            port: 8080,
            log_level: "info".to_string(),
            jwt_secret: "supersecret".to_string(),
            database_url: None,
            redis_url: None,
            metrics_enabled: true,
            metrics_port: 9080,
        }
    }
}

/// Initialize tracing/logging for a service
pub fn init_logging(config: &ServiceConfig) {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();

    tracing::info!(
        service = %config.service_name,
        "Service starting on {}",
        config.socket_addr()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ServiceConfig::default();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert_eq!(config.log_level, "info");
        assert!(config.metrics_enabled);
    }

    #[test]
    fn test_socket_addr() {
        let config = ServiceConfig::default();
        assert_eq!(config.socket_addr(), "0.0.0.0:8080");
        assert_eq!(config.metrics_addr(), "0.0.0.0:9080");
    }
}
