fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).expect("writes");
    crate::load_files(std::slice::from_ref(&path))
}

#[test]
fn an_absent_table_follows_every_player_and_fetches_art() {
    let mpris = load("").expect("an absent table is fine").mpris;

    assert!(mpris.ignore.is_empty());
    assert!(mpris.fetch_art);
    assert_eq!(mpris.art_max_kib, 4096);
}

#[test]
fn every_key_reads_kebab_case() {
    let parsed =
        load("[mpris]\nignore = ['(?i)kdeconnect']\nfetch-art = false\nart-max-kib = 512\n")
            .expect("kebab-case keys")
            .mpris;

    assert_eq!(parsed.ignore, ["(?i)kdeconnect"]);
    assert!(!parsed.fetch_art);
    assert_eq!(parsed.art_max_kib, 512);
}

#[test]
fn a_pattern_that_cannot_compile_still_loads() {
    let parsed = load("[mpris]\nignore = ['[']\n")
        .expect("a malformed pattern is not a document error")
        .mpris;

    assert_eq!(parsed.ignore, ["["]);
}
