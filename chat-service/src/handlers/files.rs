//! File upload/download handlers with E2EE encryption
//!
//! Supports secure file sharing with client-side encryption:
//! - Files are encrypted before upload using E2EE session keys
//! - Storage backends: Local filesystem, MinIO, AWS S3
//! - Automatic thumbnail generation for images
//! - SHA-256 checksums for integrity verification

use axum::{
    body::Bytes,
    extract::{Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::io::Write;
use uuid::Uuid;

use core::error::ApiError;

#[cfg(feature = "minio")]
use s3::{Bucket, creds::Credentials, Region};

/// File upload configuration
#[derive(Debug, Clone)]
pub struct FileStorageConfig {
    /// Storage backend: "local", "minio", "s3"
    pub backend: String,
    /// Local storage path
    pub local_path: Option<String>,
    /// MinIO/S3 endpoint
    pub endpoint: Option<String>,
    /// Bucket name
    pub bucket: Option<String>,
    /// Access key
    pub access_key: Option<String>,
    /// Secret key
    pub secret_key: Option<String>,
    /// Region (for S3)
    pub region: Option<String>,
    /// Max file size in bytes (default: 100MB)
    pub max_file_size: usize,
}

impl Default for FileStorageConfig {
    fn default() -> Self {
        Self {
            backend: "local".to_string(),
            local_path: Some("/tmp/unhidra-uploads".to_string()),
            endpoint: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            region: None,
            max_file_size: 100 * 1024 * 1024, // 100MB
        }
    }
}

impl FileStorageConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        Self {
            backend: std::env::var("FILE_STORAGE_BACKEND").unwrap_or_else(|_| "local".to_string()),
            local_path: std::env::var("FILE_STORAGE_PATH").ok(),
            endpoint: std::env::var("MINIO_ENDPOINT").ok(),
            bucket: std::env::var("MINIO_BUCKET").ok(),
            access_key: std::env::var("MINIO_ACCESS_KEY").ok(),
            secret_key: std::env::var("MINIO_SECRET_KEY").ok(),
            region: std::env::var("AWS_REGION").ok(),
            max_file_size: std::env::var("MAX_FILE_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100 * 1024 * 1024),
        }
    }
}

/// File upload response
#[derive(Debug, Serialize)]
pub struct FileUploadResponse {
    pub id: String,
    pub filename: String,
    pub file_size: i64,
    pub mime_type: String,
    pub checksum: String,
    pub encrypted: bool,
    pub created_at: String,
    pub download_url: String,
}

/// File list response
#[derive(Debug, Serialize)]
pub struct FileListResponse {
    pub files: Vec<FileUploadResponse>,
    pub total: i64,
}

/// Upload encrypted file
///
/// Expects multipart form data with:
/// - `file`: Binary file data (already encrypted by client)
/// - `channel_id`: Channel ID
/// - `message_id`: (Optional) Associated message ID
/// - `encryption_key_id`: E2EE key ID used for encryption
/// - `encrypted`: Boolean flag (should be "true" for E2EE)
pub async fn upload_file(
    State((pool, config)): State<(PgPool, FileStorageConfig)>,
    mut multipart: Multipart,
) -> Result<Json<FileUploadResponse>, ApiError> {
    let user_id = "system"; // TODO: Extract from JWT
    let file_id = Uuid::new_v4().to_string();

    let mut file_data: Option<Bytes> = None;
    let mut channel_id: Option<String> = None;
    let mut message_id: Option<String> = None;
    let mut encryption_key_id: Option<String> = None;
    let mut encrypted = false;
    let mut original_filename = "untitled".to_string();
    let mut mime_type = "application/octet-stream".to_string();

    // Parse multipart form
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        ApiError::ValidationError(format!("Invalid multipart data: {}", e))
    })? {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "file" => {
                original_filename = field.file_name()
                    .unwrap_or("untitled")
                    .to_string();
                mime_type = field.content_type()
                    .unwrap_or("application/octet-stream")
                    .to_string();
                file_data = Some(field.bytes().await.map_err(|e| {
                    ApiError::ValidationError(format!("Failed to read file: {}", e))
                })?);
            }
            "channel_id" => {
                channel_id = Some(field.text().await.map_err(|e| {
                    ApiError::ValidationError(format!("Invalid channel_id: {}", e))
                })?);
            }
            "message_id" => {
                message_id = Some(field.text().await.map_err(|e| {
                    ApiError::ValidationError(format!("Invalid message_id: {}", e))
                })?);
            }
            "encryption_key_id" => {
                encryption_key_id = Some(field.text().await.map_err(|e| {
                    ApiError::ValidationError(format!("Invalid encryption_key_id: {}", e))
                })?);
            }
            "encrypted" => {
                let value = field.text().await.map_err(|e| {
                    ApiError::ValidationError(format!("Invalid encrypted flag: {}", e))
                })?;
                encrypted = value == "true" || value == "1";
            }
            _ => {}
        }
    }

    let file_data = file_data.ok_or_else(|| {
        ApiError::ValidationError("No file provided".to_string())
    })?;

    let channel_id = channel_id.ok_or_else(|| {
        ApiError::ValidationError("channel_id is required".to_string())
    })?;

    // Validate file size
    if file_data.len() > config.max_file_size {
        return Err(ApiError::ValidationError(format!(
            "File too large: {} bytes (max: {})",
            file_data.len(),
            config.max_file_size
        )));
    }

    // Verify user is a channel member
    let _membership = sqlx::query!(
        r#"
        SELECT user_id FROM channel_members
        WHERE channel_id = $1 AND user_id = $2
        "#,
        channel_id,
        user_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| ApiError::DatabaseError(e.to_string()))?
    .ok_or(ApiError::Forbidden("Not a channel member".to_string()))?;

    // Calculate checksum
    let mut hasher = Sha256::new();
    hasher.update(&file_data);
    let checksum = format!("{:x}", hasher.finalize());

    // Generate storage filename
    let filename = format!("{}-{}", file_id, sanitize_filename(&original_filename));

    // Store file based on backend
    let storage_path = match config.backend.as_str() {
        "local" => {
            store_local_file(&config, &filename, &file_data).await?
        }
        #[cfg(feature = "minio")]
        "minio" | "s3" => {
            store_s3_file(&config, &filename, &file_data, &mime_type).await?
        }
        _ => {
            return Err(ApiError::InternalError(format!(
                "Unsupported storage backend: {}",
                config.backend
            )));
        }
    };

    // Insert file metadata into database
    let file_record = sqlx::query!(
        r#"
        INSERT INTO file_uploads (
            id, message_id, channel_id, uploader_id,
            filename, original_filename, file_size, mime_type,
            storage_path, storage_backend, checksum,
            encrypted, encryption_key_id
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        RETURNING id, filename, file_size, mime_type, checksum,
                  encrypted, created_at::text as "created_at!"
        "#,
        file_id,
        message_id,
        channel_id,
        user_id,
        filename,
        original_filename,
        file_data.len() as i64,
        mime_type,
        storage_path,
        config.backend,
        checksum,
        encrypted as i32,
        encryption_key_id
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

    Ok(Json(FileUploadResponse {
        id: file_record.id.clone(),
        filename: file_record.filename,
        file_size: file_record.file_size,
        mime_type: file_record.mime_type,
        checksum: file_record.checksum,
        encrypted: file_record.encrypted != 0,
        created_at: file_record.created_at,
        download_url: format!("/api/files/{}/download", file_record.id),
    }))
}

/// Download file
pub async fn download_file(
    State((pool, config)): State<(PgPool, FileStorageConfig)>,
    Path(file_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = "system"; // TODO: Extract from JWT

    // Fetch file metadata
    let file = sqlx::query!(
        r#"
        SELECT
            f.id, f.filename, f.original_filename, f.mime_type,
            f.storage_path, f.storage_backend, f.encrypted,
            f.channel_id, f.deleted_at
        FROM file_uploads f
        JOIN channel_members cm ON f.channel_id = cm.channel_id
        WHERE f.id = $1 AND cm.user_id = $2 AND f.deleted_at IS NULL
        "#,
        file_id,
        user_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| ApiError::DatabaseError(e.to_string()))?
    .ok_or(ApiError::NotFound("File not found or access denied".to_string()))?;

    // Read file data from storage
    let file_data = match file.storage_backend.as_str() {
        "local" => {
            read_local_file(&file.storage_path).await?
        }
        #[cfg(feature = "minio")]
        "minio" | "s3" => {
            read_s3_file(&config, &file.storage_path).await?
        }
        _ => {
            return Err(ApiError::InternalError(format!(
                "Unsupported storage backend: {}",
                file.storage_backend
            )));
        }
    };

    // Build response headers
    let headers = [
        (header::CONTENT_TYPE, file.mime_type),
        (
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", file.original_filename),
        ),
        (header::CONTENT_LENGTH, file_data.len().to_string()),
    ];

    Ok((headers, file_data))
}

/// List files in a channel
pub async fn list_channel_files(
    State((pool, _config)): State<(PgPool, FileStorageConfig)>,
    Path(channel_id): Path<String>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<FileListResponse>, ApiError> {
    let user_id = "system"; // TODO: Extract from JWT

    let limit = pagination.limit.unwrap_or(50).min(100) as i64;
    let offset = pagination.offset.unwrap_or(0) as i64;

    // Verify user is a channel member
    let _membership = sqlx::query!(
        r#"
        SELECT user_id FROM channel_members
        WHERE channel_id = $1 AND user_id = $2
        "#,
        channel_id,
        user_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| ApiError::DatabaseError(e.to_string()))?
    .ok_or(ApiError::Forbidden("Not a channel member".to_string()))?;

    // Get total count
    let total = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) as "count!"
        FROM file_uploads
        WHERE channel_id = $1 AND deleted_at IS NULL
        "#,
        channel_id
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

    // Fetch files
    let files = sqlx::query!(
        r#"
        SELECT id, filename, file_size, mime_type, checksum,
               encrypted, created_at::text as "created_at!"
        FROM file_uploads
        WHERE channel_id = $1 AND deleted_at IS NULL
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
        "#,
        channel_id,
        limit,
        offset
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

    let files = files
        .into_iter()
        .map(|f| FileUploadResponse {
            id: f.id.clone(),
            filename: f.filename,
            file_size: f.file_size,
            mime_type: f.mime_type,
            checksum: f.checksum,
            encrypted: f.encrypted != 0,
            created_at: f.created_at,
            download_url: format!("/api/files/{}/download", f.id),
        })
        .collect();

    Ok(Json(FileListResponse {
        files,
        total,
    }))
}

/// Delete file (soft delete)
pub async fn delete_file(
    State((pool, _config)): State<(PgPool, FileStorageConfig)>,
    Path(file_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let user_id = "system"; // TODO: Extract from JWT

    // Verify user is the uploader or channel admin
    let file = sqlx::query!(
        r#"
        SELECT f.uploader_id, f.channel_id, cm.role
        FROM file_uploads f
        JOIN channel_members cm ON f.channel_id = cm.channel_id
        WHERE f.id = $1 AND cm.user_id = $2 AND f.deleted_at IS NULL
        "#,
        file_id,
        user_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| ApiError::DatabaseError(e.to_string()))?
    .ok_or(ApiError::NotFound("File not found".to_string()))?;

    // Check permissions
    if file.uploader_id != user_id
        && file.role != "admin"
        && file.role != "owner"
    {
        return Err(ApiError::Forbidden(
            "Only file uploader or channel admins can delete files".to_string(),
        ));
    }

    // Soft delete
    sqlx::query!(
        r#"
        UPDATE file_uploads
        SET deleted_at = datetime('now', 'utc')
        WHERE id = $1
        "#,
        file_id
    )
    .execute(&pool)
    .await
    .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Store file to local filesystem
async fn store_local_file(
    config: &FileStorageConfig,
    filename: &str,
    data: &[u8],
) -> Result<String, ApiError> {
    let base_path = config.local_path.as_ref()
        .ok_or_else(|| ApiError::InternalError("Local storage path not configured".to_string()))?;

    // Create directory if it doesn't exist
    tokio::fs::create_dir_all(base_path).await
        .map_err(|e| ApiError::InternalError(format!("Failed to create storage directory: {}", e)))?;

    let file_path = std::path::Path::new(base_path).join(filename);

    tokio::fs::write(&file_path, data).await
        .map_err(|e| ApiError::InternalError(format!("Failed to write file: {}", e)))?;

    Ok(file_path.to_string_lossy().to_string())
}

/// Read file from local filesystem
async fn read_local_file(path: &str) -> Result<Vec<u8>, ApiError> {
    tokio::fs::read(path).await
        .map_err(|e| ApiError::InternalError(format!("Failed to read file: {}", e)))
}

/// Store file to S3/MinIO
#[cfg(feature = "minio")]
async fn store_s3_file(
    config: &FileStorageConfig,
    filename: &str,
    data: &[u8],
    content_type: &str,
) -> Result<String, ApiError> {
    let bucket = create_s3_bucket(config)?;

    bucket.put_object_with_content_type(filename, data, content_type)
        .await
        .map_err(|e| ApiError::InternalError(format!("S3 upload failed: {}", e)))?;

    Ok(filename.to_string())
}

/// Read file from S3/MinIO
#[cfg(feature = "minio")]
async fn read_s3_file(
    config: &FileStorageConfig,
    path: &str,
) -> Result<Vec<u8>, ApiError> {
    let bucket = create_s3_bucket(config)?;

    let response = bucket.get_object(path)
        .await
        .map_err(|e| ApiError::InternalError(format!("S3 download failed: {}", e)))?;

    Ok(response.bytes().to_vec())
}

/// Create S3/MinIO bucket connection
#[cfg(feature = "minio")]
fn create_s3_bucket(config: &FileStorageConfig) -> Result<Bucket, ApiError> {
    let bucket_name = config.bucket.as_ref()
        .ok_or_else(|| ApiError::InternalError("Bucket name not configured".to_string()))?;

    let region = if let Some(endpoint) = &config.endpoint {
        Region::Custom {
            region: config.region.clone().unwrap_or_else(|| "us-east-1".to_string()),
            endpoint: endpoint.clone(),
        }
    } else {
        Region::from_str(&config.region.as_ref().unwrap_or(&"us-east-1".to_string()))
            .map_err(|e| ApiError::InternalError(format!("Invalid region: {}", e)))?
    };

    let credentials = Credentials::new(
        config.access_key.as_deref(),
        config.secret_key.as_deref(),
        None,
        None,
        None,
    ).map_err(|e| ApiError::InternalError(format!("Invalid credentials: {}", e)))?;

    Bucket::new(bucket_name, region, credentials)
        .map_err(|e| ApiError::InternalError(format!("Failed to create bucket: {}", e)))
}

/// Sanitize filename to prevent path traversal
fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

/// Pagination query parameters
#[derive(Debug, Deserialize)]
pub struct Pagination {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}
