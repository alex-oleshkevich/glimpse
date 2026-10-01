use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const FILE: &str = "language-codes.json";

/// The same table that ships to `/usr/share/glimpse`, compiled in so a build that has never been
/// installed still names layouts — the two can therefore never disagree.
const BUILT_IN: &str = include_str!("../../../data/language-codes.json");

type Codes = BTreeMap<String, String>;

/// The short badge a panel shows for a layout. Compositors name layouts inconsistently — niri
/// reports xkb descriptions ("English (US)"), Hyprland reports xkb codes ("us") — so both shapes
/// have to reduce to the same two letters.
pub fn layout_code(layout: &str) -> String {
    let first_word = layout.split_whitespace().next().unwrap_or(layout);

    if let Some(code) = table().get(&first_word.to_lowercase()) {
        return code.clone();
    }

    // A bare xkb code carries no spaces and is already the short form, variants included: "de_ch"
    // has to stay distinguishable from "de".
    if !layout.contains(' ') {
        return layout.to_uppercase();
    }

    first_word
        .chars()
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

/// Read once, on the first layout that needs naming. The read is blocking and deliberately so: it
/// happens a single time per process, and threading an async load through every caller would cost
/// more than the one file open it saves.
fn table() -> &'static Codes {
    static TABLE: OnceLock<Codes> = OnceLock::new();
    TABLE.get_or_init(|| load(&search_paths()))
}

/// The user's copy wins over the installed one, so a layout this table names badly can be corrected
/// without touching the package.
fn search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(dir) = glimpse_config::user_dir() {
        paths.push(dir.join(FILE));
    }
    paths.push(Path::new(glimpse_config::DATA_DIR).join(FILE));
    paths
}

fn load(paths: &[PathBuf]) -> Codes {
    for path in paths {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "could not read the language codes");
                continue;
            }
        };

        match parse(&text) {
            Ok(codes) => {
                tracing::debug!(path = %path.display(), entries = codes.len(), "read language codes");
                return codes;
            }
            // A hand-edited file with a typo in it falls back rather than leaving every layout
            // unnamed, and says where to look.
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "ignored malformed language codes");
            }
        }
    }

    parse(BUILT_IN).unwrap_or_default()
}

fn parse(text: &str) -> Result<Codes, serde_json::Error> {
    let codes: Codes = serde_json::from_str(text)?;

    Ok(codes
        .into_iter()
        .map(|(language, code)| (language.to_lowercase(), code))
        .collect())
}

#[cfg(test)]
mod tests;
