//! Common traits for Unhidra services

use async_trait::async_trait;
use crate::error::Result;

/// Repository trait for generic CRUD operations
#[async_trait]
pub trait Repository<T, Id> {
    /// Find an entity by its ID
    async fn find_by_id(&self, id: Id) -> Result<Option<T>>;

    /// Save an entity (insert or update)
    async fn save(&self, entity: &T) -> Result<T>;

    /// Delete an entity by its ID
    async fn delete(&self, id: Id) -> Result<bool>;

    /// Check if an entity exists by its ID
    async fn exists(&self, id: Id) -> Result<bool>;
}

/// Trait for entities that can be validated
pub trait Validatable {
    /// Validate the entity, returning errors if invalid
    fn validate(&self) -> Result<()>;
}

/// Trait for entities with timestamps
pub trait Timestamped {
    /// Get the creation timestamp
    fn created_at(&self) -> chrono::DateTime<chrono::Utc>;

    /// Get the last update timestamp, if any
    fn updated_at(&self) -> Option<chrono::DateTime<chrono::Utc>>;
}

/// Trait for entities that belong to a user
pub trait OwnedByUser {
    /// Get the ID of the owning user
    fn owner_id(&self) -> uuid::Uuid;

    /// Check if the entity is owned by the given user
    fn is_owned_by(&self, user_id: uuid::Uuid) -> bool {
        self.owner_id() == user_id
    }
}

/// Trait for services that need health checks
#[async_trait]
pub trait HealthCheck {
    /// Check if the service is healthy
    async fn health_check(&self) -> Result<HealthStatus>;
}

/// Health status of a service or component
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealthStatus {
    pub status: HealthState,
    pub message: Option<String>,
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HealthState {
    Healthy,
    Degraded,
    Unhealthy,
}

impl HealthStatus {
    pub fn healthy() -> Self {
        Self {
            status: HealthState::Healthy,
            message: None,
            details: None,
        }
    }

    pub fn unhealthy(message: impl Into<String>) -> Self {
        Self {
            status: HealthState::Unhealthy,
            message: Some(message.into()),
            details: None,
        }
    }

    pub fn degraded(message: impl Into<String>) -> Self {
        Self {
            status: HealthState::Degraded,
            message: Some(message.into()),
            details: None,
        }
    }
}
