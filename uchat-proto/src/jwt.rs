use chrono::{Utc, Duration};
use jsonwebtoken::{encode, decode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

pub fn create_token(secret: &str, username: &str) -> String {
    let expiration = Utc::now() + Duration::hours(12);
    let claims = Claims {
        sub: username.to_string(),
        exp: expiration.timestamp() as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    ).unwrap()
}

pub fn verify_token(secret: &str, token: &str) -> Option<String> {
    let validation = Validation::new(Algorithm::HS256);
    let decoded = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    ).ok()?;

    Some(decoded.claims.sub)
}
