//! Basic dummy plugin implementation for simple success scenarios

use async_trait::async_trait;
use glimpse_sdk::{Action, Metadata, Method, MethodResult, Plugin, PluginError, Match};

/// A simple plugin that always succeeds with predictable responses
#[derive(Debug, Clone)]
pub struct BasicDummyPlugin {
    metadata: Metadata,
}

impl BasicDummyPlugin {
    /// Create a new basic dummy plugin with default metadata
    pub fn new() -> Self {
        Self {
            metadata: Metadata {
                id: "test.basic".to_string(),
                name: "Basic Test Plugin".to_string(),
                version: "1.0.0".to_string(),
                description: "A basic test plugin for unit testing".to_string(),
                author: "Test Suite".to_string(),
            },
        }
    }

    /// Create a basic plugin with custom metadata
    pub fn with_metadata(metadata: Metadata) -> Self {
        Self { metadata }
    }

    /// Create search results for testing
    fn create_search_results(query: &str) -> Vec<Match> {
        vec![
            Match {
                title: format!("Result 1 for '{}'", query),
                subtitle: Some("Basic search result".to_string()),
                icon: Some("test-icon.png".to_string()),
                actions: vec![
                    Action::Clipboard {
                        text: query.to_string(),
                    },
                    Action::ShellExec {
                        command: "echo".to_string(),
                        args: vec![query.to_string()],
                    },
                ],
                score: 1.0,
            },
            Match {
                title: format!("Result 2 for '{}'", query),
                subtitle: None,
                icon: None,
                actions: vec![Action::OpenPath {
                    path: "/tmp/test".to_string(),
                }],
                score: 0.8,
            },
        ]
    }
}

impl Default for BasicDummyPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for BasicDummyPlugin {
    fn metadata(&self) -> Metadata {
        self.metadata.clone()
    }

    async fn handle(&self, method: Method) -> Result<MethodResult, PluginError> {
        match method {
            Method::Search(query) => {
                let results = Self::create_search_results(&query);
                Ok(MethodResult::Matches(results))
            }
            Method::Cancel => {
                // Cancel method typically doesn't return anything in this context
                // but we need to return something for testing
                Ok(MethodResult::Matches(vec![]))
            }
            Method::Quit => {
                // Quit method typically doesn't return anything in this context
                // but we need to return something for testing
                Ok(MethodResult::Matches(vec![]))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_plugin_metadata() {
        let plugin = BasicDummyPlugin::new();
        let metadata = plugin.metadata();

        assert_eq!(metadata.id, "test.basic");
        assert_eq!(metadata.name, "Basic Test Plugin");
        assert_eq!(metadata.version, "1.0.0");
    }

    #[tokio::test]
    async fn test_basic_plugin_search() {
        let plugin = BasicDummyPlugin::new();
        let result = plugin
            .handle(Method::Search("test query".to_string()))
            .await;

        assert!(result.is_ok());
        match result.unwrap() {
            MethodResult::Matches(results) => {
                assert_eq!(results.len(), 2);
                assert_eq!(results[0].title, "Result 1 for 'test query'");
                assert_eq!(results[1].title, "Result 2 for 'test query'");
            }
            _ => panic!("Expected SearchResults"),
        }
    }

    #[tokio::test]
    async fn test_basic_plugin_cancel() {
        let plugin = BasicDummyPlugin::new();
        let result = plugin.handle(Method::Cancel).await;

        assert!(result.is_ok());
        match result.unwrap() {
            MethodResult::Matches(results) => {
                assert!(results.is_empty());
            }
            _ => panic!("Expected SearchResults"),
        }
    }

    #[tokio::test]
    async fn test_basic_plugin_quit() {
        let plugin = BasicDummyPlugin::new();
        let result = plugin.handle(Method::Quit).await;

        assert!(result.is_ok());
        match result.unwrap() {
            MethodResult::Matches(results) => {
                assert!(results.is_empty());
            }
            _ => panic!("Expected SearchResults"),
        }
    }
}
