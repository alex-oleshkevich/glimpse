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
