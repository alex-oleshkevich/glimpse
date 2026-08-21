use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Appearance {
    pub pack: String,
    pub scheme: Scheme,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            pack: String::new(),
            scheme: Scheme::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Scheme {
    Light,
    Dark,
    Auto,
}
