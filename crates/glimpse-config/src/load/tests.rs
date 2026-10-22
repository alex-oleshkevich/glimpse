use super::*;

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

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().expect("a parent directory")).expect("creates");
    fs::write(path, text).expect("writes");
}

mod edit;
mod resolve;
mod seed;

#[test]
fn semantic_problems_are_collected_after_merging_without_rejecting_the_document() {
    let root = tempfile::tempdir().expect("temporary directory");
    let roots = [root.path().join("user"), root.path().join("system")];
    write(
        &roots[1].join("config.toml"),
        "[night-light]\nstart-time = \"07:00\"\n",
    );
    write(
        &roots[0].join("config.toml"),
        "[night-light]\nend-time = \"7:00\"\n[[idle.profiles.ac.listeners]]\ntimeout = 10\non-idle = \"first\"\n[[idle.profiles.ac.listeners]]\ntimeout = 10\non-idle = \"   \"\n",
    );
    write(
        &roots[0].join("panel.toml"),
        "[[panels]]\nleft = [\"missing-applet\"]\n",
    );
    let files = files_in(&roots, Documents::ConfigAndPanel).expect("lists");
    let config = load_files(&files).expect("semantic warnings keep loading");
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
    let path = root.path().join("config.toml");
    write(
        &path,
        "[night-light]\ntemperature = 9000\nschedule = \"off\"\n[screenshot]\nflash = false\n",
    );
    let config = load_files(&[path]).expect("an out-of-range temperature loads");
    assert_eq!(config.night_light.temperature, crate::MAX_TEMPERATURE);
    assert_eq!(config.night_light.schedule, crate::Schedule::Off);
    assert!(!config.screenshot.flash);
}
