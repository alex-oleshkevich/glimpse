use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which media players the panel service follows, and what it does with their artwork. There are no
/// players here: every `org.mpris.MediaPlayer2.*` on the session bus is followed unless `ignore`
/// says otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Mpris {
    /// Regular expressions matched against a player's bus-name suffix (`spotify`,
    /// `kdeconnect.mpris_desktop_1`) and its `Identity` (`Spotify`, `Firefox`). A player matching
    /// any of them is never shown. Patterns are unanchored and case-sensitive; write `(?i)` for
    /// case-insensitive and `^…$` to anchor. A pattern that does not compile is skipped with a
    /// warning and the rest keep working. `playerctld` and KDE Connect mirrors are already
    /// handled without this.
    ///
    /// To start from: `'^chromium'` hides every Chromium tab, whose bus names carry an instance
    /// number after the application (`chromium.instance4181`); `'(?i)firefox'` hides Firefox
    /// however it happens to capitalize its `Identity`; `'^mpv$'` hides exactly `mpv` and leaves
    /// `mpv-shim` alone. Write them as TOML literal strings, in single quotes, so a backslash
    /// reaches the regex instead of being read as a string escape:
    /// `ignore = ['^chromium', '(?i)firefox']`.
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
