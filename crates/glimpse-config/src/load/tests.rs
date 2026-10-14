use super::*;

fn shipped(dir: &Path) -> PathBuf {
    let source = dir.join(COMMENTED_FILE_NAME);
    std::fs::write(&source, crate::commented_document()).expect("writes");
    source
}

fn flash_off() -> toml_edit::Table {
    let mut values = toml_edit::Table::new();
    values.insert("flash", toml_edit::value(false));
    values
}

const DOTFILE: &str = "# my dotfiles\n[appearance]\ntheme = \"mine\" # keep\n\n\
                           [screenshot]\nnotify = false\n# blink on capture\n\"flash\" = true # blink\n\
                           delay = 3\n";

fn panel_naming(name: &str) -> Config {
    Config {
        panels: vec![crate::schema::Panel {
            right: vec![name.to_owned()],
            ..crate::schema::Panel::default()
        }],
        ..Config::default()
    }
}

mod edit;
mod resolve;
mod seed;

#[test]
fn semantic_problems_are_collected_after_merging_without_rejecting_the_document() {
    let root = tempfile::tempdir().expect("temporary directory");
    let system = root.path().join("system");
    let user = root.path().join("user");
    fs::create_dir_all(&system).expect("creates");
    fs::create_dir_all(&user).expect("creates");
    fs::write(
        system.join(FILE_NAME),
        "[night-light]\nstart-time = \"07:00\"\n",
    )
    .expect("writes");
    fs::write(user.join(FILE_NAME), "[night-light]\nend-time = \"7:00\"\n[[idle.profiles.ac.listeners]]\ntimeout = 10\non-idle = \"first\"\n[[idle.profiles.ac.listeners]]\ntimeout = 10\non-idle = \"   \"\n[[panels]]\nleft = [\"missing-applet\"]\n").expect("writes");
    let config = load_from(&system, Some(&user), None).expect("semantic warnings keep loading");
    let problems: Vec<String> = config.validate().iter().map(ToString::to_string).collect();
    for key in [
        "night-light.start-time",
        "listeners[1].timeout",
        "listeners[1].on-idle",
        "missing-applet",
    ] {
        assert!(
            problems.iter().any(|problem| problem.contains(key)),
            "{key}: {problems:?}"
        );
    }
    assert_eq!(problems.len(), 4);
}

#[test]
fn an_out_of_range_night_light_temperature_is_clamped_without_dropping_the_document() {
    let root = tempfile::tempdir().expect("temporary directory");
    fs::write(
        root.path().join(FILE_NAME),
        "[night-light]\ntemperature = 9000\nschedule = \"off\"\n[screenshot]\nflash = false\n",
    )
    .expect("writes");
    let config = load_from(root.path(), None, None).expect("an out-of-range temperature loads");
    assert_eq!(config.night_light.temperature, crate::MAX_TEMPERATURE);
    assert_eq!(config.night_light.schedule, crate::Schedule::Off);
    assert!(!config.screenshot.flash);
}
