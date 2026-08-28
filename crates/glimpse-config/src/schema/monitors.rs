use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Monitors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builtin_connector: Option<String>,
}
