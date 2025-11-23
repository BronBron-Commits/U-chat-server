//! Unhidra Core Library
//!
//! Provides core utilities used across all Unhidra services:
//! - Audit logging for compliance
//! - Error types
//! - Common utilities

pub mod audit;

// Re-export commonly used items
pub use audit::{
    ActionResult, ActorType, AuditAction, AuditEvent, AuditFilter, AuditLogger,
    MemoryAuditLogger, audit_logger, init_audit_logger, log, log_auth, log_message,
};

/// Initialize tracing with standard configuration
pub fn init_tracing(service_name: &str) {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("{}=info,tower_http=debug", service_name)));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();
}

/// Initialize tracing with JSON output (for production)
pub fn init_tracing_json(service_name: &str) {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("{}=info,tower_http=info", service_name)));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exports() {
        let event = AuditEvent::new("test", AuditAction::Login);
        assert_eq!(event.actor_id, "test");
    }
}
