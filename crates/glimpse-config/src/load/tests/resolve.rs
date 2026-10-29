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

fn roots_of(base: &Path) -> [PathBuf; 3] {
    [base.join("dir"), base.join("user"), base.join("etc")]
}

fn loaded(roots: &[PathBuf], documents: Documents) -> Result<Config, ConfigError> {
    load_files(&files_in(roots, documents)?)
}

#[test]
fn a_document_that_fails_to_load_starts_the_defaults_and_keeps_the_error() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    write(&path, "[panle]\n");

    let (config, error) = or_default(load_files(&[path]));
    assert_eq!(config, Config::default());
    assert!(error.is_some_and(|error| error.to_string().contains("panle")));
}

#[test]
fn retired_keys_are_dropped_and_the_rest_of_the_document_still_loads() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    write(
        &path,
        "[regional]\nlanguage = \"ru\"\nunits = \"kelvin\"\n\n[idle]\nenabled = false\n\n\
         [lock.clock]\nenabled = false\ntime-format = \"%I\"\ndate-format = \"%d\"\n\n\
         [lock.status]\nenabled = false\n\n[lock.media]\nenabled = false\n\n\
         [lock.session]\nenabled = false\nactions = [\"suspend\"]\n\n\
         [lock.notifications]\nenabled = false\nprivacy = \"count\"\n\n\
         [lock]\npam-service = \"login\"\n",
    );

    let config = load_files(&[path]).expect("an old document still loads");
    assert!(!config.idle.enabled);
    assert_eq!(config.lock.clock.date_format, "%d");
}

#[test]
fn the_files_run_from_the_lowest_root_up_and_the_panel_follows_the_config() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let roots = roots_of(base.path());
    let [d, u, e] = &roots;
    write(&e.join("panel.d/20-b.toml"), "");
    write(&e.join("panel.d/10-a.toml"), "");
    write(&u.join("panel.d/10.toml"), "");
    write(&d.join("panel.d/notes.txt"), "");

    let config_files = [
        e.join("config.toml"),
        u.join("config.toml"),
        d.join("config.toml"),
    ];
    let panel_files = [
        e.join("panel.toml"),
        e.join("panel.d/10-a.toml"),
        e.join("panel.d/20-b.toml"),
        u.join("panel.toml"),
        u.join("panel.d/10.toml"),
        d.join("panel.toml"),
    ];

    assert_eq!(
        files_in(&roots, Documents::Config).expect("lists"),
        config_files
    );
    assert_eq!(
        files_in(&roots, Documents::ConfigAndPanel).expect("lists"),
        [config_files.to_vec(), panel_files.to_vec()].concat()
    );
}

#[test]
fn panels_written_only_in_a_user_panel_file_are_the_panels() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let roots = roots_of(base.path());
    write(
        &roots[1].join("panel.toml"),
        "[[panels]]\nright = [\"clock\"]\n",
    );

    let config = loaded(&roots, Documents::ConfigAndPanel).expect("loads");

    assert_eq!(config.panels.len(), 1);
    assert_eq!(config.panels[0].right, ["clock"]);
}

#[test]
fn a_broken_panel_file_does_not_touch_the_config_document() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let roots = roots_of(base.path());
    write(&roots[1].join("panel.toml"), "not toml [[[");
    write(
        &roots[1].join("config.toml"),
        "[night-light]\ntemperature = 3000\n",
    );

    let config = loaded(&roots, Documents::Config).expect("the panel file is never read");

    assert_eq!(config.night_light.temperature, 3000);
    assert_eq!(config.panels, Config::default().panels);
    assert!(loaded(&roots, Documents::ConfigAndPanel).is_err());
}

#[test]
fn a_table_in_the_wrong_file_names_the_file_it_belongs_in() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let roots = roots_of(base.path());
    let dropin = roots[1].join("panel.d/10.toml");
    write(&dropin, "[lock]\n");

    assert_eq!(
        loaded(&roots, Documents::ConfigAndPanel)
            .expect_err("a lock table in a drop-in")
            .to_string(),
        format!("{}: [lock] belongs in config.toml", dropin.display())
    );
    fs::remove_file(&dropin).expect("removes");

    let config = roots[1].join("config.toml");
    write(&config, "[[panels]]\n");
    for documents in [Documents::Config, Documents::ConfigAndPanel] {
        assert_eq!(
            loaded(&roots, documents)
                .expect_err("a panel table in the config file")
                .to_string(),
            format!("{}: [[panels]] belongs in panel.toml", config.display())
        );
    }
}

