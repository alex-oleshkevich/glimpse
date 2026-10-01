use super::*;

fn pixmap(size: i32) -> TrayPixmap {
    TrayPixmap {
        width: size,
        height: size,
        argb: vec![0; (size * size * 4) as usize],
    }
}

fn nothing_exists(_: &Path) -> bool {
    false
}

fn everything_exists(_: &Path) -> bool {
    true
}

#[test]
fn an_absolute_path_that_is_really_there_wins_outright() {
    assert_eq!(
        choose(
            Some("/run/user/1000/tray-icon/x.png"),
            None,
            &[],
            22,
            &everything_exists
        ),
        Source::File(PathBuf::from("/run/user/1000/tray-icon/x.png"))
    );
}

#[test]
fn an_absolute_path_that_is_gone_drops_to_pixels_rather_than_to_the_icon_theme() {
    assert_eq!(
        choose(Some("/gone/x.png"), None, &[], 22, &nothing_exists),
        Source::Missing,
        "the theme has no entry called /gone/x.png and would draw its own broken-image glyph"
    );
    assert_eq!(
        choose(
            Some("/gone/x.png"),
            None,
            &[pixmap(22)],
            22,
            &nothing_exists
        ),
        Source::Pixmap(0),
        "an application that sends both keeps working when its file is rotated away"
    );
}

#[test]
fn a_themed_name_containing_a_slash_is_not_mistaken_for_a_path() {
    assert_eq!(
        choose(Some("app/status"), None, &[], 22, &nothing_exists),
        Source::Themed("app/status".to_owned()),
        "only an *absolute* path is a path"
    );
}

#[test]
fn the_items_own_theme_directory_is_probed_as_a_literal_file_first() {
    let exists =
        |path: &Path| path == Path::new("/theme") || path == Path::new("/theme/tray-icon.svg");
    assert_eq!(
        choose(Some("tray-icon"), Some("/theme"), &[], 22, &exists),
        Source::File(PathBuf::from("/theme/tray-icon.svg")),
        "an application shipping tray-icon.svg beside its binary means that file"
    );
}

#[test]
fn a_theme_directory_that_does_not_exist_is_skipped_not_probed() {
    assert_eq!(
        choose(
            Some("tray-icon"),
            Some("/app/share/icons"),
            &[],
            22,
            &nothing_exists
        ),
        Source::Themed("tray-icon".to_owned()),
        "the Flatpak case: real inside the sandbox, absent out here"
    );
}

#[test]
fn pixels_are_used_only_when_there_is_no_name_at_all() {
    let sizes = [pixmap(16), pixmap(32)];
    assert_eq!(
        choose(None, None, &sizes, 22, &nothing_exists),
        Source::Pixmap(1),
        "22 does not fit in 16, so the 32 is chosen"
    );
    assert_eq!(
        choose(Some(""), None, &sizes, 22, &nothing_exists),
        Source::Pixmap(1),
        "an empty name is no name; Slack sends exactly this"
    );
}

#[test]
fn a_scaled_bar_picks_a_different_pixmap_for_the_same_slot() {
    let sizes = [pixmap(16), pixmap(24), pixmap(48)];
    assert_eq!(
        choose(None, None, &sizes, 24, &nothing_exists),
        Source::Pixmap(1)
    );
    assert_eq!(
        choose(None, None, &sizes, 48, &nothing_exists),
        Source::Pixmap(2),
        "scale 2 of a 24px slot must not reuse the 24px pixmap"
    );
}

#[test]
fn an_item_with_neither_a_name_nor_pixels_gets_the_missing_icon() {
    assert_eq!(
        choose(None, None, &[], 22, &nothing_exists),
        Source::Missing,
        "a blank chip is worse than one that says the icon is gone"
    );
}
