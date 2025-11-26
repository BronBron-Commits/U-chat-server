//! JWT authentication utilities for chat service

use async_trait::async_trait;
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    RequestPartsExt,
};
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use jwt_common::TokenService;
use std::sync::OnceLock;

static TOKEN_SERVICE: OnceLock<TokenService> = OnceLock::new();

/// Initialize the token service from environment
pub fn init_token_service() {
    TOKEN_SERVICE.get_or_init(TokenService::from_env);
}

/// Get the global token service instance
fn token_service() -> &'static TokenService {
    TOKEN_SERVICE
        .get()
        .expect("TokenService not initialized. Call init_token_service() first.")
}

/// Authenticated user extractor
///
/// Use this in handler parameters to automatically extract and validate
/// the user ID from the JWT token in the Authorization header.
///
/// # Example
///
/// ```rust,ignore
/// async fn my_handler(
///     AuthUser(user_id): AuthUser,
///     // ... other params
/// ) -> Result<Json<Response>, ApiError> {
///     // user_id is the authenticated user's ID from JWT
///     Ok(Json(Response { ... }))
/// }
/// ```
#[derive(Debug, Clone)]
pub struct AuthUser(pub String);

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract Authorization header with Bearer token
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| {
                (
                    StatusCode::UNAUTHORIZED,
                    "Missing or invalid Authorization header".to_string(),
                )
            })?;

        // Validate token and extract user ID
        let service = token_service();
        let claims = service.validate(bearer.token()).map_err(|e| {
            (
                StatusCode::UNAUTHORIZED,
                format!("Invalid JWT token: {}", e),
            )
        })?;

        Ok(AuthUser(claims.sub))
    }
}

/// Optional authenticated user extractor
///
/// Similar to AuthUser, but allows unauthenticated requests.
/// Returns None if no valid token is provided.
#[derive(Debug, Clone)]
pub struct OptionalAuthUser(pub Option<String>);

#[async_trait]
impl<S> FromRequestParts<S> for OptionalAuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth_header = parts.extract::<TypedHeader<Authorization<Bearer>>>().await;

        match auth_header {
            Ok(TypedHeader(Authorization(bearer))) => {
                let service = token_service();
                match service.validate(bearer.token()) {
                    Ok(claims) => Ok(OptionalAuthUser(Some(claims.sub))),
                    Err(_) => Ok(OptionalAuthUser(None)),
                }
            }
            Err(_) => Ok(OptionalAuthUser(None)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jwt_common::Claims;

    #[test]
    fn test_token_service_initialization() {
        std::env::set_var("JWT_SECRET", "test-secret-key");
        init_token_service();

        let service = token_service();
        let claims = Claims::new("testuser", 3600, None);
        let token = service.generate(&claims).expect("Token generation failed");

        let validated = service.validate(&token).expect("Token validation failed");
        assert_eq!(validated.sub, "testuser");
    }
}
