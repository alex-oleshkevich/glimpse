use super::*;

#[test]
fn the_runtime_dir_is_glimpse_under_the_sessions_runtime_dir() {
    assert_eq!(
        runtime_dir_in(Some("/fixture/runtime".into())),
        Some(PathBuf::from("/fixture/runtime/glimpse"))
    );
    assert_eq!(runtime_dir_in(None), None);
    assert_eq!(runtime_dir_in(Some("".into())), None);
}

#[test]
fn an_applet_name_the_document_cannot_resolve_is_reported() {
    let [message] = unknown_applets(&panel_naming("not-an-applet"))
        .try_into()
        .expect("one name nothing resolves is one report");
    assert!(
        message.contains("not-an-applet"),
        "the message has to name the applet: {message}"
    );
    assert!(message.contains("right"));
}

#[test]
fn an_underscore_spelling_names_the_kebab_case_one_it_meant() {
    let [message] = unknown_applets(&panel_naming("next_event"))
        .try_into()
        .expect("every applet kind is kebab-case");
    assert!(
        message.contains("did you mean `next-event`?"),
        "an underscore is the typo worth spelling out: {message}"
    );
}

#[test]
fn a_kind_name_and_a_configured_instance_name_both_resolve() {
    assert!(unknown_applets(&panel_naming("next-event")).is_empty());

    let mut named = panel_naming("my-clock");
    named.applets.insert(
        "my-clock".to_owned(),
        Applet::from_name("clock").expect("clock"),
    );
    assert!(unknown_applets(&named).is_empty());
}

#[test]
fn a_document_that_fails_to_load_starts_the_defaults_and_keeps_the_error() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[panle]\n").expect("the broken document is written");

    let (config, error) = load_or_default(Some(&path));
    assert_eq!(config, Config::default());
    assert!(error.is_some_and(|error| error.to_string().contains("panle")));
}

#[test]
fn the_stack_is_each_directory_and_its_dropins_in_merge_order() {
    assert_eq!(
        watch_dirs_from(
            Path::new("/etc/glimpse"),
            Some(Path::new("/home/u/.config/glimpse")),
            None,
        ),
        [
            PathBuf::from("/etc/glimpse"),
            PathBuf::from("/etc/glimpse/config.d"),
            PathBuf::from("/home/u/.config/glimpse"),
            PathBuf::from("/home/u/.config/glimpse/config.d"),
        ]
    );
}

#[test]
fn an_explicit_file_is_watched_through_its_own_directory_alone() {
    assert_eq!(
        watch_dirs_from(
            Path::new("/etc/glimpse"),
            Some(Path::new("/home/u/.config/glimpse")),
            Some(Path::new("/tmp/nowhere/config.toml")),
        ),
        [PathBuf::from("/tmp/nowhere")]
    );
}

#[test]
fn a_symlinked_base_file_adds_the_directory_holding_its_target() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let store = root.path().join("dotfiles");
    let config = root.path().join("glimpse");
    std::fs::create_dir_all(&store).expect("creates");
    std::fs::create_dir_all(&config).expect("creates");
    std::fs::write(store.join(FILE_NAME), "").expect("writes");
    std::os::unix::fs::symlink(store.join(FILE_NAME), config.join(FILE_NAME)).expect("links");

    let dirs = watch_dirs_from(&config, None, None);

    assert_eq!(
        dirs,
        [
            config.clone(),
            config.join(DROPIN_DIR),
            store.canonicalize().expect("the target resolves"),
        ]
    );
}

#[test]
fn a_dropin_completes_a_base_table_rather_than_replacing_it() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let system = root.path().join("etc");
    let user = root.path().join("config");
    std::fs::create_dir_all(system.join(DROPIN_DIR)).expect("creates");
    std::fs::create_dir_all(&user).expect("creates");
    std::fs::write(
        system.join(FILE_NAME),
        "[geolocation]\nprovider = \"manual\"\nlatitude = 51.5074\nlongitude = -0.1278\n",
    )
    .expect("writes");
    std::fs::write(
        system.join(DROPIN_DIR).join("10-move.toml"),
        "[geolocation]\nlatitude = 52.2297\n",
    )
    .expect("writes");

    let loaded = load_from(&system, Some(&user), None).expect("the layers merge");

    assert_eq!(
        loaded.geolocation,
        crate::Geolocation::Manual {
            latitude: 52.2297,
            longitude: -0.1278
        },
        "the drop-in overrides one key and leaves the other standing"
    );
}

