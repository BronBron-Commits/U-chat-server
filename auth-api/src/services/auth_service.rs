//! Argon2id-based password hashing service
//!
//! This module provides secure password hashing using Argon2id, the Password Hashing
//! Competition winner. Parameters exceed OWASP 2024+ minimums for memory-hard security.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use rand_core::OsRng;

/// Strong Argon2id password hashing service
///
/// Uses OWASP-compliant parameters:
/// - Memory: ~48 MiB (49152 KiB)
/// - Iterations: 3
/// - Parallelism: 1 (to avoid exhausting async runtime thread pool)
pub struct PasswordService {
    argon2: Argon2<'static>,
}

impl Default for PasswordService {
    fn default() -> Self {
        Self::new()
    }
}

impl PasswordService {
    /// Initialize Argon2id with OWASP-compliant memory-hard parameters
    ///
    /// Configuration: ~48 MiB memory, 3 iterations, parallelism = 1
    /// This exceeds OWASP's baseline (~19 MiB, 2 iterations) to future-proof
    /// against advancing attacker capabilities.
    pub fn new() -> Self {
        // Configure Argon2id with ~48 MiB memory, 3 iterations, parallelism = 1
        let params = Params::new(48 * 1024, 3, 1, None).expect("Invalid Argon2 parameters");
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        PasswordService { argon2 }
    }

    /// Initialize Argon2id with reduced parameters for development/testing
    ///
    /// Uses lower memory (4 MiB) and iterations for faster test execution.
    /// NEVER use this in production.
    #[cfg(any(test, debug_assertions))]
    pub fn new_dev() -> Self {
        // Reduced parameters for development: 4 MiB memory, 1 iteration
        let params = Params::new(4 * 1024, 1, 1, None).expect("Invalid Argon2 parameters");
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        PasswordService { argon2 }
    }

    /// Hash a plaintext password, returning the PHC-formatted hash string
    ///
    /// Generates a random 128-bit (16-byte) salt using a cryptographically secure
    /// random number generator (OsRng). The output includes algorithm, version,
    /// parameters, salt, and hash in PHC format.
    ///
    /// Example output format: `$argon2id$v=19$m=49152,t=3,p=1$<salt>$<hash>`
    pub fn hash_password(&self, password: &str) -> Result<String, argon2::password_hash::Error> {
        // Generate a random 128-bit salt for each password
        let salt = SaltString::generate(&mut OsRng);
        // Hash the password with Argon2id; output includes algorithm, salt, params, hash
        let password_hash = self.argon2.hash_password(password.as_bytes(), &salt)?;
        Ok(password_hash.to_string())
    }

    /// Verify a plaintext password against a stored hash string (constant-time)
    ///
    /// Parses the stored PHC-formatted hash (includes parameters and salt) and
    /// performs constant-time verification to prevent timing side-channel attacks.
    pub fn verify_password(
        &self,
        password: &str,
        password_hash: &str,
    ) -> Result<bool, argon2::password_hash::Error> {
        // Parse the stored hash (includes parameters and salt)
        let parsed_hash = PasswordHash::new(password_hash)?;
        // Use constant-time verification provided by the Argon2 crate
        match self.argon2.verify_password(password.as_bytes(), &parsed_hash) {
            Ok(_) => Ok(true),                                      // password is correct
            Err(argon2::password_hash::Error::Password) => Ok(false), // password mismatch
            Err(e) => Err(e),                                       // other errors (e.g. malformed hash)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_service_roundtrip() {
        let svc = PasswordService::new_dev();
        let pw = "S3cur3P@ssw0rd";
        let hash = svc.hash_password(pw).expect("Hashing failed");

        // Verify the hash starts with expected Argon2id prefix
        assert!(hash.starts_with("$argon2id$"), "Hash should be in PHC format");

        // Verify correct password matches
        assert!(svc.verify_password(pw, &hash).unwrap(), "Correct password should verify");

        // Verify wrong password fails
        assert!(!svc.verify_password("wrongPassword", &hash).unwrap(), "Wrong password should fail");
    }

    #[test]
    fn test_unique_salts() {
        let svc = PasswordService::new_dev();
        let pw = "testPassword123";

        let hash1 = svc.hash_password(pw).expect("Hashing failed");
        let hash2 = svc.hash_password(pw).expect("Hashing failed");

        // Same password should produce different hashes due to unique salts
        assert_ne!(hash1, hash2, "Same password should produce different hashes");

        // Both should still verify correctly
        assert!(svc.verify_password(pw, &hash1).unwrap());
        assert!(svc.verify_password(pw, &hash2).unwrap());
    }

    #[test]
    fn test_empty_password() {
        let svc = PasswordService::new_dev();
        let hash = svc.hash_password("").expect("Hashing empty password should work");
        assert!(svc.verify_password("", &hash).unwrap());
        assert!(!svc.verify_password("notempty", &hash).unwrap());
    }

    #[test]
    fn test_long_password() {
        let svc = PasswordService::new_dev();
        let long_pw = "a".repeat(1000);
        let hash = svc.hash_password(&long_pw).expect("Hashing long password should work");
        assert!(svc.verify_password(&long_pw, &hash).unwrap());
    }

    #[test]
    fn test_unicode_password() {
        let svc = PasswordService::new_dev();
        let unicode_pw = "密码🔐пароль";
        let hash = svc.hash_password(unicode_pw).expect("Hashing unicode password should work");
        assert!(svc.verify_password(unicode_pw, &hash).unwrap());
    }

    #[test]
    fn test_invalid_hash_format() {
        let svc = PasswordService::new_dev();
        let result = svc.verify_password("anypassword", "not-a-valid-hash");
        assert!(result.is_err(), "Invalid hash format should return error");
    }

    #[test]
    fn test_production_params() {
        // Verify production service can be instantiated (but use dev for actual hashing)
        let prod_svc = PasswordService::new();
        let dev_svc = PasswordService::new_dev();

        // Hash with dev (faster), verify with prod should still work
        // since verification reads params from the hash
        let pw = "testPassword";
        let hash = dev_svc.hash_password(pw).expect("Hashing failed");

        // Production service should be able to verify dev hashes
        assert!(prod_svc.verify_password(pw, &hash).unwrap());
    }
}