#[test]
fn an_unknown_table_in_a_panel_file_is_not_sent_to_the_config_file() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let roots = roots_of(base.path());
    write(&roots[1].join("panel.toml"), "[[panel]]\n");

    let rendered = loaded(&roots, Documents::ConfigAndPanel)
        .expect_err("a misspelled panels table")
        .to_string();

    assert!(rendered.contains("unknown field `panel`"), "{rendered}");
    assert!(!rendered.contains("belongs in"), "{rendered}");
}

#[test]
fn a_schema_error_names_the_file_it_came_from() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let roots = roots_of(base.path());
    let config = roots[1].join("config.toml");
    write(&config, "[clock]\nformat = 1\n");

    let rendered = loaded(&roots, Documents::Config)
        .expect_err("an unknown table")
        .to_string();

    assert!(
        rendered.starts_with(&format!("{}: ", config.display())),
        "{rendered}"
    );
}

#[test]
fn ownership_follows_the_file_name_of_a_file_given_directly() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let panel = base.path().join("panel.toml");
    let other = base.path().join("anything.toml");
    write(&panel, "[applets]\n");
    write(&other, "[lock]\n");

    assert!(load_files(&[panel, other]).is_ok());
    let stray = base.path().join("stray.toml");
    write(&stray, "[applets]\n");
    assert!(load_files(&[stray]).is_err());
}

#[test]
fn the_highest_root_wins_a_key_and_the_lower_roots_keep_the_rest() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let roots = roots_of(base.path());
    let [d, u, _] = &roots;
    write(
        &u.join("config.toml"),
        "[night-light]\ntemperature = 4000\nschedule = \"off\"\n",
    );
    write(
        &d.join("config.toml"),
        "[night-light]\ntemperature = 3000\n",
    );

    let config = loaded(&roots, Documents::Config).expect("loads");

    assert_eq!(config.night_light.temperature, 3000);
    assert_eq!(config.night_light.schedule, crate::Schedule::Off);
}

#[test]
fn a_dropin_completes_a_base_table_rather_than_replacing_it() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let roots = roots_of(base.path());
    let etc = &roots[2];
    write(
        &etc.join("panel.toml"),
        "[applets.clock]\nlabel-format = \"%H\"\ntooltip-format = \"%A\"\n",
    );
    write(
        &etc.join("panel.d/10-move.toml"),
        "[applets.clock]\nlabel-format = \"%H:%M\"\n",
    );

    let config = loaded(&roots, Documents::ConfigAndPanel).expect("the layers merge");

    let crate::AppletKind::Clock(clock) = &config.applets["clock"].kind else {
        panic!("the table names the clock");
    };
    assert_eq!(clock.label_format, "%H:%M");
    assert_eq!(
        config.applets["clock"].kind.tooltip_format(),
        Some("%A"),
        "the drop-in overrides one key and leaves the other standing"
    );
}

#[test]
fn a_missing_file_is_skipped_without_an_error() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let loaded = load_files(&[base.path().join("typo.toml")]).expect("a missing file is skipped");

    assert_eq!(loaded, Config::default());
}

#[test]
fn the_writable_file_is_the_config_dir_else_the_user_dir_else_nothing() {
    let user = Path::new("/home/u/.config/glimpse");
    let dir = Path::new("/tmp/dir");

    assert_eq!(
        writable_in(None, Some(user), "config"),
        Some(user.join("config.toml"))
    );
    assert_eq!(
        writable_in(Some(dir), Some(user), "panel"),
        Some(dir.join("panel.toml"))
    );
    assert_eq!(writable_in(None, None, "config"), None);
}

#[test]
fn the_roots_are_computed_afresh_so_a_late_init_is_seen() {
    let cell = OnceLock::new();
    let user = Path::new("/home/u/.config/glimpse");
    let system = Path::new("/etc/glimpse");
    let dir = PathBuf::from("/tmp/dir");

    assert_eq!(
        roots_in(stored(&cell), Some(user), system),
        [user.to_path_buf(), system.to_path_buf()]
    );
    cell.set(Some(dir.clone())).expect("the first init");
    assert_eq!(
        roots_in(stored(&cell), Some(user), system),
        [dir, user.to_path_buf(), system.to_path_buf()]
    );
}

