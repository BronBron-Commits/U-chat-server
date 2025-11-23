//! Worker modules for ML inference processing
//!
//! This module contains the `PythonWorker` implementation for IPC-based
//! ML inference using a Python sidecar process.

mod ml_bridge;

pub use ml_bridge::{InputPayload, OutputPayload, PythonWorker, PythonWorkerError};
