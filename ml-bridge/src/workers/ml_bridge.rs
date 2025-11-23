//! Python Worker Bridge for ML Inference
//!
//! This module provides the `PythonWorker` struct that manages a Python subprocess
//! running ML inference, communicating via Unix Domain Sockets (UDS) with
//! length-prefixed JSON messages.
//!
//! # Protocol
//!
//! The IPC protocol uses length-prefixed JSON messages:
//! - Request: 4-byte big-endian length + JSON payload
//! - Response: 4-byte big-endian length + JSON payload
//!
//! # Safety
//!
//! This implementation:
//! - Uses pure async I/O (no blocking)
//! - Ensures child process cleanup on drop
//! - Provides timeout support for resilience

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tracing::{debug, error, info, warn};

/// Default socket path for ML inference
pub const DEFAULT_SOCKET_PATH: &str = "/tmp/unhidra_ml_infer.sock";

/// Default path to the Python inference worker script
pub const DEFAULT_WORKER_SCRIPT: &str = "scripts/inference_worker.py";

/// Maximum time to wait for the Python worker to start (in milliseconds)
const WORKER_STARTUP_TIMEOUT_MS: u64 = 10_000;

/// Interval between connection retry attempts (in milliseconds)
const CONNECTION_RETRY_INTERVAL_MS: u64 = 50;

/// Errors that can occur during ML bridge operations
#[derive(Error, Debug)]
pub enum PythonWorkerError {
    #[error("Failed to spawn Python process: {0}")]
    SpawnError(#[from] std::io::Error),

    #[error("Python worker exited unexpectedly with status: {0}")]
    WorkerExited(String),

    #[error("Connection to worker socket failed after timeout")]
    ConnectionTimeout,

    #[error("Failed to serialize request: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("IPC communication error: {0}")]
    IpcError(String),

    #[error("Inference timeout: worker did not respond within {0:?}")]
    InferenceTimeout(Duration),

    #[error("Worker health check failed: {0}")]
    HealthCheckFailed(String),
}

/// Input payload for ML inference requests
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InputPayload {
    /// Input data for inference (e.g., text, base64-encoded audio, etc.)
    pub data: String,

    /// Optional request identifier for correlation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,

    /// Optional model identifier (for multi-model deployments)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
}

impl InputPayload {
    /// Create a new input payload with just data
    pub fn new(data: impl Into<String>) -> Self {
        Self {
            data: data.into(),
            request_id: None,
            model_id: None,
        }
    }

    /// Create a new input payload with a request ID for correlation
    pub fn with_request_id(data: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self {
            data: data.into(),
            request_id: Some(request_id.into()),
            model_id: None,
        }
    }
}

/// Output payload from ML inference
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputPayload {
    /// Inference result
    pub result: String,

    /// Echo back the request ID if provided
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,

    /// Processing time in milliseconds (if reported by worker)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processing_time_ms: Option<u64>,

    /// Optional error message if inference failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Python worker that manages a subprocess for ML inference via IPC
pub struct PythonWorker {
    /// Handle to the spawned Python process
    process: Child,

    /// Unix domain socket connection to the worker
    socket: UnixStream,

    /// Path to the socket file (for cleanup/reconnection)
    socket_path: String,
}

impl PythonWorker {
    /// Start the Python worker process and connect to its Unix socket.
    ///
    /// This method:
    /// 1. Removes any leftover socket file from previous runs
    /// 2. Spawns the Python inference worker subprocess
    /// 3. Waits for the worker to create and listen on the socket
    /// 4. Establishes a connection to the socket
    ///
    /// # Arguments
    ///
    /// * `socket_path` - Path where the Unix domain socket will be created
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The Python process fails to spawn
    /// - The worker exits before creating the socket
    /// - Connection cannot be established within the timeout period
    pub async fn start(socket_path: &str) -> Result<PythonWorker, PythonWorkerError> {
        Self::start_with_script(socket_path, DEFAULT_WORKER_SCRIPT).await
    }

    /// Start the Python worker with a custom script path.
    ///
    /// # Arguments
    ///
    /// * `socket_path` - Path where the Unix domain socket will be created
    /// * `script_path` - Path to the Python inference worker script
    pub async fn start_with_script(
        socket_path: &str,
        script_path: &str,
    ) -> Result<PythonWorker, PythonWorkerError> {
        info!(socket_path = %socket_path, script_path = %script_path, "Starting Python ML worker");

        // Remove any leftover socket file from previous runs
        if Path::new(socket_path).exists() {
            debug!(socket_path = %socket_path, "Removing existing socket file");
            let _ = std::fs::remove_file(socket_path);
        }

        // Spawn the Python inference subprocess
        let child = Command::new("python3")
            .arg(script_path)
            .arg(socket_path)
            .kill_on_drop(true) // Ensure cleanup if Rust process exits
            .spawn()?;

        info!(pid = child.id(), "Python worker process spawned");

        // Attempt to connect to the Unix socket with retry logic
        let socket = Self::connect_with_retry(socket_path, child.id()).await?;

        info!(socket_path = %socket_path, "Connected to Python worker socket");

        Ok(PythonWorker {
            process: child,
            socket,
            socket_path: socket_path.to_string(),
        })
    }

    /// Attempt to connect to the worker socket with retries and timeout
    async fn connect_with_retry(
        socket_path: &str,
        child_pid: Option<u32>,
    ) -> Result<UnixStream, PythonWorkerError> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(WORKER_STARTUP_TIMEOUT_MS);
        let retry_interval = Duration::from_millis(CONNECTION_RETRY_INTERVAL_MS);

        loop {
            match UnixStream::connect(socket_path).await {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    // Check if we've exceeded the timeout
                    if start.elapsed() > timeout {
                        error!(
                            socket_path = %socket_path,
                            elapsed_ms = start.elapsed().as_millis(),
                            "Connection timeout waiting for Python worker"
                        );
                        return Err(PythonWorkerError::ConnectionTimeout);
                    }

                    // Log retry attempts at debug level
                    debug!(
                        socket_path = %socket_path,
                        error = %e,
                        pid = ?child_pid,
                        "Socket not ready, retrying..."
                    );

                    // Wait before retrying
                    tokio::time::sleep(retry_interval).await;
                }
            }
        }
    }

