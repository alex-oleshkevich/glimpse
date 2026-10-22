fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).expect("writes");
    crate::load_files(std::slice::from_ref(&path))
}

#[test]
fn an_absent_table_watches_the_clipboard_and_keeps_images() {
    let clipboard = load("").expect("an absent table is fine").clipboard;

    assert!(clipboard.enabled);
    assert_eq!(clipboard.limit, 100);
    assert_eq!(clipboard.max_entry_kib, 2048);
    assert_eq!(clipboard.max_total_kib, 10240);
    assert!(clipboard.capture_images);
}

#[test]
fn every_key_reads_kebab_case() {
    let parsed = load(
        "[clipboard]\nenabled = false\nlimit = 5\nmax-entry-kib = 64\nmax-total-kib = 512\ncapture-images = false\n",
    )
    .expect("kebab-case keys")
    .clipboard;

    assert!(!parsed.enabled);
    assert_eq!(parsed.limit, 5);
    assert_eq!(parsed.max_entry_kib, 64);
    assert_eq!(parsed.max_total_kib, 512);
    assert!(!parsed.capture_images);
}

#[test]
fn a_setting_this_table_does_not_have_is_refused() {
    let rendered = load("[clipboard]\npersist = true\n")
        .expect_err("`persist` is not a key of this table")
        .to_string();

    assert!(
        rendered.contains("persist"),
        "the error must name `persist`, got {rendered}"
    );
}
