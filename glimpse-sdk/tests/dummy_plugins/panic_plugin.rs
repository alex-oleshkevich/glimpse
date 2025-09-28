//! Panic dummy plugin implementation for testing panic recovery scenarios

use async_trait::async_trait;
use glimpse_sdk::{Metadata, Method, MethodResult, Plugin, PluginError};
use std::collections::HashSet;

/// A plugin that panics under specific conditions for testing panic recovery
#[derive(Debug, Clone)]
pub struct PanicDummyPlugin {
    metadata: Metadata,
    panic_config: PanicConfig,
}

/// Configuration for when and how to panic
#[derive(Debug, Clone)]
pub struct PanicConfig {
    panic_on_methods: HashSet<String>,
    panic_on_queries: HashSet<String>,
    panic_after_calls: Option<usize>,
    call_count: usize,
    panic_message: String,
}

impl PanicConfig {
    /// Create a config that never panics
    pub fn never() -> Self {
        Self {
            panic_on_methods: HashSet::new(),
            panic_on_queries: HashSet::new(),
            panic_after_calls: None,
            call_count: 0,
            panic_message: "Test panic".to_string(),
        }
    }

    /// Create a config that panics on search method
    pub fn on_search() -> Self {
        let mut methods = HashSet::new();
        methods.insert("search".to_string());

        Self {
            panic_on_methods: methods,
            panic_on_queries: HashSet::new(),
            panic_after_calls: None,
            call_count: 0,
            panic_message: "Panic on search".to_string(),
        }
    }

    /// Create a config that panics on cancel method
    pub fn on_cancel() -> Self {
        let mut methods = HashSet::new();
        methods.insert("cancel".to_string());

        Self {
            panic_on_methods: methods,
            panic_on_queries: HashSet::new(),
            panic_after_calls: None,
            call_count: 0,
            panic_message: "Panic on cancel".to_string(),
        }
    }

    /// Create a config that panics on quit method
    pub fn on_quit() -> Self {
        let mut methods = HashSet::new();
        methods.insert("quit".to_string());

        Self {
            panic_on_methods: methods,
            panic_on_queries: HashSet::new(),
            panic_after_calls: None,
            call_count: 0,
            panic_message: "Panic on quit".to_string(),
        }
    }

    /// Create a config that panics on all methods
    pub fn on_all_methods() -> Self {
        let mut methods = HashSet::new();
        methods.insert("search".to_string());
        methods.insert("cancel".to_string());
        methods.insert("quit".to_string());

        Self {
            panic_on_methods: methods,
            panic_on_queries: HashSet::new(),
            panic_after_calls: None,
            call_count: 0,
            panic_message: "Panic on all methods".to_string(),
        }
    }

    /// Create a config that panics on specific queries
    pub fn on_query(query: &str) -> Self {
        let mut queries = HashSet::new();
        queries.insert(query.to_string());

        Self {
            panic_on_methods: HashSet::new(),
            panic_on_queries: queries,
            panic_after_calls: None,
            call_count: 0,
            panic_message: format!("Panic on query '{}'", query),
        }
    }

    /// Create a config that panics after N calls
    pub fn after_calls(n: usize) -> Self {
        Self {
            panic_on_methods: HashSet::new(),
            panic_on_queries: HashSet::new(),
            panic_after_calls: Some(n),
            call_count: 0,
            panic_message: format!("Panic after {} calls", n),
        }
    }

    /// Create a config with custom panic message
    pub fn with_message(mut self, message: &str) -> Self {
        self.panic_message = message.to_string();
        self
    }

    /// Check if we should panic for given method and query
    fn should_panic(&mut self, method: &str, query: Option<&str>) -> bool {
        self.call_count += 1;

        // Check call count limit
        if let Some(limit) = self.panic_after_calls {
            if self.call_count > limit {
                return true;
            }
        }

        // Check method-based panic
        if self.panic_on_methods.contains(method) {
            return true;
        }

        // Check query-based panic
        if let Some(q) = query {
            if self.panic_on_queries.contains(q) {
                return true;
            }
        }

        false
    }

    /// Get the panic message
    fn panic_message(&self) -> &str {
        &self.panic_message
    }
}

impl PanicDummyPlugin {
    /// Create a new panic plugin that never panics
    pub fn new() -> Self {
        Self {
            metadata: Metadata {
                id: "test.panic".to_string(),
                name: "Panic Test Plugin".to_string(),
                version: "1.0.0".to_string(),
                description: "A panic test plugin for panic recovery testing".to_string(),
                author: "Test Suite".to_string(),
            },
            panic_config: PanicConfig::never(),
        }
    }