    /// Send an input to the Python worker and await the output over IPC.
    ///
    /// This method:
    /// 1. Serializes the input to JSON
    /// 2. Sends a length-prefixed message to the worker
    /// 3. Reads the length-prefixed response
    /// 4. Deserializes and returns the output
    ///
    /// # Arguments
    ///
    /// * `input` - The input payload for inference
    ///
    /// # Errors
    ///
    /// Returns an error if serialization, I/O, or deserialization fails.
    pub async fn infer(&mut self, input: &InputPayload) -> Result<OutputPayload, PythonWorkerError> {
        debug!(request_id = ?input.request_id, "Sending inference request");

        // Serialize the input struct to JSON
        let request_bytes = serde_json::to_vec(input)?;

        // Send length (4 bytes, big-endian) followed by the JSON payload
        let len = (request_bytes.len() as u32).to_be_bytes();
        self.socket
            .write_all(&len)
            .await
            .map_err(|e| PythonWorkerError::IpcError(format!("Failed to write length: {}", e)))?;
        self.socket
            .write_all(&request_bytes)
            .await
            .map_err(|e| PythonWorkerError::IpcError(format!("Failed to write payload: {}", e)))?;
        self.socket
            .flush()
            .await
            .map_err(|e| PythonWorkerError::IpcError(format!("Failed to flush: {}", e)))?;

        debug!(payload_len = request_bytes.len(), "Request sent, awaiting response");

        // Read the 4-byte length of the response
        let mut len_buf = [0u8; 4];
        self.socket
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| PythonWorkerError::IpcError(format!("Failed to read response length: {}", e)))?;
        let resp_len = u32::from_be_bytes(len_buf) as usize;

        // Read the JSON response of the indicated length
        let mut resp_buf = vec![0u8; resp_len];
        self.socket
            .read_exact(&mut resp_buf)
            .await
            .map_err(|e| PythonWorkerError::IpcError(format!("Failed to read response: {}", e)))?;

        debug!(response_len = resp_len, "Response received");

        // Deserialize the JSON into OutputPayload
        let output = serde_json::from_slice::<OutputPayload>(&resp_buf)?;

