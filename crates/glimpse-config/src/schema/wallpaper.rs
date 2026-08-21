use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Wallpaper {
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub fit: Fit,
    pub transition_ms: u32,
}

impl Default for Wallpaper {
    fn default() -> Self {
        Self {
            color: "#101010".to_owned(),
            path: None,
            fit: Fit::Cover,
            transition_ms: 800,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    Cover,
    Contain,
    Fill,
}
