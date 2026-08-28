use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Wallpaper {
    pub fit: Fit,
    pub color: String,
    pub background: String,
    pub background_dark: String,
}

impl Default for Wallpaper {
    fn default() -> Self {
        Self {
            fit: Fit::Fill,
            color: "#000000".to_owned(),
            background: String::new(),
            background_dark: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Fit {
    Cover,
    Contain,
    Fill,
}
