//! Rate limiting for WebSocket connections and messages
//!
//! Implements per-IP and per-user rate limiting using the Governor crate
//! to prevent resource exhaustion and abuse.

use dashmap::DashMap;
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter as GovRateLimiter,
};
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

/// Maximum connections per IP per minute
const IP_CONNECTIONS_PER_MINUTE: u32 = 60;

/// Maximum connections per user per minute
const USER_CONNECTIONS_PER_MINUTE: u32 = 30;

/// Maximum messages per connection per second
const MESSAGES_PER_SECOND: u32 = 50;

/// Burst allowance multiplier
const BURST_MULTIPLIER: u32 = 2;

/// Rate limiter for WebSocket connections
pub struct RateLimiter {
    /// Per-IP connection rate limiters
    ip_limiters: DashMap<IpAddr, Arc<GovRateLimiter<NotKeyed, InMemoryState, DefaultClock>>>,

    /// Per-user connection rate limiters
    user_limiters: DashMap<String, Arc<GovRateLimiter<NotKeyed, InMemoryState, DefaultClock>>>,

    /// Per-connection message rate limiters (keyed by connection ID)
    message_limiters: DashMap<String, Arc<GovRateLimiter<NotKeyed, InMemoryState, DefaultClock>>>,

    /// Configuration
    config: RateLimitConfig,
}

/// Rate limit configuration
#[derive(Clone, Debug)]
pub struct RateLimitConfig {
    pub ip_connections_per_minute: u32,
    pub user_connections_per_minute: u32,
    pub messages_per_second: u32,
    pub burst_multiplier: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            ip_connections_per_minute: IP_CONNECTIONS_PER_MINUTE,
            user_connections_per_minute: USER_CONNECTIONS_PER_MINUTE,
            messages_per_second: MESSAGES_PER_SECOND,
            burst_multiplier: BURST_MULTIPLIER,
        }
    }
}

impl RateLimiter {
    /// Create a new rate limiter with default configuration
    pub fn new() -> Self {
        Self::with_config(RateLimitConfig::default())
    }

