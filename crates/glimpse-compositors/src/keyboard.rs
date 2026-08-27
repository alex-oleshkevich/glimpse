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
mod tests {
    use std::io::Write;

    use super::*;

    fn file(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join(FILE);
        let mut handle = std::fs::File::create(&path).expect("create the table");
        handle.write_all(body.as_bytes()).expect("write the table");
        path
    }

    #[test]
    fn maps_known_language_names() {
        assert_eq!(layout_code("English (US)"), "EN");
        assert_eq!(layout_code("Russian"), "RU");
        assert_eq!(layout_code("Polish"), "PL");
        assert_eq!(layout_code("Georgian"), "GE");
    }

    #[test]
    fn passes_raw_xkb_codes_through_uppercased() {
        assert_eq!(layout_code("us"), "US");
        assert_eq!(layout_code("ru"), "RU");
        assert_eq!(layout_code("de_ch"), "DE_CH");
    }

    #[test]
    fn falls_back_to_two_letters_of_an_unknown_multiword_name() {
        assert_eq!(layout_code("Klingon (pIqaD)"), "KL");
    }

    #[test]
    fn an_empty_name_does_not_panic() {
        assert_eq!(layout_code(""), "");
    }

    /// The shipped file and the compiled-in copy are the same bytes, so this is really a check that
    /// `include_str!` still points at the file the packaging installs.
    #[test]
    fn the_built_in_table_parses_and_covers_the_common_layouts() {
        let built_in = parse(BUILT_IN).expect("the shipped table is valid JSON");

        assert_eq!(built_in.get("english").map(String::as_str), Some("EN"));
        assert_eq!(built_in.get("ukrainian").map(String::as_str), Some("UA"));
        assert!(built_in.len() > 30, "got {} entries", built_in.len());
    }

    #[test]
    fn a_file_on_disk_replaces_the_built_in_table() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = file(dir.path(), r#"{ "english": "ENG", "klingon": "TLH" }"#);

        let codes = load(&[path]);

        assert_eq!(codes.get("english").map(String::as_str), Some("ENG"));
        assert_eq!(codes.get("klingon").map(String::as_str), Some("TLH"));
        assert_eq!(codes.get("russian"), None, "the file replaces, not extends");
    }

    /// The whole point of the search order: a user's copy is consulted before the installed one.
    #[test]
    fn the_user_copy_wins_over_the_installed_one() {
        let user = tempfile::tempdir().expect("a temporary directory");
        let shared = tempfile::tempdir().expect("a temporary directory");
        let user = file(user.path(), r#"{ "english": "MINE" }"#);
        let shared = file(shared.path(), r#"{ "english": "SHIPPED" }"#);

        assert_eq!(
            load(&[user.clone(), shared.clone()])
                .get("english")
                .map(String::as_str),
            Some("MINE")
        );
        assert_eq!(
            load(&[shared]).get("english").map(String::as_str),
            Some("SHIPPED")
        );
    }

    #[test]
    fn a_missing_file_falls_through_to_the_next_path() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let installed = file(dir.path(), r#"{ "english": "SHIPPED" }"#);

        let codes = load(&[dir.path().join("absent.json"), installed]);

        assert_eq!(codes.get("english").map(String::as_str), Some("SHIPPED"));
    }

    /// A typo in a hand-edited file must not leave every layout unnamed.
    #[test]
    fn a_malformed_file_falls_back_to_the_built_in_table() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let broken = file(dir.path(), "{ not json");

        let codes = load(&[broken]);

        assert_eq!(codes.get("english").map(String::as_str), Some("EN"));
    }

    #[test]
    fn keys_are_matched_regardless_of_the_case_they_were_written_in() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = file(dir.path(), r#"{ "ENGLISH": "EN", "Russian": "RU" }"#);

        let codes = load(&[path]);

        assert_eq!(codes.get("english").map(String::as_str), Some("EN"));
        assert_eq!(codes.get("russian").map(String::as_str), Some("RU"));
    }

    #[test]
    fn the_search_order_puts_the_config_directory_before_the_shared_one() {
        let paths = search_paths();

        assert!(
            paths.last().expect("a shared path") == &Path::new(glimpse_config::DATA_DIR).join(FILE),
            "the installed copy must be the last resort, got {paths:?}"
        );
        if let Some(dir) = glimpse_config::user_dir() {
            assert_eq!(paths.first(), Some(&dir.join(FILE)));
        }
    }
}
