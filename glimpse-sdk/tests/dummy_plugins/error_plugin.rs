//! Error dummy plugin implementation for testing all error scenarios

use async_trait::async_trait;
use glimpse_sdk::{Metadata, Method, MethodResult, Plugin, PluginError};
use std::collections::HashMap;

/// A plugin that returns various types of errors on demand
#[derive(Debug, Clone)]
pub struct ErrorDummyPlugin {
    metadata: Metadata,
    error_config: ErrorConfig,
}

/// Configuration for what errors to return for different methods
#[derive(Debug, Clone)]
pub struct ErrorConfig {
    search_error: Option<PluginError>,
    cancel_error: Option<PluginError>,
    quit_error: Option<PluginError>,
    error_counter: HashMap<String, usize>,
}

impl ErrorConfig {
    /// Create a config that always succeeds (no errors)
    pub fn success() -> Self {
        Self {
            search_error: None,
            cancel_error: None,
            quit_error: None,
            error_counter: HashMap::new(),
        }
    }

    /// Create a config that returns authentication errors
    pub fn auth_error() -> Self {
        Self {
            search_error: Some(PluginError::Authenticate(
                "Authentication failed".to_string(),
            )),
            cancel_error: None,
            quit_error: None,
            error_counter: HashMap::new(),
        }
    }

    /// Create a config that returns IO errors
    pub fn io_error() -> Self {
        Self {
            search_error: Some(PluginError::Io(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "Connection refused",
            ))),
            cancel_error: None,
            quit_error: None,
            error_counter: HashMap::new(),
        }
    }

    /// Create a config that returns JSON errors
    pub fn json_error() -> Self {
        Self {
            search_error: Some(PluginError::Json(
                serde_json::from_str::<()>("invalid json").unwrap_err()
            )),
            cancel_error: None,
            quit_error: None,
            error_counter: HashMap::new(),
        }
    }

    /// Create a config that returns cancelled errors
    pub fn cancelled_error() -> Self {
        Self {
            search_error: Some(PluginError::Cancelled("Operation was cancelled".to_string())),
            cancel_error: Some(PluginError::Cancelled("Cancel was cancelled".to_string())),
            quit_error: None,
            error_counter: HashMap::new(),
        }
    }

    /// Create a config that returns generic errors
    pub fn generic_error() -> Self {
        Self {
            search_error: Some(PluginError::Other("Something went wrong".to_string())),
            cancel_error: Some(PluginError::Other("Cancel failed".to_string())),
            quit_error: Some(PluginError::Other("Quit failed".to_string())),
            error_counter: HashMap::new(),
        }
    }

    /// Create a config that returns different errors for different methods
    pub fn mixed_errors() -> Self {
        Self {
            search_error: Some(PluginError::Authenticate("Auth error".to_string())),
            cancel_error: Some(PluginError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Broken pipe",
            ))),
            quit_error: Some(PluginError::Other("Generic quit error".to_string())),
            error_counter: HashMap::new(),
        }
    }

    /// Create a config that errors only on the nth call
    pub fn error_on_nth_call(method: &str, n: usize, error: PluginError) -> Self {
        let mut config = Self::success();
        match method {
            "search" => config.search_error = Some(error),
            "cancel" => config.cancel_error = Some(error),
            "quit" => config.quit_error = Some(error),
            _ => {}
        }
        config.error_counter.insert(format!("{}_count", method), 0);
        config.error_counter.insert(format!("{}_error_at", method), n);
        config
    }
}

impl ErrorDummyPlugin {
    /// Create a new error plugin with success configuration
    pub fn new() -> Self {
        Self {
            metadata: Metadata {
                id: "test.error".to_string(),
                name: "Error Test Plugin".to_string(),
                version: "1.0.0".to_string(),
                description: "An error test plugin for error handling testing".to_string(),
                author: "Test Suite".to_string(),
            },
            error_config: ErrorConfig::success(),
        }
    }

