//! Rate limiting for authentication endpoints
//!
//! Implements per-IP rate limiting to prevent brute force attacks
//! and resource exhaustion from expensive Argon2id computations.

use dashmap::DashMap;
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter as GovRateLimiter,
};
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use tracing::{info, warn};

/// Maximum login attempts per IP per minute
const LOGIN_ATTEMPTS_PER_MINUTE: u32 = 10;

/// Maximum registration attempts per IP per hour
const REGISTRATION_ATTEMPTS_PER_HOUR: u32 = 5;

/// Burst allowance for login (allows brief spikes)
const LOGIN_BURST: u32 = 15;

/// Rate limiter for authentication endpoints
pub struct AuthRateLimiter {
    /// Per-IP login rate limiters
    login_limiters: DashMap<IpAddr, Arc<GovRateLimiter<NotKeyed, InMemoryState, DefaultClock>>>,

    /// Per-IP registration rate limiters
    registration_limiters: DashMap<IpAddr, Arc<GovRateLimiter<NotKeyed, InMemoryState, DefaultClock>>>,

    /// Per-IP device registration rate limiters
    device_registration_limiters: DashMap<IpAddr, Arc<GovRateLimiter<NotKeyed, InMemoryState, DefaultClock>>>,

    /// Configuration
    config: AuthRateLimitConfig,
}

/// Rate limit configuration
#[derive(Clone, Debug)]
pub struct AuthRateLimitConfig {
    pub login_attempts_per_minute: u32,
    pub registration_attempts_per_hour: u32,
    pub login_burst: u32,
}

impl Default for AuthRateLimitConfig {
    fn default() -> Self {
        Self {
            login_attempts_per_minute: LOGIN_ATTEMPTS_PER_MINUTE,
            registration_attempts_per_hour: REGISTRATION_ATTEMPTS_PER_HOUR,
            login_burst: LOGIN_BURST,
        }
    }
}

impl AuthRateLimiter {
    /// Create a new rate limiter with default configuration
    pub fn new() -> Self {
        Self::with_config(AuthRateLimitConfig::default())
    }

    /// Create a rate limiter from environment variables
    pub fn from_env() -> Self {
        let config = AuthRateLimitConfig {
            login_attempts_per_minute: std::env::var("RATE_LIMIT_LOGIN_PER_MINUTE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(LOGIN_ATTEMPTS_PER_MINUTE),
            registration_attempts_per_hour: std::env::var("RATE_LIMIT_REGISTER_PER_HOUR")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(REGISTRATION_ATTEMPTS_PER_HOUR),
            login_burst: std::env::var("RATE_LIMIT_LOGIN_BURST")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(LOGIN_BURST),
        };
        Self::with_config(config)
    }

    /// Create a rate limiter with custom configuration
    pub fn with_config(config: AuthRateLimitConfig) -> Self {
        info!(
            login_limit = config.login_attempts_per_minute,
            register_limit = config.registration_attempts_per_hour,
            login_burst = config.login_burst,
            "Auth rate limiter initialized"
        );

        Self {
            login_limiters: DashMap::new(),
            registration_limiters: DashMap::new(),
            device_registration_limiters: DashMap::new(),
            config,
        }
    }

    /// Check if an IP address is allowed to attempt login
    pub fn check_login(&self, ip: IpAddr) -> bool {
        let limiter = self
            .login_limiters
            .entry(ip)
            .or_insert_with(|| {
                let quota = Quota::per_minute(
                    NonZeroU32::new(self.config.login_attempts_per_minute).unwrap(),
                )
                .allow_burst(NonZeroU32::new(self.config.login_burst).unwrap());
                Arc::new(GovRateLimiter::direct(quota))
            })
            .clone();

        match limiter.check() {
            Ok(_) => true,
            Err(_) => {
                warn!(ip = %ip, "Login rate limit exceeded");
                false
            }
        }
    }

    /// Check if an IP address is allowed to register a user
    pub fn check_registration(&self, ip: IpAddr) -> bool {
        let limiter = self
            .registration_limiters
            .entry(ip)
            .or_insert_with(|| {
                let quota = Quota::per_hour(
                    NonZeroU32::new(self.config.registration_attempts_per_hour).unwrap(),
                );
                Arc::new(GovRateLimiter::direct(quota))
            })
            .clone();

        match limiter.check() {
            Ok(_) => true,
            Err(_) => {
                warn!(ip = %ip, "Registration rate limit exceeded");
                false
            }
        }
    }

    /// Check if an IP address is allowed to register a device
    pub fn check_device_registration(&self, ip: IpAddr) -> bool {
        let limiter = self
            .device_registration_limiters
            .entry(ip)
            .or_insert_with(|| {
                // Allow 10 device registrations per hour per IP
                let quota = Quota::per_hour(NonZeroU32::new(10).unwrap());
                Arc::new(GovRateLimiter::direct(quota))
            })
            .clone();

        match limiter.check() {
            Ok(_) => true,
            Err(_) => {
                warn!(ip = %ip, "Device registration rate limit exceeded");
                false
            }
        }
    }

    /// Get rate limit info for stats endpoint
    pub fn get_info(&self) -> serde_json::Value {
        serde_json::json!({
            "login": {
                "per_minute": self.config.login_attempts_per_minute,
                "burst": self.config.login_burst,
                "active_limiters": self.login_limiters.len()
            },
            "registration": {
                "per_hour": self.config.registration_attempts_per_hour,
                "active_limiters": self.registration_limiters.len()
            },
            "device_registration": {
                "per_hour": 10,
                "active_limiters": self.device_registration_limiters.len()
            }
        })
    }
}

impl Default for AuthRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_login_rate_limit() {
        let limiter = AuthRateLimiter::with_config(AuthRateLimitConfig {
            login_attempts_per_minute: 3,
            login_burst: 3,
            ..Default::default()
        });

        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        // First 3 should succeed
        for _ in 0..3 {
            assert!(limiter.check_login(ip));
        }

        // 4th should fail
        assert!(!limiter.check_login(ip));
    }

    #[test]
    fn test_registration_rate_limit() {
        let limiter = AuthRateLimiter::with_config(AuthRateLimitConfig {
            registration_attempts_per_hour: 2,
            ..Default::default()
        });

        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2));

        // First 2 should succeed
        assert!(limiter.check_registration(ip));
        assert!(limiter.check_registration(ip));

        // 3rd should fail
        assert!(!limiter.check_registration(ip));
    }

    #[test]
    fn test_different_ips_independent() {
        let limiter = AuthRateLimiter::with_config(AuthRateLimitConfig {
            login_attempts_per_minute: 1,
            login_burst: 1,
            ..Default::default()
        });

        let ip1 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2));

        // Exhaust ip1's limit
        assert!(limiter.check_login(ip1));
        assert!(!limiter.check_login(ip1));

        // ip2 should still work
        assert!(limiter.check_login(ip2));
    }
}
