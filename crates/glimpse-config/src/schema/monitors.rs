use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Monitors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builtin_connector: Option<String>,
}
