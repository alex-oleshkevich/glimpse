use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Keyboard {
    /// Where a layout choice sticks: `window` gives each window its own layout, `app` shares one
    /// per application, and `global` keeps one layout for everything.
    pub remember: Remember,
    /// The text the panel shows for a layout, keyed by the layout's code or full name, matched
    /// without regard to case: `{ "English (US)" = "EN", ua = "УКР" }`. Unlisted layouts show
    /// their code.
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
