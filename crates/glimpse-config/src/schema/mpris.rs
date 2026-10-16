use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Choose which media players you follow and whether to download their artwork.
/// Playing players take priority over paused, then stopped players, with the most recently active first.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Mpris {
    /// Hide media players whose names match these case-sensitive regular expressions. Upper and lower case must match the name; start an entry with `(?i)` to
    /// ignore case, as in `'(?i)firefox'`. Use single quotes around each entry.
    pub ignore: Vec<String>,
    /// Download album artwork from the internet. Artwork from local files still works when this is
    /// off.
    pub fetch_art: bool,
    /// The largest artwork download you allow, in kibibytes, from 16 to 65536.
    pub art_max_kib: u32,
}

impl Default for Mpris {
    fn default() -> Self {
        Self {
            ignore: Vec::new(),
            fetch_art: true,
            art_max_kib: 4096,
        }
    }
}

#[cfg(test)]
mod tests;
