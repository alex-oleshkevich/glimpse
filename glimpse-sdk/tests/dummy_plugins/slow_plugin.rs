//! Slow dummy plugin implementation for testing timeouts and cancellation

use async_trait::async_trait;
use glimpse_sdk::{Metadata, Method, MethodResult, Plugin, PluginError, SearchItem};
use std::time::Duration;
use tokio::time::sleep;

/// A plugin that introduces delays to simulate slow operations
#[derive(Debug, Clone)]
pub struct SlowDummyPlugin {
    metadata: Metadata,
    search_delay: Duration,
    cancel_delay: Duration,
    quit_delay: Duration,
}

impl SlowDummyPlugin {
    /// Create a new slow plugin with default delays
    pub fn new() -> Self {
        Self {
            metadata: Metadata {
                id: "test.slow".to_string(),
                name: "Slow Test Plugin".to_string(),
                version: "1.0.0".to_string(),
                description: "A slow test plugin for timeout and cancellation testing".to_string(),
                author: "Test Suite".to_string(),
            },
            search_delay: Duration::from_millis(100),
            cancel_delay: Duration::from_millis(50),
            quit_delay: Duration::from_millis(25),
        }
    }

    /// Create a slow plugin with custom delays
    pub fn with_delays(
        search_delay: Duration,
        cancel_delay: Duration,
        quit_delay: Duration,
    ) -> Self {
        Self {
            metadata: Metadata {
                id: "test.slow_custom".to_string(),
                name: "Custom Slow Test Plugin".to_string(),
                version: "1.0.0".to_string(),
                description: "A customizable slow test plugin".to_string(),
                author: "Test Suite".to_string(),
            },
            search_delay,
            cancel_delay,
            quit_delay,
        }
    }

    /// Create a very slow plugin (for cancellation testing)
    pub fn very_slow() -> Self {
        Self::with_delays(
            Duration::from_secs(10), // Long enough to cancel
            Duration::from_secs(5),
            Duration::from_secs(2),
        )
    }

    /// Create an extremely slow plugin (for timeout testing)
    pub fn extremely_slow() -> Self {
        Self::with_delays(
            Duration::from_secs(60), // Very long for timeout tests
            Duration::from_secs(30),
            Duration::from_secs(10),
        )
    }

    /// Create search results after delay
    async fn create_delayed_search_results(&self, query: &str) -> Vec<SearchItem> {
        sleep(self.search_delay).await;
        vec![SearchItem {
            title: format!("Slow result for '{}'", query),
            subtitle: Some(format!("Delayed by {:?}", self.search_delay)),
            icon: Some("slow-icon.png".to_string()),
            actions: vec![],
            score: 0.9,
        }]
    }
}

impl Default for SlowDummyPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for SlowDummyPlugin {
    fn metadata(&self) -> Metadata {
        self.metadata.clone()
    }

    async fn handle(&self, method: Method) -> Result<MethodResult, PluginError> {
        match method {
            Method::Search(query) => {
                let results = self.create_delayed_search_results(&query).await;
                Ok(MethodResult::SearchResults(results))
            }
            Method::Cancel => {
                sleep(self.cancel_delay).await;
                Ok(MethodResult::SearchResults(vec![]))
            }
            Method::Quit => {
                sleep(self.quit_delay).await;
                Ok(MethodResult::SearchResults(vec![]))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test]
    async fn test_slow_plugin_search_timing() {
        let plugin = SlowDummyPlugin::with_delays(
            Duration::from_millis(50),
            Duration::from_millis(25),
            Duration::from_millis(10),
        );

        let start = Instant::now();
        let result = plugin.handle(Method::Search("test".to_string())).await;
        let elapsed = start.elapsed();

        assert!(result.is_ok());
        assert!(elapsed >= Duration::from_millis(45)); // Account for timing variations
        assert!(elapsed < Duration::from_millis(100)); // Should not take too long
    }

    #[tokio::test]
    async fn test_very_slow_plugin_creation() {
        let plugin = SlowDummyPlugin::very_slow();
        let metadata = plugin.metadata();

        assert_eq!(metadata.id, "test.slow_custom");
        assert!(plugin.search_delay >= Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_extremely_slow_plugin_creation() {
        let plugin = SlowDummyPlugin::extremely_slow();
        assert!(plugin.search_delay >= Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_cancel_method_delay() {
        let plugin = SlowDummyPlugin::with_delays(
            Duration::from_millis(10),
            Duration::from_millis(30),
            Duration::from_millis(5),
        );

        let start = Instant::now();
        let result = plugin.handle(Method::Cancel).await;
        let elapsed = start.elapsed();

        assert!(result.is_ok());
        assert!(elapsed >= Duration::from_millis(25)); // Account for timing variations
    }

    #[tokio::test]
    async fn test_quit_method_delay() {
        let plugin = SlowDummyPlugin::with_delays(
            Duration::from_millis(10),
            Duration::from_millis(15),
            Duration::from_millis(20),
        );

        let start = Instant::now();
        let result = plugin.handle(Method::Quit).await;
        let elapsed = start.elapsed();

        assert!(result.is_ok());
        assert!(elapsed >= Duration::from_millis(15)); // Account for timing variations
    }
}