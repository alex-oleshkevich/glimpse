use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Keep your copied text and PNG or JPEG images in memory until the panel restarts.
/// Content marked secret by a password manager is skipped; copying an existing entry moves it to the top.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Clipboard {
    /// Keep a history of what you copy. Turning this off clears your history, including pinned
    /// entries.
    pub enabled: bool,
    /// How many recent clipboard entries you keep. Pinned entries don't count.
    pub limit: usize,
    /// The largest clipboard entry you keep, in kibibytes, from 4 to 2048.
    pub max_entry_kib: u32,
    /// How much memory your clipboard history can use, in kibibytes. When it fills up, the oldest
    /// unpinned entries are dropped.
    pub max_total_kib: u32,
    /// Keep copied images in your clipboard history alongside text.
    pub capture_images: bool,
}

impl Default for Clipboard {
    fn default() -> Self {
        Self {
            enabled: true,
            limit: 100,
            max_entry_kib: 2048,
            max_total_kib: 10240,
            capture_images: true,
        }
    }
}

#[cfg(test)]
mod tests;
