//! Flaky dummy plugin implementation for testing intermittent failures

use async_trait::async_trait;
use glimpse_sdk::{Metadata, Method, MethodResult, Plugin, PluginError, SearchItem};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::sleep;

/// A plugin that exhibits flaky behavior - sometimes succeeds, sometimes fails
#[derive(Debug, Clone)]
pub struct FlakyDummyPlugin {
    metadata: Metadata,
    config: FlakyConfig,
    call_counter: Arc<AtomicUsize>,
}

/// Configuration for flaky behavior patterns
#[derive(Debug, Clone)]
pub struct FlakyConfig {
    /// Fail every Nth call (None means never fail based on count)
    fail_every_n: Option<usize>,
    /// Probability of failure (0.0 = never fail, 1.0 = always fail)
    failure_rate: f64,
    /// Whether to introduce random delays
    random_delays: bool,
    /// Base delay for operations
    base_delay: Duration,
    /// Error type to return on failure
    failure_error: PluginError,
    /// Whether to panic instead of returning error
    panic_on_failure: bool,
}

impl FlakyConfig {
    /// Create a config that never fails (for baseline testing)
    pub fn reliable() -> Self {
        Self {
            fail_every_n: None,
            failure_rate: 0.0,
            random_delays: false,
            base_delay: Duration::from_millis(1),
            failure_error: PluginError::Other("Flaky failure".to_string()),
            panic_on_failure: false,
        }
    }

    /// Create a config that fails every N calls
    pub fn fail_every_n(n: usize) -> Self {
        Self {
            fail_every_n: Some(n),
            failure_rate: 0.0,
            random_delays: false,
            base_delay: Duration::from_millis(1),
            failure_error: PluginError::Other(format!("Failure every {} calls", n)),
            panic_on_failure: false,
        }
    }

    /// Create a config with specific failure rate (0.0 to 1.0)
    pub fn with_failure_rate(rate: f64) -> Self {
        Self {
            fail_every_n: None,
            failure_rate: rate.clamp(0.0, 1.0),
            random_delays: false,
            base_delay: Duration::from_millis(1),
            failure_error: PluginError::Other(format!("Random failure (rate: {})", rate)),
            panic_on_failure: false,
        }
    }

    /// Create a config with random delays
    pub fn with_random_delays(base_delay: Duration) -> Self {
        Self {
            fail_every_n: None,
            failure_rate: 0.0,
            random_delays: true,
            base_delay,
            failure_error: PluginError::Other("Flaky failure".to_string()),
            panic_on_failure: false,
        }
    }

    /// Create a config that panics on failure instead of returning error
    pub fn panic_on_failure() -> Self {
        Self {
            fail_every_n: Some(3), // Panic every 3rd call
            failure_rate: 0.0,
            random_delays: false,
            base_delay: Duration::from_millis(1),
            failure_error: PluginError::Other("This should never be seen".to_string()),
            panic_on_failure: true,
        }
    }

    /// Create a highly unreliable config (for stress testing)
    pub fn very_unreliable() -> Self {
        Self {
            fail_every_n: Some(2),
            failure_rate: 0.7,
            random_delays: true,
            base_delay: Duration::from_millis(50),
            failure_error: PluginError::Io(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "Very unreliable connection",
            )),
            panic_on_failure: false,
        }
    }

    /// Set custom error type
    pub fn with_error(mut self, error: PluginError) -> Self {
        self.failure_error = error;
        self
    }
}

