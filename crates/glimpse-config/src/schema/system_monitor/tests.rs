fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).expect("writes");
    crate::load(Some(&path))
}

#[test]
fn an_absent_table_samples_root_every_two_seconds_with_gpu_on() {
    let system_monitor = load("").expect("an absent table is fine").system_monitor;

    assert_eq!(system_monitor.poll_interval, 2);
    assert_eq!(system_monitor.disk_paths, vec!["/".to_owned()]);
    assert!(system_monitor.gpu);
}

#[test]
fn a_poll_interval_outside_one_to_sixty_is_refused() {
    for value in [0, 61] {
        let rendered = load(&format!("[system-monitor]\npoll-interval = {value}\n"))
            .expect_err("out of range")
            .to_string();
        assert!(
            rendered.contains("poll-interval"),
            "the error must name `poll-interval`, got {rendered}"
        );
    }
}

#[test]
fn a_relative_disk_path_is_refused() {
    let rendered = load("[system-monitor]\ndisk-paths = [\"home\"]\n")
        .expect_err("relative path")
        .to_string();
    assert!(
        rendered.contains("home"),
        "the error must name the offending path, got {rendered}"
    );
}

#[test]
fn duplicate_disk_paths_collapse_to_one_after_normalizing_a_trailing_slash() {
    let system_monitor = load("[system-monitor]\ndisk-paths = [\"/data\", \"/data/\"]\n")
        .expect("both are absolute")
        .system_monitor;

    assert_eq!(system_monitor.disk_paths, vec!["/data".to_owned()]);
}

#[test]
fn an_all_slash_path_normalizes_to_root_rather_than_an_empty_string() {
    let system_monitor = load("[system-monitor]\ndisk-paths = [\"//\", \"///\"]\n")
        .expect("both are absolute")
        .system_monitor;

    assert_eq!(system_monitor.disk_paths, vec!["/".to_owned()]);
}

#[test]
fn a_setting_this_table_does_not_have_is_refused() {
    let rendered = load("[system-monitor]\ninterval = 5\n")
        .expect_err("`interval` is not a key of this table")
        .to_string();
    assert!(
        rendered.contains("interval"),
        "the error must name `interval`, got {rendered}"
    );
}
