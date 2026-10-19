use std::os::unix::fs::symlink;

use super::*;

#[test]
fn a_change_keeps_the_symlink_and_every_comment() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let dotfiles = dir.path().join("dotfiles");
    std::fs::create_dir(&dotfiles).expect("creates");
    let real = dotfiles.join("glimpse.toml");
    std::fs::write(
        &real,
        "# my settings\n[appearance]\n# keep me\ncolor-scheme = \"dark\"\n\n[screenshot]\n# a note on the flash\nflash = true\n",
    )
    .expect("writes");
    let link = dir.path().join("config.toml");
    symlink(&real, &link).expect("links");

    let pending = RefCell::default();
    stage(&pending, "flash", false);
    stage(&pending, "delay", 3i64);
    stage(&pending, "editor", "window");
    flush(&link, &pending).expect("written");

    assert!(
        std::fs::symlink_metadata(&link)
            .expect("exists")
            .file_type()
            .is_symlink(),
        "config.toml is still a symlink"
    );
    let text = std::fs::read_to_string(&real).expect("reads");
    for kept in ["# my settings", "# keep me", "# a note on the flash"] {
        assert!(text.contains(kept), "{kept} lost:\n{text}");
    }
    let config = glimpse_config::load(Some(&link)).expect("still valid");
    assert!(!config.screenshot.flash);
    assert_eq!(config.screenshot.delay, 3);
    assert_eq!(config.screenshot.editor, Editor::Window);
    assert!(pending.borrow().is_empty(), "a flush empties the queue");
}

fn widgets(root: &gtk4::Widget) -> Vec<gtk4::Widget> {
    let mut found = vec![root.clone()];
    let mut child = root.first_child();
    while let Some(widget) = child {
        found.extend(widgets(&widget));
        child = widget.next_sibling();
    }
    found
}

fn page(dialog: &impl IsA<adw::PreferencesDialog>) -> adw::PreferencesPage {
    dialog.visible_page().expect("a page")
}

fn switch_named(dialog: &impl IsA<adw::PreferencesDialog>, title: &str) -> adw::SwitchRow {
    widgets(page(dialog).upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<adw::SwitchRow>().ok())
        .find(|row| row.title() == title)
        .expect("the row")
}

fn read_only(dialog: &impl IsA<adw::PreferencesDialog>) -> bool {
    page(dialog)
        .banner()
        .is_some_and(|banner| banner.is_revealed())
}

#[test]
#[ignore = "needs a display"]
fn a_flipped_row_is_written_when_the_dialog_closes_with_every_comment_kept() {
    gtk4::init().expect("display");
    adw::init().expect("libadwaita");
    glimpse_widgets::register_resources().expect("resources");
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    let shot = dir.path().join("shot.toml");
    let original = "# mine\n[screenshot]\nflash = true\ndelay = 2\n";
    std::fs::write(&path, original).expect("writes");
    std::fs::write(&shot, "# a note on the flash\n[screenshot]\nflash = true\n").expect("writes");

    let dialog = build(Some(&path));
    assert!(!read_only(&dialog));
    let filename = dialog.fields().filename;
    assert_eq!(filename.text(), "Screenshot from %Y-%m-%d %H-%M-%S");
    filename.set_text("shot %Y");
    switch_named(&dialog, "Flash on Capture").set_active(false);
    assert_eq!(
        std::fs::read_to_string(&shot).expect("reads"),
        "# a note on the flash\n[screenshot]\nflash = true\n",
        "nothing is written before the debounce or the close"
    );
    dialog.emit_by_name::<()>("closed", &[]);

    assert_eq!(
        std::fs::read_to_string(&path).expect("reads"),
        original,
        "config.toml is never written"
    );
    let text = std::fs::read_to_string(&shot).expect("reads");
    assert!(
        text.contains("# a note on the flash"),
        "comment lost:\n{text}"
    );
    let config = glimpse_config::load(Some(&path)).expect("valid");
    assert!(!config.screenshot.flash);
    assert_eq!(
        config.screenshot.delay, 2,
        "config.toml fills what shot.toml leaves out"
    );
    assert_eq!(config.screenshot.filename.as_str(), "shot %Y.png");

    std::fs::write(&shot, "[screenshot]\ncolor = \"purple\"\n").expect("writes");
    let refused = build(Some(&path));
    assert!(read_only(&refused), "an invalid value is read-only too");
    assert!(!refused.fields().saving.is_sensitive());
    assert!(refused.fields().keyboard.is_sensitive());
    switch_named(&refused, "Show Pointer").set_active(true);
    refused.emit_by_name::<()>("closed", &[]);
    assert_eq!(
        std::fs::read_to_string(&shot).expect("reads"),
        "[screenshot]\ncolor = \"purple\"\n"
    );
}

#[test]
fn an_unparseable_file_is_read_only_and_a_missing_one_is_not() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let broken = dir.path().join("config.toml");
    std::fs::write(&broken, "[screenshot\n").expect("writes");
    assert!(!parses(&broken));
    assert!(parses(&dir.path().join("absent.toml")));

    let pending = RefCell::default();
    stage(&pending, "flash", false);
    assert!(matches!(
        flush(&broken, &pending),
        Err(ConfigError::Parse { .. })
    ));
    assert_eq!(
        std::fs::read_to_string(&broken).expect("reads"),
        "[screenshot\n"
    );
    assert!(!pending.borrow().is_empty());
    std::fs::write(&broken, "[screenshot]\n").expect("repairs");
    flush(&broken, &pending).expect("retry writes the pending change");
    assert!(pending.borrow().is_empty());
    assert!(
        !glimpse_config::load(Some(&broken))
            .expect("valid")
            .screenshot
            .flash
    );
}