impl FlakyDummyPlugin {
    /// Create a new reliable flaky plugin
    pub fn new() -> Self {
        Self {
            metadata: Metadata {
                id: "test.flaky".to_string(),
                name: "Flaky Test Plugin".to_string(),
                version: "1.0.0".to_string(),
                description: "A flaky test plugin for intermittent failure testing".to_string(),
                author: "Test Suite".to_string(),
            },
            config: FlakyConfig::reliable(),
            call_counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Create a flaky plugin with specific configuration
    pub fn with_config(config: FlakyConfig) -> Self {
        Self {
            metadata: Metadata {
                id: "test.flaky_configured".to_string(),
                name: "Configured Flaky Test Plugin".to_string(),
                version: "1.0.0".to_string(),
                description: "A configured flaky test plugin".to_string(),
                author: "Test Suite".to_string(),
            },
            config,
            call_counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Create a plugin that fails every N calls
    pub fn fail_every_n(n: usize) -> Self {
        Self::with_config(FlakyConfig::fail_every_n(n))
    }

    /// Create a plugin with specific failure rate
    pub fn with_failure_rate(rate: f64) -> Self {
        Self::with_config(FlakyConfig::with_failure_rate(rate))
    }

    /// Create a plugin with random delays
    pub fn with_random_delays(base_delay: Duration) -> Self {
        Self::with_config(FlakyConfig::with_random_delays(base_delay))
    }

    /// Create a very unreliable plugin for stress testing
    pub fn very_unreliable() -> Self {
        Self::with_config(FlakyConfig::very_unreliable())
    }

    /// Check if this call should fail
    async fn should_fail(&self) -> bool {
        let call_count = self.call_counter.fetch_add(1, Ordering::SeqCst) + 1;

        // Check count-based failure
        if let Some(n) = self.config.fail_every_n {
            if call_count % n == 0 {
                return true;
            }
        }

        // Check probability-based failure
        if self.config.failure_rate > 0.0 {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            // Use deterministic "randomness" based on call count for reproducible tests
            let mut hasher = DefaultHasher::new();
            call_count.hash(&mut hasher);
            let hash = hasher.finish();
            let pseudo_random = (hash % 1000) as f64 / 1000.0;

            if pseudo_random < self.config.failure_rate {
                return true;
            }
        }

        false
    }

    /// Add random delay if configured
    async fn maybe_add_delay(&self) {
        if self.config.random_delays {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let call_count = self.call_counter.load(Ordering::SeqCst);
            let mut hasher = DefaultHasher::new();
            (call_count * 2).hash(&mut hasher); // Different seed than failure check
            let hash = hasher.finish();
            let delay_multiplier = 1.0 + (hash % 100) as f64 / 100.0; // 1.0 to 2.0x base delay

            let delay = Duration::from_nanos(
                (self.config.base_delay.as_nanos() as f64 * delay_multiplier) as u64,
            );
            sleep(delay).await;
        } else {
            sleep(self.config.base_delay).await;
        }
    }
}

impl Default for FlakyDummyPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for FlakyDummyPlugin {
    fn metadata(&self) -> Metadata {
        self.metadata.clone()
    }

    async fn handle(&self, method: Method) -> Result<MethodResult, PluginError> {
        // Add delay if configured
        self.maybe_add_delay().await;

        // Check if we should fail this call
        if self.should_fail().await {
            if self.config.panic_on_failure {
                panic!("Flaky plugin panic: {}", self.config.failure_error);
            } else {
                return Err(self.config.failure_error.clone());
            }
        }

        // Success case
        match method {
            Method::Search(query) => {
                let results = vec![SearchItem {
                    title: format!("Flaky result for '{}'", query),
                    subtitle: Some("This result might not always appear".to_string()),
                    icon: Some("flaky-icon.png".to_string()),
                    actions: vec![],
                    score: 0.7,
                }];
                Ok(MethodResult::SearchResults(results))
            }
            Method::Cancel => Ok(MethodResult::SearchResults(vec![])),
            Method::Quit => Ok(MethodResult::SearchResults(vec![])),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_reliable_flaky_plugin() {
        let plugin = FlakyDummyPlugin::new();

        // Should succeed multiple times
        for _ in 0..10 {
            let result = plugin.handle(Method::Search("test".to_string())).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_fail_every_n_plugin() {
        let plugin = FlakyDummyPlugin::fail_every_n(3);

        // First two calls should succeed
        let result1 = plugin.handle(Method::Search("test1".to_string())).await;
        let result2 = plugin.handle(Method::Search("test2".to_string())).await;
        assert!(result1.is_ok());
        assert!(result2.is_ok());

        // Third call should fail
        let result3 = plugin.handle(Method::Search("test3".to_string())).await;
        assert!(result3.is_err());

        // Fourth and fifth calls should succeed
        let result4 = plugin.handle(Method::Search("test4".to_string())).await;
        let result5 = plugin.handle(Method::Search("test5".to_string())).await;
        assert!(result4.is_ok());
        assert!(result5.is_ok());

        // Sixth call should fail
        let result6 = plugin.handle(Method::Search("test6".to_string())).await;
        assert!(result6.is_err());
    }

    #[tokio::test]
    async fn test_failure_rate_plugin() {
        let plugin = FlakyDummyPlugin::with_failure_rate(1.0); // Always fail

        // Should always fail
        for _ in 0..5 {
            let result = plugin.handle(Method::Search("test".to_string())).await;
            assert!(result.is_err());
        }

        let plugin = FlakyDummyPlugin::with_failure_rate(0.0); // Never fail

        // Should always succeed
        for _ in 0..5 {
            let result = plugin.handle(Method::Search("test".to_string())).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_random_delays() {
        use tokio::time::Instant;

        let plugin = FlakyDummyPlugin::with_random_delays(Duration::from_millis(10));

        let start = Instant::now();
        let _result = plugin.handle(Method::Search("test".to_string())).await;
        let elapsed = start.elapsed();

        // Should have some delay (at least base delay)
        assert!(elapsed >= Duration::from_millis(9)); // Account for timing variations
    }

    #[tokio::test]
    async fn test_very_unreliable_plugin() {
        let plugin = FlakyDummyPlugin::very_unreliable();
        let metadata = plugin.metadata();

        assert_eq!(metadata.id, "test.flaky_configured");

        // Test a few calls - some should fail, some might succeed
        let mut failures = 0;
        let mut successes = 0;

        for i in 0..10 {
            let result = plugin.handle(Method::Search(format!("test{}", i))).await;
            if result.is_err() {
                failures += 1;
            } else {
                successes += 1;
            }
        }

        // Very unreliable plugin should have some failures
        assert!(failures > 0);
    }

    #[test]
    fn test_config_creation() {
        let config = FlakyConfig::fail_every_n(5);
        assert_eq!(config.fail_every_n, Some(5));
        assert_eq!(config.failure_rate, 0.0);

        let config = FlakyConfig::with_failure_rate(0.3);
        assert_eq!(config.failure_rate, 0.3);
        assert_eq!(config.fail_every_n, None);

        let config = FlakyConfig::very_unreliable();
        assert!(config.failure_rate > 0.5);
        assert!(config.fail_every_n.is_some());
    }
}