    /// Create a rate limiter from environment variables
    pub fn from_env() -> Self {
        let config = RateLimitConfig {
            ip_connections_per_minute: std::env::var("RATE_LIMIT_IP_PER_MINUTE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(IP_CONNECTIONS_PER_MINUTE),
            user_connections_per_minute: std::env::var("RATE_LIMIT_USER_PER_MINUTE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(USER_CONNECTIONS_PER_MINUTE),
            messages_per_second: std::env::var("RATE_LIMIT_MESSAGES_PER_SEC")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(MESSAGES_PER_SECOND),
            burst_multiplier: std::env::var("RATE_LIMIT_BURST_MULTIPLIER")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(BURST_MULTIPLIER),
        };
        Self::with_config(config)
    }

    /// Create a rate limiter with custom configuration
    pub fn with_config(config: RateLimitConfig) -> Self {
        info!(
            ip_limit = config.ip_connections_per_minute,
            user_limit = config.user_connections_per_minute,
            msg_limit = config.messages_per_second,
            "Rate limiter initialized"
        );

        Self {
            ip_limiters: DashMap::new(),
            user_limiters: DashMap::new(),
            message_limiters: DashMap::new(),
            config,
        }
    }

    /// Check if an IP address is allowed to make a new connection
    pub fn check_ip(&self, ip: IpAddr) -> bool {
        let limiter = self
            .ip_limiters
            .entry(ip)
            .or_insert_with(|| {
                let quota = Quota::per_minute(
                    NonZeroU32::new(self.config.ip_connections_per_minute).unwrap(),
                )
                .allow_burst(
                    NonZeroU32::new(
                        self.config.ip_connections_per_minute * self.config.burst_multiplier,
                    )
                    .unwrap(),
                );
                Arc::new(GovRateLimiter::direct(quota))
            })
            .clone();

        match limiter.check() {
            Ok(_) => true,
            Err(_) => {
                warn!(ip = %ip, "IP rate limit exceeded");
                crate::metrics::record_rate_limit_hit("ip");
                false
            }
        }
    }

    /// Check if a user is allowed to make a new connection
    pub fn check_user(&self, user_id: &str) -> bool {
        let limiter = self
            .user_limiters
            .entry(user_id.to_string())
            .or_insert_with(|| {
                let quota = Quota::per_minute(
                    NonZeroU32::new(self.config.user_connections_per_minute).unwrap(),
                )
                .allow_burst(
                    NonZeroU32::new(
                        self.config.user_connections_per_minute * self.config.burst_multiplier,
                    )
                    .unwrap(),
                );
                Arc::new(GovRateLimiter::direct(quota))
            })
            .clone();

        match limiter.check() {
            Ok(_) => true,
            Err(_) => {
                warn!(user_id = %user_id, "User rate limit exceeded");
                crate::metrics::record_rate_limit_hit("user");
                false
            }
        }
    }

    /// Check if a connection is allowed to send a message
    pub fn check_message(&self, connection_id: &str) -> bool {
        let limiter = self
            .message_limiters
            .entry(connection_id.to_string())
            .or_insert_with(|| {
                let quota = Quota::per_second(
                    NonZeroU32::new(self.config.messages_per_second).unwrap(),
                )
                .allow_burst(
                    NonZeroU32::new(
                        self.config.messages_per_second * self.config.burst_multiplier,
                    )
                    .unwrap(),
                );
                Arc::new(GovRateLimiter::direct(quota))
            })
            .clone();

        match limiter.check() {
            Ok(_) => true,
            Err(_) => {
                warn!(connection_id = %connection_id, "Message rate limit exceeded");
                crate::metrics::record_rate_limit_hit("message");
                false
            }
        }
    }

    /// Remove rate limiter for a disconnected connection
    pub fn remove_connection(&self, connection_id: &str) {
        self.message_limiters.remove(connection_id);
    }

    /// Clean up stale rate limiters (should be called periodically)
    pub fn cleanup(&self) {
        // IP limiters: remove those with no recent activity
        // In production, implement LRU eviction or TTL-based cleanup
        let ip_count = self.ip_limiters.len();
        let user_count = self.user_limiters.len();
        let msg_count = self.message_limiters.len();

        info!(
            ip_limiters = ip_count,
            user_limiters = user_count,
            message_limiters = msg_count,
            "Rate limiter state"
        );
    }

    /// Get information about IP rate limit configuration
    pub fn ip_limit_info(&self) -> serde_json::Value {
        serde_json::json!({
            "per_minute": self.config.ip_connections_per_minute,
            "burst": self.config.ip_connections_per_minute * self.config.burst_multiplier,
            "active_limiters": self.ip_limiters.len()
        })
    }

    /// Get information about user rate limit configuration
    pub fn user_limit_info(&self) -> serde_json::Value {
        serde_json::json!({
            "per_minute": self.config.user_connections_per_minute,
            "burst": self.config.user_connections_per_minute * self.config.burst_multiplier,
            "active_limiters": self.user_limiters.len()
        })
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_ip_rate_limit() {
        let limiter = RateLimiter::with_config(RateLimitConfig {
            ip_connections_per_minute: 5,
            burst_multiplier: 1,
            ..Default::default()
        });

        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        // First 5 should succeed
        for _ in 0..5 {
            assert!(limiter.check_ip(ip));
        }

        // 6th should fail
        assert!(!limiter.check_ip(ip));
    }

    #[test]
    fn test_user_rate_limit() {
        let limiter = RateLimiter::with_config(RateLimitConfig {
            user_connections_per_minute: 3,
            burst_multiplier: 1,
            ..Default::default()
        });

        // First 3 should succeed
        for _ in 0..3 {
            assert!(limiter.check_user("test_user"));
        }

        // 4th should fail
        assert!(!limiter.check_user("test_user"));

        // Different user should work
        assert!(limiter.check_user("other_user"));
    }

    #[test]
    fn test_message_rate_limit() {
        let limiter = RateLimiter::with_config(RateLimitConfig {
            messages_per_second: 10,
            burst_multiplier: 1,
            ..Default::default()
        });

        // First 10 should succeed
        for _ in 0..10 {
            assert!(limiter.check_message("conn_1"));
        }

        // 11th should fail
        assert!(!limiter.check_message("conn_1"));

        // Different connection should work
        assert!(limiter.check_message("conn_2"));
    }

    #[test]
    fn test_connection_cleanup() {
        let limiter = RateLimiter::new();

        // Create some limiters
        limiter.check_message("conn_1");
        limiter.check_message("conn_2");

        assert_eq!(limiter.message_limiters.len(), 2);

        // Remove one
        limiter.remove_connection("conn_1");

        assert_eq!(limiter.message_limiters.len(), 1);
    }
}