#[test]
fn the_screenshot_file_overrides_screenshot_keys_and_nothing_else() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let path = root.path().join(FILE_NAME);
    std::fs::write(
        &path,
        "[appearance]\ncolor-scheme = \"dark\"\n[screenshot]\nflash = true\ndelay = 2\n",
    )
    .expect("writes");
    std::fs::write(
        root.path().join(SCREENSHOT_FILE_NAME),
        "[appearance]\ncolor-scheme = \"light\"\n[screenshot]\nflash = false\n",
    )
    .expect("writes");

    let loaded = load_from(root.path(), None, Some(&path)).expect("the layers merge");

    assert!(!loaded.screenshot.flash, "shot.toml wins for its own keys");
    assert_eq!(loaded.screenshot.delay, 2, "config.toml fills the rest");
    assert_eq!(
        loaded.appearance.color_scheme,
        crate::ColorScheme::Dark,
        "any other table in shot.toml is ignored"
    );
    assert_eq!(
        stack(root.path(), None, Some(&path)).expect("a stack"),
        [path.clone(), root.path().join(SCREENSHOT_FILE_NAME)]
    );
}

#[test]
fn a_tilde_prefixed_path_expands_against_home() {
    let home = Path::new("/home/u");
    let resolved = expand_tilde(Path::new("~/wallpapers/city.jpg"), Some(home));
    assert_eq!(resolved, PathBuf::from("/home/u/wallpapers/city.jpg"));
}

#[test]
fn an_absolute_path_that_exists_is_used_as_is() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let file = dir.path().join("city.jpg");
    std::fs::write(&file, b"").expect("writes");

    let resolved = resolve_in(&file, None, None, Path::new("/nonexistent"));
    assert_eq!(resolved.as_deref(), Some(file.as_path()));
}

#[test]
fn an_absolute_path_that_is_missing_resolves_to_nothing() {
    let resolved = resolve_in(
        Path::new("/nonexistent/city.jpg"),
        None,
        None,
        Path::new("/nonexistent"),
    );
    assert_eq!(resolved, None);
}

#[test]
fn a_relative_path_prefers_the_user_root() {
    let root = tempfile::tempdir().expect("a temp dir");
    let user = root.path().join("user");
    let data = root.path().join("data");
    std::fs::create_dir_all(&user).expect("creates");
    std::fs::create_dir_all(data.join("wallpapers")).expect("creates");
    std::fs::write(user.join("city.jpg"), b"user").expect("writes");
    std::fs::write(data.join("wallpapers").join("city.jpg"), b"data").expect("writes");

    let resolved = resolve_in(Path::new("city.jpg"), None, Some(&user), &data)
        .expect("the user copy resolves");
    assert_eq!(resolved, user.join("city.jpg"));
}

#[test]
fn a_relative_path_falls_through_to_the_data_root() {
    let root = tempfile::tempdir().expect("a temp dir");
    let user = root.path().join("user");
    let data = root.path().join("data");
    std::fs::create_dir_all(&user).expect("creates");
    std::fs::create_dir_all(data.join("wallpapers")).expect("creates");
    std::fs::write(data.join("wallpapers").join("city.jpg"), b"data").expect("writes");

    let resolved = resolve_in(Path::new("city.jpg"), None, Some(&user), &data)
        .expect("the data copy resolves");
    assert_eq!(resolved, data.join("wallpapers").join("city.jpg"));
}

#[test]
fn a_relative_path_resolving_nowhere_is_a_miss() {
    let root = tempfile::tempdir().expect("a temp dir");
    let user = root.path().join("user");
    let data = root.path().join("data");
    std::fs::create_dir_all(&user).expect("creates");

    let resolved = resolve_in(Path::new("city.jpg"), None, Some(&user), &data);
    assert_eq!(resolved, None);
}
