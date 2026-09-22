use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Places {
    /// Whether the trash is watched and listed. Turns off one persistent inotify watch.
    pub trash: bool,
    /// Whether network shares are watched and listed. Turns off one directory read.
    pub network: bool,
}

impl Default for Places {
    fn default() -> Self {
        Self {
            trash: true,
            network: true,
        }
    }
}
