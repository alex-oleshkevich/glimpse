fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).expect("writes");
    crate::load(Some(&path))
}

#[test]
fn an_unknown_key_is_refused_naming_the_table_and_the_key() {
    let rendered = load("[ruler]\nbogus = 1\n")
        .expect_err("`bogus` is not a ruler setting")
        .to_string();

    assert!(
        rendered.contains("ruler") && rendered.contains("bogus"),
        "the error must name the table and the key, got {rendered}"
    );
}