        Ok(output)
    }

    /// Send an inference request with a timeout.
    ///
    /// Wraps `infer()` with a timeout to prevent hung workers from blocking
    /// the calling task indefinitely.
    ///
    /// # Arguments
    ///
    /// * `input` - The input payload for inference
    /// * `timeout` - Maximum time to wait for response
    ///
    /// # Errors
    ///
    /// Returns `PythonWorkerError::InferenceTimeout` if the worker doesn't
    /// respond within the specified duration.
    pub async fn infer_with_timeout(
        &mut self,
        input: &InputPayload,
        timeout: Duration,
    ) -> Result<OutputPayload, PythonWorkerError> {
        match tokio::time::timeout(timeout, self.infer(input)).await {
            Ok(result) => result,
            Err(_) => {
                warn!(
                    timeout_ms = timeout.as_millis(),
                    request_id = ?input.request_id,
                    "Inference request timed out"
                );
                Err(PythonWorkerError::InferenceTimeout(timeout))
            }
        }
    }

    /// Perform a health check on the Python worker.
    ///
    /// Sends a simple "ping" request and verifies the worker responds correctly.
    /// Useful for monitoring and liveness probes.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Maximum time to wait for health check response
    pub async fn health_check(&mut self, timeout: Duration) -> Result<(), PythonWorkerError> {
        let input = InputPayload::with_request_id("health_check", "healthcheck-probe");

        match self.infer_with_timeout(&input, timeout).await {
            Ok(output) => {
                if output.result.contains("ok") {
                    debug!("Health check passed");
                    Ok(())
                } else {
                    Err(PythonWorkerError::HealthCheckFailed(format!(
                        "Unexpected response: {}",
                        output.result
                    )))
                }
            }
            Err(e) => Err(PythonWorkerError::HealthCheckFailed(e.to_string())),
        }
    }

    /// Check if the Python worker process is still running.
    pub fn is_alive(&mut self) -> bool {
        match self.process.try_wait() {
            Ok(None) => true,        // Process still running
            Ok(Some(_)) => false,    // Process has exited
            Err(_) => false,         // Error checking status
        }
    }

    /// Get the process ID of the Python worker (if still running).
    pub fn pid(&self) -> Option<u32> {
        self.process.id()
    }

    /// Get the socket path being used for IPC.
    pub fn socket_path(&self) -> &str {
        &self.socket_path
    }

    /// Gracefully shutdown the worker by killing the process.
    pub async fn shutdown(mut self) -> Result<(), PythonWorkerError> {
        info!(pid = self.process.id(), "Shutting down Python worker");

        // Attempt graceful shutdown first
        if let Err(e) = self.process.kill().await {
            warn!(error = %e, "Error during worker shutdown");
        }

        // Clean up the socket file
        if Path::new(&self.socket_path).exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }

        Ok(())
    }
}

impl Drop for PythonWorker {
    fn drop(&mut self) {
        // Ensure the Python process is terminated when the worker is dropped
        // Note: kill_on_drop(true) in Command should handle this, but this is a fallback
        if let Some(pid) = self.process.id() {
            debug!(pid = pid, "Dropping PythonWorker, ensuring process cleanup");
        }
        // The child process will be killed automatically due to kill_on_drop(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    /// Test that InputPayload serializes correctly
    #[test]
    fn test_input_payload_serialization() {
        let input = InputPayload::new("test data");
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("test data"));
        assert!(!json.contains("request_id")); // Should be skipped when None

        let input_with_id = InputPayload::with_request_id("test", "req-123");
        let json = serde_json::to_string(&input_with_id).unwrap();
        assert!(json.contains("req-123"));
    }

    /// Test that OutputPayload deserializes correctly
    #[test]
    fn test_output_payload_deserialization() {
        let json = r#"{"result": "test_ok", "request_id": "123"}"#;
        let output: OutputPayload = serde_json::from_str(json).unwrap();
        assert_eq!(output.result, "test_ok");
        assert_eq!(output.request_id, Some("123".to_string()));
    }

    /// Integration test that requires Python environment
    /// Run with: cargo test --package ml-bridge -- --ignored
    #[test]
    #[ignore = "Requires Python environment with inference_worker.py"]
    fn test_ipc_integration_basic() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let socket_path = "/tmp/unhidra_test_integration.sock";

            // Start the Python worker
            let mut worker = PythonWorker::start(socket_path)
                .await
                .expect("Failed to start worker");

            // Send a sample request and get a response
            let input = InputPayload::new("ping");
            let output = worker.infer(&input).await.expect("Inference call failed");

            assert_eq!(output.result, "ping_ok");

            // Verify worker is alive
            assert!(worker.is_alive());

            // Clean shutdown
            worker.shutdown().await.expect("Shutdown failed");
        });
    }

    /// Test timeout functionality (mocked, doesn't require Python)
    #[test]
    fn test_inference_timeout_error() {
        let err = PythonWorkerError::InferenceTimeout(Duration::from_secs(5));
        assert!(err.to_string().contains("5s"));
    }

    /// Test error variants
    #[test]
    fn test_error_messages() {
        let spawn_err = PythonWorkerError::SpawnError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "python3 not found",
        ));
        assert!(spawn_err.to_string().contains("spawn"));

        let worker_err = PythonWorkerError::WorkerExited("exit code 1".to_string());
        assert!(worker_err.to_string().contains("exit code 1"));

        let health_err = PythonWorkerError::HealthCheckFailed("timeout".to_string());
        assert!(health_err.to_string().contains("timeout"));
    }
}
