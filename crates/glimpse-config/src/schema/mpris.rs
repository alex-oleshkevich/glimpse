use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which media players the panel service follows, and what it does with their artwork. There are no
/// players here: every `org.mpris.MediaPlayer2.*` on the session bus is followed unless `ignore`
/// says otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Mpris {
    /// Media players to keep off the bar. Write part of the player's name, as it appears in the
    /// popover: `ignore = ['Firefox', 'chromium']` hides Firefox and every Chromium window
    /// playing sound. Upper and lower case must match the name; start an entry with `(?i)` to
    /// ignore case, as in `'(?i)firefox'`. Use single quotes around each entry.
    pub ignore: Vec<String>,
    /// Whether artwork named by an `http` or `https` URL is downloaded. Players that name a local
    /// file are unaffected, and turning this off leaves those working. Downloads go to
    /// `$XDG_RUNTIME_DIR/glimpse/art/` and are capped by `art-max-kib`.
    pub fetch_art: bool,
    /// The largest artwork that will be downloaded, in kibibytes. A player chooses this URL, so the
    /// cap is what stops one pointing the service at something enormous. It bounds downloads only;
    /// artwork a player names as a local file is bounded by its pixel dimensions instead, when it is
    /// decoded. Clamped to 16..=65536.
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
