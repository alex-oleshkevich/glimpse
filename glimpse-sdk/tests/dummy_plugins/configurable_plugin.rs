//! Configurable dummy plugin implementation for flexible testing scenarios

use async_trait::async_trait;
use glimpse_sdk::{Action, Metadata, Method, MethodResult, Plugin, PluginError, SearchItem};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// A highly configurable plugin that can simulate any behavior needed for testing
#[derive(Debug, Clone)]
pub struct ConfigurableDummyPlugin {
    metadata: Metadata,
    behavior: PluginBehavior,
    call_counter: Arc<AtomicUsize>,
}

/// Complete behavior configuration for the plugin
#[derive(Debug, Clone)]
pub struct PluginBehavior {
    /// Method-specific configurations
    method_configs: HashMap<String, MethodConfig>,
    /// Default configuration for unconfigured methods
    default_config: MethodConfig,
    /// Global plugin settings
    global_settings: GlobalSettings,
}

/// Configuration for a specific method
#[derive(Debug, Clone)]
pub struct MethodConfig {
    /// Delay before processing
    delay: Duration,
    /// Whether to return an error
    error_response: Option<PluginError>,
    /// Whether to panic instead of returning
    should_panic: bool,
    /// Panic message if should_panic is true
    panic_message: String,
    /// Custom search results for search method
    search_results: Vec<SearchItem>,
    /// Success probability (0.0 to 1.0)
    success_rate: f64,
}

/// Global plugin settings
#[derive(Debug, Clone)]
pub struct GlobalSettings {
    /// Whether to track call counts
    track_calls: bool,
    /// Maximum number of calls before auto-failure
    max_calls: Option<usize>,
    /// Whether to simulate memory issues
    simulate_memory_pressure: bool,
    /// Custom metadata to override defaults
    custom_metadata: Option<Metadata>,
}

impl MethodConfig {
    /// Create a successful method config
    pub fn success() -> Self {
        Self {
            delay: Duration::from_millis(1),
            error_response: None,
            should_panic: false,
            panic_message: "Configured panic".to_string(),
            search_results: vec![],
            success_rate: 1.0,
        }
    }

    /// Create an error method config
    pub fn error(error: PluginError) -> Self {
        Self {
            delay: Duration::from_millis(1),
            error_response: Some(error),
            should_panic: false,
            panic_message: "Configured panic".to_string(),
            search_results: vec![],
            success_rate: 0.0,
        }
    }

    /// Create a panic method config
    pub fn panic(message: &str) -> Self {
        Self {
            delay: Duration::from_millis(1),
            error_response: None,
            should_panic: true,
            panic_message: message.to_string(),
            search_results: vec![],
            success_rate: 0.0,
        }
    }

    /// Create a slow method config
    pub fn slow(delay: Duration) -> Self {
        Self {
            delay,
            error_response: None,
            should_panic: false,
            panic_message: "Configured panic".to_string(),
            search_results: vec![],
            success_rate: 1.0,
        }
    }

    /// Set custom search results
    pub fn with_results(mut self, results: Vec<SearchItem>) -> Self {
        self.search_results = results;
        self
    }

    /// Set success rate
    pub fn with_success_rate(mut self, rate: f64) -> Self {
        self.success_rate = rate.clamp(0.0, 1.0);
        self
    }

    /// Set delay
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

impl GlobalSettings {
    /// Create default global settings
    pub fn default() -> Self {
        Self {
            track_calls: true,
            max_calls: None,
            simulate_memory_pressure: false,
            custom_metadata: None,
        }
    }

    /// Set maximum calls before failure
    pub fn with_max_calls(mut self, max: usize) -> Self {
        self.max_calls = Some(max);
        self
    }

    /// Enable memory pressure simulation
    pub fn with_memory_pressure(mut self) -> Self {
        self.simulate_memory_pressure = true;
        self
    }

    /// Set custom metadata
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.custom_metadata = Some(metadata);
        self
    }
}

impl PluginBehavior {
    /// Create behavior with all methods succeeding
    pub fn all_success() -> Self {
        Self {
            method_configs: HashMap::new(),
            default_config: MethodConfig::success(),
            global_settings: GlobalSettings::default(),
        }
    }

    /// Create behavior with all methods failing
    pub fn all_error(error: PluginError) -> Self {
        Self {
            method_configs: HashMap::new(),
            default_config: MethodConfig::error(error),
            global_settings: GlobalSettings::default(),
        }
    }

    /// Create behavior with all methods panicking
    pub fn all_panic(message: &str) -> Self {
        Self {
            method_configs: HashMap::new(),
            default_config: MethodConfig::panic(message),
            global_settings: GlobalSettings::default(),
        }
    }

    /// Create behavior with specific method configuration
    pub fn with_method_config(mut self, method: &str, config: MethodConfig) -> Self {
        self.method_configs.insert(method.to_string(), config);
        self
    }

    /// Set global settings
    pub fn with_global_settings(mut self, settings: GlobalSettings) -> Self {
        self.global_settings = settings;
        self
    }

