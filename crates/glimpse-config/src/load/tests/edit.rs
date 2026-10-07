use super::*;

#[test]
fn a_symlinked_commented_file_keeps_everything_but_the_key() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let store = root.path().join("dotfiles");
    let config = root.path().join("glimpse");
    std::fs::create_dir_all(&store).expect("creates");
    std::fs::create_dir_all(&config).expect("creates");
    std::fs::write(store.join(FILE_NAME), DOTFILE).expect("writes");
    let link = config.join(FILE_NAME);
    std::os::unix::fs::symlink(store.join(FILE_NAME), &link).expect("links");

    write_table(&link, "screenshot", &flash_off()).expect("writes");

    assert!(
        std::fs::symlink_metadata(&link)
            .expect("the link")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        std::fs::read_to_string(store.join(FILE_NAME)).expect("reads"),
        DOTFILE.replace("\"flash\" = true # blink", "\"flash\" = false # blink")
    );
    assert_eq!(
        std::fs::read_dir(&store).expect("lists").count(),
        1,
        "no temporary file is left behind"
    );
}

#[test]
fn a_change_between_read_and_write_is_kept_and_the_edit_reapplied() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join(FILE_NAME);
    std::fs::write(&path, DOTFILE).expect("writes");
    let concurrent = format!("{DOTFILE}\n[ruler]\nhistory-limit = 12\n");
    let mut edits = 0;

    write_table_with(&path, "screenshot", &flash_off(), &mut || {
        if edits == 0 {
            std::fs::write(&path, &concurrent).expect("writes");
        }
        edits += 1;
    })
    .expect("writes");

    assert_eq!(edits, 2, "the edit ran once more on the new content");
    assert_eq!(
        std::fs::read_dir(dir.path()).expect("lists").count(),
        1,
        "the abandoned temporary file is removed"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("reads"),
        concurrent.replace("\"flash\" = true # blink", "\"flash\" = false # blink")
    );
}

#[test]
fn a_file_that_never_settles_is_not_written() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join(FILE_NAME);
    std::fs::write(&path, DOTFILE).expect("writes");
    let mut churn = String::from(DOTFILE);

    write_table_with(&path, "screenshot", &flash_off(), &mut || {
        churn.push('\n');
        std::fs::write(&path, &churn).expect("writes");
    })
    .expect_err("three attempts, then a refusal");

    assert!(
        std::fs::read_to_string(&path)
            .expect("reads")
            .contains("\"flash\" = true")
    );
}

#[test]
fn an_unparseable_file_is_refused_and_left_untouched() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join(FILE_NAME);
    let broken = b"[screenshot\nflash = true\n";
    std::fs::write(&path, broken).expect("writes");

    let error = write_table(&path, "screenshot", &flash_off()).expect_err("refused");

    assert!(matches!(error, ConfigError::Parse { .. }), "{error}");
    assert_eq!(std::fs::read(&path).expect("reads"), broken);
}

#[test]
fn a_crlf_file_keeps_its_line_endings() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join(FILE_NAME);
    std::fs::write(&path, "# mine\r\n[appearance]\r\ntheme = \"mine\"\r\n").expect("writes");

    write_table(&path, "screenshot", &flash_off()).expect("writes");

    assert_eq!(
        std::fs::read_to_string(&path).expect("reads"),
        "# mine\r\n[appearance]\r\ntheme = \"mine\"\r\n\r\n[screenshot]\r\nflash = false\r\n"
    );
}

#[test]
fn a_dangling_symlink_is_refused_and_left_a_symlink() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let link = dir.path().join(FILE_NAME);
    std::os::unix::fs::symlink(dir.path().join("gone.toml"), &link).expect("links");

    let error = write_table(&link, "screenshot", &flash_off()).expect_err("refused");

    assert!(matches!(error, ConfigError::Unwritable { .. }), "{error}");
    assert!(
        std::fs::symlink_metadata(&link)
            .expect("the link")
            .file_type()
            .is_symlink()
    );
    assert!(!dir.path().join("gone.toml").exists());
}

#[test]
fn a_read_only_file_is_refused_and_left_untouched() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join(FILE_NAME);
    std::fs::write(&path, "[screenshot]\nflash = true\n").expect("writes");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).expect("chmod");

    let error = write_table(&path, "screenshot", &flash_off()).expect_err("refused");

    assert!(matches!(error, ConfigError::Unwritable { .. }), "{error}");
    assert_eq!(
        std::fs::read_to_string(&path).expect("reads"),
        "[screenshot]\nflash = true\n"
    );
    assert_eq!(std::fs::read_dir(dir.path()).expect("lists").count(), 1);
}

#[test]
fn a_table_where_a_value_belongs_is_refused() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join(FILE_NAME);
    std::fs::write(&path, "screenshot = { flash = true }\n").expect("writes");
    let mut values = toml_edit::Table::new();
    values.insert("flash", toml_edit::table());

    let error = write_table(&path, "screenshot", &values).expect_err("refused");

    assert!(matches!(error, ConfigError::Schema { .. }), "{error}");
    assert_eq!(
        std::fs::read_to_string(&path).expect("reads"),
        "screenshot = { flash = true }\n"
    );
}

#[test]
fn a_missing_file_is_created_holding_only_the_table() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("glimpse").join(FILE_NAME);

    write_table(&path, "screenshot", &flash_off()).expect("writes");

    assert_eq!(
        std::fs::read_to_string(&path).expect("reads"),
        "[screenshot]\nflash = false\n"
    );
}