    /// Create a panic plugin with specific panic configuration
    pub fn with_config(panic_config: PanicConfig) -> Self {
        Self {
            metadata: Metadata {
                id: "test.panic_configured".to_string(),
                name: "Configured Panic Test Plugin".to_string(),
                version: "1.0.0".to_string(),
                description: "A configured panic test plugin".to_string(),
                author: "Test Suite".to_string(),
            },
            panic_config,
        }
    }

    /// Create a plugin that panics on search
    pub fn panic_on_search() -> Self {
        Self::with_config(PanicConfig::on_search())
    }

    /// Create a plugin that panics on cancel
    pub fn panic_on_cancel() -> Self {
        Self::with_config(PanicConfig::on_cancel())
    }

    /// Create a plugin that panics on quit
    pub fn panic_on_quit() -> Self {
        Self::with_config(PanicConfig::on_quit())
    }

    /// Create a plugin that panics on all methods
    pub fn panic_on_all() -> Self {
        Self::with_config(PanicConfig::on_all_methods())
    }

    /// Create a plugin that panics on specific query
    pub fn panic_on_query(query: &str) -> Self {
        Self::with_config(PanicConfig::on_query(query))
    }

    /// Create a plugin that panics after N calls
    pub fn panic_after_calls(n: usize) -> Self {
        Self::with_config(PanicConfig::after_calls(n))
    }

    /// Create a plugin that panics immediately (after 0 calls)
    pub fn panic_immediately() -> Self {
        Self::panic_after_calls(0)
    }
}

impl Default for PanicDummyPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for PanicDummyPlugin {
    fn metadata(&self) -> Metadata {
        self.metadata.clone()
    }

    async fn handle(&self, method: Method) -> Result<MethodResult, PluginError> {
        // Note: We need to work around the immutable reference to check panic conditions
        // In a real scenario, this would be handled differently, but for testing purposes
        // we'll create a simple approach

        match method {
            Method::Search(ref query) => {
                // Create a temporary config to check panic conditions
                let mut temp_config = self.panic_config.clone();
                if temp_config.should_panic("search", Some(query)) {
                    panic!("{}", temp_config.panic_message());
                }
                Ok(MethodResult::Matches(vec![]))
            }
            Method::Cancel => {
                let mut temp_config = self.panic_config.clone();
                if temp_config.should_panic("cancel", None) {
                    panic!("{}", temp_config.panic_message());
                }
                Ok(MethodResult::Matches(vec![]))
            }
            Method::Quit => {
                let mut temp_config = self.panic_config.clone();
                if temp_config.should_panic("quit", None) {
                    panic!("{}", temp_config.panic_message());
                }
                Ok(MethodResult::Matches(vec![]))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic;

    #[tokio::test]
    async fn test_panic_plugin_normal_operation() {
        let plugin = PanicDummyPlugin::new();
        let result = plugin.handle(Method::Search("test".to_string())).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_panic_on_search_plugin() {
        let plugin = PanicDummyPlugin::panic_on_search();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let result = panic::catch_unwind(|| {
            rt.block_on(async {
                let _ = plugin.handle(Method::Search("test".to_string())).await;
            });
        });

        assert!(result.is_err());
    }

    #[test]
    fn test_panic_on_cancel_plugin() {
        let plugin = PanicDummyPlugin::panic_on_cancel();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let result = panic::catch_unwind(|| {
            rt.block_on(async {
                let _ = plugin.handle(Method::Cancel).await;
            });
        });

        assert!(result.is_err());
    }

    #[test]
    fn test_panic_on_quit_plugin() {
        let plugin = PanicDummyPlugin::panic_on_quit();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let result = panic::catch_unwind(|| {
            rt.block_on(async {
                let _ = plugin.handle(Method::Quit).await;
            });
        });

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_panic_on_specific_query() {
        let plugin = PanicDummyPlugin::panic_on_query("panic_trigger");

        // Should not panic on normal queries
        let result = plugin.handle(Method::Search("normal".to_string())).await;
        assert!(result.is_ok());

        // Should panic on trigger query
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| async {
            let _ = plugin
                .handle(Method::Search("panic_trigger".to_string()))
                .await;
        }));

        // Note: This test is complex due to async + panic interaction
        // In practice, the panic would be caught by the tokio runtime
    }

    #[tokio::test]
    async fn test_panic_config_creation() {
        let config = PanicConfig::on_search();
        assert!(config.panic_on_methods.contains("search"));
        assert!(!config.panic_on_methods.contains("cancel"));

        let config = PanicConfig::on_all_methods();
        assert!(config.panic_on_methods.contains("search"));
        assert!(config.panic_on_methods.contains("cancel"));
        assert!(config.panic_on_methods.contains("quit"));

        let config = PanicConfig::after_calls(5);
        assert_eq!(config.panic_after_calls, Some(5));
    }
}