    /// Get configuration for a method
    fn get_config(&self, method: &str) -> &MethodConfig {
        self.method_configs
            .get(method)
            .unwrap_or(&self.default_config)
    }
}

impl ConfigurableDummyPlugin {
    /// Create a new configurable plugin with default behavior
    pub fn new() -> Self {
        Self {
            metadata: Metadata {
                id: "test.configurable".to_string(),
                name: "Configurable Test Plugin".to_string(),
                version: "1.0.0".to_string(),
                description: "A fully configurable test plugin".to_string(),
                author: "Test Suite".to_string(),
            },
            behavior: PluginBehavior::all_success(),
            call_counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Create a configurable plugin with specific behavior
    pub fn with_behavior(behavior: PluginBehavior) -> Self {
        let metadata = behavior
            .global_settings
            .custom_metadata
            .clone()
            .unwrap_or_else(|| Metadata {
                id: "test.configurable_custom".to_string(),
                name: "Custom Configurable Test Plugin".to_string(),
                version: "1.0.0".to_string(),
                description: "A custom configured test plugin".to_string(),
                author: "Test Suite".to_string(),
            });

        Self {
            metadata,
            behavior,
            call_counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Create a plugin that succeeds for search but fails for other methods
    pub fn search_only_success() -> Self {
        let behavior = PluginBehavior::all_error(PluginError::Other("Not search method".to_string()))
            .with_method_config("search", MethodConfig::success());
        Self::with_behavior(behavior)
    }

    /// Create a plugin with method-specific delays
    pub fn with_method_delays(
        search_delay: Duration,
        cancel_delay: Duration,
        quit_delay: Duration,
    ) -> Self {
        let behavior = PluginBehavior::all_success()
            .with_method_config("search", MethodConfig::slow(search_delay))
            .with_method_config("cancel", MethodConfig::slow(cancel_delay))
            .with_method_config("quit", MethodConfig::slow(quit_delay));
        Self::with_behavior(behavior)
    }

    /// Create a plugin with custom search results
    pub fn with_custom_results(results: Vec<SearchItem>) -> Self {
        let behavior = PluginBehavior::all_success().with_method_config(
            "search",
            MethodConfig::success().with_results(results),
        );
        Self::with_behavior(behavior)
    }

    /// Create a plugin that fails after max calls
    pub fn fail_after_calls(max_calls: usize) -> Self {
        let behavior = PluginBehavior::all_success()
            .with_global_settings(GlobalSettings::default().with_max_calls(max_calls));
        Self::with_behavior(behavior)
    }

    /// Check if we should fail based on call count
    fn should_fail_on_call_count(&self) -> bool {
        if let Some(max_calls) = self.behavior.global_settings.max_calls {
            let current_calls = self.call_counter.load(Ordering::SeqCst);
            return current_calls >= max_calls;
        }
        false
    }

    /// Simulate memory pressure if configured
    async fn simulate_memory_pressure(&self) {
        if self.behavior.global_settings.simulate_memory_pressure {
            // Simulate memory allocation (small amount for testing)
            let _memory_pressure: Vec<u8> = vec![0; 1024]; // 1KB allocation
            sleep(Duration::from_millis(1)).await; // Small delay to simulate pressure
        }
    }

    /// Check success rate for probabilistic failures
    fn check_success_rate(&self, config: &MethodConfig, call_count: usize) -> bool {
        if config.success_rate >= 1.0 {
            return true;
        }
        if config.success_rate <= 0.0 {
            return false;
        }

        // Deterministic pseudo-random based on call count
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        call_count.hash(&mut hasher);
        let hash = hasher.finish();
        let pseudo_random = (hash % 1000) as f64 / 1000.0;

        pseudo_random < config.success_rate
    }
}

impl Default for ConfigurableDummyPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for ConfigurableDummyPlugin {
    fn metadata(&self) -> Metadata {
        self.metadata.clone()
    }

    async fn handle(&self, method: Method) -> Result<MethodResult, PluginError> {
        // Check if we should fail due to call count limit BEFORE incrementing
        if self.should_fail_on_call_count() {
            let current_calls = self.call_counter.load(Ordering::SeqCst);
            return Err(PluginError::Other(format!(
                "Maximum calls exceeded: {}",
                current_calls
            )));
        }

        // Increment call counter if tracking is enabled
        let call_count = if self.behavior.global_settings.track_calls {
            self.call_counter.fetch_add(1, Ordering::SeqCst) + 1
        } else {
            0
        };

        // Simulate memory pressure if configured
        self.simulate_memory_pressure().await;

        // Get method-specific configuration
        let method_name = match &method {
            Method::Search(_) => "search",
            Method::Cancel => "cancel",
            Method::Quit => "quit",
        };

        let config = self.behavior.get_config(method_name);

        // Apply delay
        if config.delay > Duration::from_millis(0) {
            sleep(config.delay).await;
        }

        // Check success rate
        if !self.check_success_rate(config, call_count) {
            return Err(config.error_response.clone().unwrap_or_else(|| {
                PluginError::Other("Probabilistic failure".to_string())
            }));
        }

        // Check if should panic
        if config.should_panic {
            panic!("{}", config.panic_message);
        }

        // Check if should return error
        if let Some(error) = &config.error_response {
            return Err(error.clone());
        }

        // Success case
        match method {
            Method::Search(query) => {
                let results = if config.search_results.is_empty() {
                    vec![SearchItem {
                        title: format!("Configurable result for '{}'", query),
                        subtitle: Some(format!("Call #{}", call_count)),
                        icon: Some("configurable-icon.png".to_string()),
                        actions: vec![
                            Action::Clipboard {
                                text: format!("Configured: {}", query),
                            },
                            Action::ShellExec {
                                command: "echo".to_string(),
                                args: vec![format!("Call #{}: {}", call_count, query)],
                            },
                        ],
                        score: 0.95,
                    }]
                } else {
                    config.search_results.clone()
                };

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
    async fn test_configurable_plugin_default() {
        let plugin = ConfigurableDummyPlugin::new();
        let result = plugin.handle(Method::Search("test".to_string())).await;
        assert!(result.is_ok());

        match result.unwrap() {
            MethodResult::SearchResults(results) => {
                assert_eq!(results.len(), 1);
                assert!(results[0].title.contains("Configurable result"));
            }
            MethodResult::Authenticate(_) => panic!("Unexpected authenticate result"),
        }
    }

    #[tokio::test]
    async fn test_method_specific_behavior() {
        let behavior = PluginBehavior::all_success()
            .with_method_config(
                "search",
                MethodConfig::error(PluginError::Other("Search failed".to_string())),
            )
            .with_method_config("cancel", MethodConfig::success());

        let plugin = ConfigurableDummyPlugin::with_behavior(behavior);

        // Search should fail
        let result = plugin.handle(Method::Search("test".to_string())).await;
        assert!(result.is_err());

        // Cancel should succeed
        let result = plugin.handle(Method::Cancel).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_custom_search_results() {
        let custom_results = vec![
            SearchItem {
                title: "Custom Result 1".to_string(),
                subtitle: Some("Custom subtitle".to_string()),
                icon: None,
                actions: vec![],
                score: 1.0,
            },
            SearchItem {
                title: "Custom Result 2".to_string(),
                subtitle: None,
                icon: Some("custom-icon.png".to_string()),
                actions: vec![],
                score: 0.9,
            },
        ];

        let plugin = ConfigurableDummyPlugin::with_custom_results(custom_results.clone());
        let result = plugin.handle(Method::Search("test".to_string())).await;

        assert!(result.is_ok());
        match result.unwrap() {
            MethodResult::SearchResults(results) => {
                assert_eq!(results.len(), 2);
                assert_eq!(results[0].title, "Custom Result 1");
                assert_eq!(results[1].title, "Custom Result 2");
            }
            MethodResult::Authenticate(_) => panic!("Unexpected authenticate result"),
        }
    }

    #[tokio::test]
    async fn test_fail_after_calls() {
        let plugin = ConfigurableDummyPlugin::fail_after_calls(3);

        // First 3 calls should succeed
        for i in 0..3 {
            let result = plugin
                .handle(Method::Search(format!("test{}", i)))
                .await;
            assert!(result.is_ok(), "Call {} should succeed", i);
        }

        // 4th call should fail
        let result = plugin.handle(Method::Search("test3".to_string())).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_method_delays() {
        use tokio::time::Instant;

        let plugin = ConfigurableDummyPlugin::with_method_delays(
            Duration::from_millis(50),
            Duration::from_millis(25),
            Duration::from_millis(10),
        );

        // Test search delay
        let start = Instant::now();
        let _result = plugin.handle(Method::Search("test".to_string())).await;
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(45)); // Account for timing variations

        // Test cancel delay
        let start = Instant::now();
        let _result = plugin.handle(Method::Cancel).await;
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(20));
    }

    #[test]
    fn test_behavior_configuration() {
        let behavior = PluginBehavior::all_success()
            .with_method_config("search", MethodConfig::panic("Test panic"))
            .with_global_settings(GlobalSettings::default().with_max_calls(5));

        let search_config = behavior.get_config("search");
        assert!(search_config.should_panic);
        assert_eq!(search_config.panic_message, "Test panic");

        let cancel_config = behavior.get_config("cancel");
        assert!(!cancel_config.should_panic);

        assert_eq!(behavior.global_settings.max_calls, Some(5));
    }

    #[tokio::test]
    async fn test_search_only_success() {
        let plugin = ConfigurableDummyPlugin::search_only_success();

        // Search should succeed
        let result = plugin.handle(Method::Search("test".to_string())).await;
        assert!(result.is_ok());

        // Cancel should fail
        let result = plugin.handle(Method::Cancel).await;
        assert!(result.is_err());

        // Quit should fail
        let result = plugin.handle(Method::Quit).await;
        assert!(result.is_err());
    }
}