    /// Create an error plugin with specific error configuration
    pub fn with_config(error_config: ErrorConfig) -> Self {
        Self {
            metadata: Metadata {
                id: "test.error_configured".to_string(),
                name: "Configured Error Test Plugin".to_string(),
                version: "1.0.0".to_string(),
                description: "A configured error test plugin".to_string(),
                author: "Test Suite".to_string(),
            },
            error_config,
        }
    }

    /// Create an error plugin that always fails authentication
    pub fn auth_failure() -> Self {
        Self::with_config(ErrorConfig::auth_error())
    }

    /// Create an error plugin that always fails with IO errors
    pub fn io_failure() -> Self {
        Self::with_config(ErrorConfig::io_error())
    }

    /// Create an error plugin that always fails with JSON errors
    pub fn json_failure() -> Self {
        Self::with_config(ErrorConfig::json_error())
    }

    /// Create an error plugin that always fails with cancelled errors
    pub fn cancelled_failure() -> Self {
        Self::with_config(ErrorConfig::cancelled_error())
    }

    /// Create an error plugin that always fails with generic errors
    pub fn generic_failure() -> Self {
        Self::with_config(ErrorConfig::generic_error())
    }

    /// Create an error plugin with mixed error types
    pub fn mixed_failure() -> Self {
        Self::with_config(ErrorConfig::mixed_errors())
    }
}

impl Default for ErrorDummyPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for ErrorDummyPlugin {
    fn metadata(&self) -> Metadata {
        self.metadata.clone()
    }

    async fn handle(&self, method: Method) -> Result<MethodResult, PluginError> {
        match method {
            Method::Search(_) => {
                if let Some(error) = &self.error_config.search_error {
                    Err(error.clone())
                } else {
                    Ok(MethodResult::SearchResults(vec![]))
                }
            }
            Method::Cancel => {
                if let Some(error) = &self.error_config.cancel_error {
                    Err(error.clone())
                } else {
                    Ok(MethodResult::SearchResults(vec![]))
                }
            }
            Method::Quit => {
                if let Some(error) = &self.error_config.quit_error {
                    Err(error.clone())
                } else {
                    Ok(MethodResult::SearchResults(vec![]))
                }
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_error_plugin_success() {
        let plugin = ErrorDummyPlugin::new();
        let result = plugin.handle(Method::Search("test".to_string())).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_auth_failure_plugin() {
        let plugin = ErrorDummyPlugin::auth_failure();
        let result = plugin.handle(Method::Search("test".to_string())).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::Authenticate(_) => {} // Expected
            other => panic!("Expected Authenticate error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_io_failure_plugin() {
        let plugin = ErrorDummyPlugin::io_failure();
        let result = plugin.handle(Method::Search("test".to_string())).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::Io(_) => {} // Expected
            other => panic!("Expected IO error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_json_failure_plugin() {
        let plugin = ErrorDummyPlugin::json_failure();
        let result = plugin.handle(Method::Search("test".to_string())).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::Json(_) => {} // Expected
            other => panic!("Expected JSON error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_cancelled_failure_plugin() {
        let plugin = ErrorDummyPlugin::cancelled_failure();
        let result = plugin.handle(Method::Search("test".to_string())).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::Cancelled(_) => {} // Expected
            other => panic!("Expected Cancelled error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_generic_failure_plugin() {
        let plugin = ErrorDummyPlugin::generic_failure();
        let result = plugin.handle(Method::Search("test".to_string())).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::Other(_) => {} // Expected
            other => panic!("Expected Other error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_mixed_failure_plugin() {
        let plugin = ErrorDummyPlugin::mixed_failure();

        // Test search error (should be auth error)
        let result = plugin.handle(Method::Search("test".to_string())).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::Authenticate(_)));

        // Test cancel error (should be IO error)
        let result = plugin.handle(Method::Cancel).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::Io(_)));

        // Test quit error (should be generic error)
        let result = plugin.handle(Method::Quit).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::Other(_)));
    }
}