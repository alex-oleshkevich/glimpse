use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Backdrop {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub blur_radius: u32,
}

impl Default for Backdrop {
    fn default() -> Self {
        Self {
            enabled: true,
            path: None,
            blur_radius: 24,
        }
    }
}