#[test]
fn a_root_named_twice_is_listed_once() {
    let user = Path::new("/home/u/.config/glimpse");

    assert_eq!(
        roots_in(Some(user), Some(user), Path::new("/etc/glimpse")),
        [user.to_path_buf(), PathBuf::from("/etc/glimpse")]
    );
}

#[test]
fn a_file_only_the_installed_data_has_is_found_and_a_root_copy_wins() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let roots = roots_of(base.path());
    let data = base.path().join("data");
    write(&data.join("themes/x/panel.css"), "data");

    assert_eq!(
        find_in(&roots, &data, Path::new("themes/x")),
        Some(data.join("themes/x"))
    );
    write(&roots[2].join("themes/x/panel.css"), "etc");
    assert_eq!(
        find_in(&roots, &data, Path::new("themes/x")),
        Some(roots[2].join("themes/x"))
    );
    write(&roots[0].join("themes/x/panel.css"), "dir");
    assert_eq!(
        find_in(&roots, &data, Path::new("themes/x")),
        Some(roots[0].join("themes/x"))
    );
    assert_eq!(find_in(&roots, &data, Path::new("themes/none")), None);
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
    write(&file, "");

    let resolved = resolve_in(&file, None, &[], Path::new("/nonexistent"));
    assert_eq!(resolved.as_deref(), Some(file.as_path()));
}

#[test]
fn an_absolute_path_that_is_missing_resolves_to_nothing() {
    let resolved = resolve_in(
        Path::new("/nonexistent/city.jpg"),
        None,
        &[],
        Path::new("/nonexistent"),
    );
    assert_eq!(resolved, None);
}

#[test]
fn a_relative_path_prefers_the_highest_root() {
    let base = tempfile::tempdir().expect("a temp dir");
    let roots = roots_of(base.path());
    let data = base.path().join("data");
    write(&roots[1].join("city.jpg"), "user");
    write(&roots[2].join("city.jpg"), "etc");
    write(&data.join("wallpapers/city.jpg"), "data");

    let resolved =
        resolve_in(Path::new("city.jpg"), None, &roots, &data).expect("the user copy resolves");
    assert_eq!(resolved, roots[1].join("city.jpg"));
}

#[test]
fn a_relative_path_falls_through_to_a_roots_wallpapers_then_the_data_root() {
    let base = tempfile::tempdir().expect("a temp dir");
    let roots = roots_of(base.path());
    let data = base.path().join("data");
    write(&data.join("wallpapers/city.jpg"), "data");
    assert_eq!(
        resolve_in(Path::new("city.jpg"), None, &roots, &data),
        Some(data.join("wallpapers/city.jpg"))
    );

    write(&roots[2].join("wallpapers/city.jpg"), "etc");
    assert_eq!(
        resolve_in(Path::new("city.jpg"), None, &roots, &data),
        Some(roots[2].join("wallpapers/city.jpg"))
    );
}

#[test]
fn a_relative_path_resolving_nowhere_is_a_miss() {
    let base = tempfile::tempdir().expect("a temp dir");
    let roots = roots_of(base.path());

    let resolved = resolve_in(
        Path::new("city.jpg"),
        None,
        &roots,
        &base.path().join("data"),
    );
    assert_eq!(resolved, None);
}

#[test]
fn a_directory_of_the_same_name_in_a_higher_root_does_not_hide_the_file_below_it() {
    let base = tempfile::tempdir().expect("a temp dir");
    let roots = roots_of(base.path());
    let data = base.path().join("data");
    std::fs::create_dir_all(roots[1].join("city.jpg")).expect("creates");
    write(&roots[2].join("city.jpg"), "etc");

    assert_eq!(
        resolve_in(Path::new("city.jpg"), None, &roots, &data),
        Some(roots[2].join("city.jpg"))
    );
}

#[test]
fn an_empty_config_dir_is_the_same_as_none() {
    init(Some(PathBuf::new()));

    assert_eq!(config_dir(), None);
}
