//! ML Bridge - IPC Sidecar for ML Inference
//!
//! This crate provides a `PythonWorker` that manages a Python subprocess
//! for ML inference, communicating via Unix Domain Sockets (UDS).
//!
//! # Architecture
//!
//! The ML inference is isolated in a separate Python process to:
//! - Prevent Tokio event-loop blocking from CPU-intensive ML tasks
//! - Bypass Python's GIL (Global Interpreter Lock) limitations
//! - Enable fault isolation (Python crash doesn't bring down Rust server)
//! - Allow independent scaling and resource management
//!
//! # Example
//!
//! ```ignore
//! use ml_bridge::workers::{PythonWorker, InputPayload};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let mut worker = PythonWorker::start("/tmp/ml_inference.sock").await?;
//!
//!     let input = InputPayload { data: "test input".to_string() };
//!     let output = worker.infer(&input).await?;
//!
//!     println!("Result: {}", output.result);
//!     Ok(())
//! }
//! ```

pub mod workers;

// Re-export main types for convenience
pub use workers::{InputPayload, OutputPayload, PythonWorker, PythonWorkerError};
