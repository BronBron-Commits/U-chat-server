//! Core shared types and utilities for Unhidra services
//!
//! This crate provides common models, traits, and utilities used across
//! all Unhidra microservices to ensure consistency and reduce duplication.

pub mod models;
pub mod error;
pub mod traits;
pub mod config;

// Re-export commonly used types
pub use models::*;
pub use error::{UnhidraError, Result};
pub use traits::*;
pub use config::ServiceConfig;
