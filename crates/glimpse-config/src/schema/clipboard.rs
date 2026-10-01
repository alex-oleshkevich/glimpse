use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What the panel remembers of the clipboard. The history lives in the panel process and in memory
/// only: it survives nothing, which is deliberate for a buffer that routinely holds passwords.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Clipboard {
    /// Whether the clipboard is watched at all. Off means no compositor connection is opened and
    /// nothing is recorded; the applet then renders as if nothing had ever been copied.
    pub enabled: bool,
    /// How many entries are kept. Pinned entries do not count against it and are never evicted, so
    /// a history full of pins holds more than this many. Clamped to 1..=10000.
    pub limit: usize,
    /// The largest single entry that will be captured, in kibibytes. Another application chooses
    /// what it puts on the clipboard, so this is what stops one handing over something enormous.
    /// Clamped to 4..=2048: the compositor read is capped at 2 MiB, and a larger value here would
    /// promise a size that is discarded before it ever reaches the history.
    pub max_entry_kib: u32,
    /// The total the history may occupy, in kibibytes. Reached, the oldest unpinned entry is
    /// dropped. This bounds memory in a way `limit` alone cannot, because one image can outweigh a
    /// hundred lines of text. Clamped to 64..=1048576.
    pub max_total_kib: u32,
    /// Whether images are captured as well as text. Off records text only; an image copied while
    /// off is not recorded at all rather than recorded without its content.
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
