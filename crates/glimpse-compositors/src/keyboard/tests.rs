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
fn a_missing_file_falls_back_to_the_built_in_table() {
    let dir = tempfile::tempdir().expect("a temporary directory");

    let codes = load(&[dir.path().join("absent.json")]);

    assert_eq!(codes.get("english").map(String::as_str), Some("EN"));
}

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
