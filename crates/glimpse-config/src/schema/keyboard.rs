use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Keyboard {
    /// Keep a keyboard layout for each `window`, each `app`, or everything with `global`.
    pub remember: Remember,
    /// Your layout labels, matched by code or full name without regard to case, such as `{ "English
    /// (US)" = "EN" }`. Other layouts show their code.
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Remember {
    Global,
    App,
    #[default]
    Window,
}
