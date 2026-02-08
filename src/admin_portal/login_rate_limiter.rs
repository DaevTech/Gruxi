use std::{collections::HashMap, sync::OnceLock};

use tokio::{sync::Mutex, time::Instant};

// Login rate limiter: tracks failed login attempts per username
const MAX_FAILED_ATTEMPTS: usize = 5;
const RATE_LIMIT_WINDOW_SECS: u64 = 60;

pub struct LoginRateLimiter {
    attempts: Mutex<HashMap<String, Vec<Instant>>>,
}

impl LoginRateLimiter {
    fn new() -> Self {
        Self { attempts: Mutex::new(HashMap::new()) }
    }

    // Check if a username is currently rate-limited. Returns seconds remaining if limited.
    pub async fn is_rate_limited(&self, username: &str) -> Option<u64> {
        let mut attempts = self.attempts.lock().await;
        let now = Instant::now();
        let window = std::time::Duration::from_secs(RATE_LIMIT_WINDOW_SECS);

        if let Some(timestamps) = attempts.get_mut(username) {
            // Remove attempts outside the window
            timestamps.retain(|t| now.duration_since(*t) < window);

            if timestamps.len() >= MAX_FAILED_ATTEMPTS {
                // Find the oldest attempt in the window to calculate retry-after
                if let Some(oldest) = timestamps.first() {
                    let elapsed = now.duration_since(*oldest);
                    let remaining = RATE_LIMIT_WINDOW_SECS.saturating_sub(elapsed.as_secs());
                    return Some(remaining.max(1));
                }
            }
        }

        None
    }

    // Record a failed login attempt for a username.
    pub async fn record_failed_attempt(&self, username: &str) {
        let mut attempts = self.attempts.lock().await;
        let entry = attempts.entry(username.to_string()).or_insert_with(Vec::new);
        entry.push(Instant::now());
    }

    // Clear failed attempts on successful login.
    pub async fn clear_attempts(&self, username: &str) {
        let mut attempts = self.attempts.lock().await;
        attempts.remove(username);
    }
}

static LOGIN_RATE_LIMITER: OnceLock<LoginRateLimiter> = OnceLock::new();

pub fn get_login_rate_limiter() -> &'static LoginRateLimiter {
    LOGIN_RATE_LIMITER.get_or_init(|| LoginRateLimiter::new())
}
