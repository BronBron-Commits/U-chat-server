//! Common error types for Unhidra services

use thiserror::Error;

/// Common result type alias using UnhidraError
pub type Result<T> = std::result::Result<T, UnhidraError>;

/// Common error type used across all Unhidra services
#[derive(Debug, Error)]
pub enum UnhidraError {
    // Authentication & Authorization errors
    #[error("Authentication required")]
    Unauthorized,

    #[error("Access denied: {0}")]
    Forbidden(String),

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    // Resource errors
    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Resource already exists: {0}")]
    AlreadyExists(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    // Validation errors
    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Invalid input: {field} - {message}")]
    InvalidField { field: String, message: String },

    // Rate limiting
    #[error("Rate limit exceeded. Try again in {retry_after} seconds")]
    RateLimitExceeded { retry_after: u64 },

    // Database errors
    #[error("Database error: {0}")]
    Database(String),

    #[error("Connection pool exhausted")]
    PoolExhausted,

    // Network errors
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    // Service errors
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Internal error: {0}")]
    Internal(String),

    // Serialization errors
    #[error("Serialization error: {0}")]
    Serialization(String),

    // Configuration errors
    #[error("Configuration error: {0}")]
    Configuration(String),

    // Generic error wrapper
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl UnhidraError {
    /// Get the HTTP status code for this error
    pub fn status_code(&self) -> u16 {
        match self {
            Self::Unauthorized | Self::InvalidCredentials | Self::TokenExpired => 401,
            Self::Forbidden(_) => 403,
            Self::NotFound(_) => 404,
            Self::AlreadyExists(_) | Self::Conflict(_) => 409,
            Self::Validation(_) | Self::InvalidField { .. } => 400,
            Self::RateLimitExceeded { .. } => 429,
            Self::ServiceUnavailable(_) | Self::PoolExhausted => 503,
            Self::Timeout(_) => 504,
            _ => 500,
        }
    }

    /// Get an error code string for API responses
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::Forbidden(_) => "forbidden",
            Self::InvalidCredentials => "invalid_credentials",
            Self::TokenExpired => "token_expired",
            Self::InvalidToken(_) => "invalid_token",
            Self::NotFound(_) => "not_found",
            Self::AlreadyExists(_) => "already_exists",
            Self::Conflict(_) => "conflict",
            Self::Validation(_) => "validation_error",
            Self::InvalidField { .. } => "invalid_field",
            Self::RateLimitExceeded { .. } => "rate_limit_exceeded",
            Self::Database(_) => "database_error",
            Self::PoolExhausted => "pool_exhausted",
            Self::ConnectionFailed(_) => "connection_failed",
            Self::Timeout(_) => "timeout",
            Self::ServiceUnavailable(_) => "service_unavailable",
            Self::Internal(_) => "internal_error",
            Self::Serialization(_) => "serialization_error",
            Self::Configuration(_) => "configuration_error",
            Self::Other(_) => "unknown_error",
        }
    }

    /// Create a validation error for a specific field
    pub fn invalid_field(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InvalidField {
            field: field.into(),
            message: message.into(),
        }
    }
}

// Implement From for common error types
impl From<std::io::Error> for UnhidraError {
    fn from(err: std::io::Error) -> Self {
        Self::Internal(err.to_string())
    }
}

impl From<serde_json::Error> for UnhidraError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes() {
        assert_eq!(UnhidraError::Unauthorized.status_code(), 401);
        assert_eq!(UnhidraError::Forbidden("test".into()).status_code(), 403);
        assert_eq!(UnhidraError::NotFound("user".into()).status_code(), 404);
        assert_eq!(UnhidraError::Validation("bad input".into()).status_code(), 400);
        assert_eq!(
            UnhidraError::RateLimitExceeded { retry_after: 60 }.status_code(),
            429
        );
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(UnhidraError::Unauthorized.error_code(), "unauthorized");
        assert_eq!(UnhidraError::TokenExpired.error_code(), "token_expired");
    }

    #[test]
    fn test_invalid_field_helper() {
        let err = UnhidraError::invalid_field("email", "must be a valid email");
        match err {
            UnhidraError::InvalidField { field, message } => {
                assert_eq!(field, "email");
                assert_eq!(message, "must be a valid email");
            }
            _ => panic!("Expected InvalidField variant"),
        }
    }
}
