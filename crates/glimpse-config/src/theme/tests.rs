use super::*;

fn roots(base: &Path) -> Vec<PathBuf> {
    vec![base.join("user"), base.join("data")]
}

fn sheet(base: &Path, root: &str, theme: &str, name: &str) {
    let dir = base.join(root).join(theme);
    std::fs::create_dir_all(&dir).expect("creates");
    std::fs::write(dir.join(name), "").expect("writes");
}

#[test]
fn the_user_copy_of_a_theme_wins_over_the_installed_one() {
    let base = tempfile::tempdir().expect("a temporary directory");
    sheet(base.path(), "user", "adwaita", "panel.css");
    sheet(base.path(), "data", "adwaita", "panel.css");

    assert_eq!(
        stylesheet_in(&roots(base.path()), "adwaita", "panel.css"),
        Some(base.path().join("user/adwaita/panel.css"))
    );
}

#[test]
fn a_theme_that_exists_answers_alone_and_does_not_borrow_from_another_root() {
    let base = tempfile::tempdir().expect("a temporary directory");
    sheet(base.path(), "user", "nord", "panel.css");
    sheet(base.path(), "data", "nord", "lock.css");
    sheet(base.path(), "data", "adwaita", "lock.css");

    assert_eq!(
        stylesheet_in(&roots(base.path()), "nord", "panel.css"),
        Some(base.path().join("user/nord/panel.css"))
    );
    assert_eq!(
        stylesheet_in(&roots(base.path()), "nord", "lock.css"),
        None,
        "the user's nord is the theme; a relative @import could not reach another root anyway"
    );
}

#[test]
fn a_theme_with_no_directory_anywhere_falls_back_to_the_default_theme() {
    let base = tempfile::tempdir().expect("a temporary directory");
    sheet(base.path(), "data", "adwaita", "panel.css");
    sheet(base.path(), "data", "adwaita", "base.css");

    assert_eq!(
        theme_dir_in(&roots(base.path()), "nord"),
        Some(base.path().join("data/adwaita")),
        "the whole directory moves together, so its own base.css comes with it"
    );
    assert_eq!(
        stylesheet_in(&roots(base.path()), "nord", "panel.css"),
        Some(base.path().join("data/adwaita/panel.css"))
    );
}

#[test]
fn the_dark_sheet_comes_from_the_resolved_directory_only() {
    let base = tempfile::tempdir().expect("a temporary directory");
    sheet(base.path(), "user", "nord", "panel.css");
    sheet(base.path(), "data", "nord", "dark.css");

    assert_eq!(
        stylesheet_in(&roots(base.path()), "nord", "dark.css"),
        None,
        "the directory is the unit, so a dark sheet cannot arrive from the other root"
    );
}

#[test]
fn the_users_dark_sheet_is_found_beside_their_light_one() {
    let base = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(base.path().join(STYLES_FILE), "").expect("writes");
    std::fs::write(base.path().join(DARK_STYLESHEET), "").expect("writes");

    assert_eq!(
        user_sheet_in(base.path(), STYLES_FILE),
        Some(base.path().join(STYLES_FILE))
    );
    assert_eq!(
        user_sheet_in(base.path(), DARK_STYLESHEET),
        Some(base.path().join(DARK_STYLESHEET))
    );
}

#[test]
fn a_dark_sheet_with_no_light_sheet_still_loads() {
    let base = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(base.path().join(DARK_STYLESHEET), "").expect("writes");

    assert_eq!(user_sheet_in(base.path(), STYLES_FILE), None);
    assert_eq!(
        user_sheet_in(base.path(), DARK_STYLESHEET),
        Some(base.path().join(DARK_STYLESHEET)),
        "a user who only tweaks dark never writes styles.css"
    );
}

#[test]
fn a_sheet_the_resolved_theme_does_not_supply_resolves_to_nothing() {
    let base = tempfile::tempdir().expect("a temporary directory");
    sheet(base.path(), "data", "adwaita", "panel.css");

    assert_eq!(
        stylesheet_in(&roots(base.path()), "nord", "wallpaper.css"),
        None
    );
}

#[test]
fn an_unnamed_theme_falls_straight_through_to_the_default_one() {
    let base = tempfile::tempdir().expect("a temporary directory");
    sheet(base.path(), "data", "adwaita", "panel.css");

    assert_eq!(
        stylesheet_in(&roots(base.path()), "", "panel.css"),
        Some(base.path().join("data/adwaita/panel.css"))
    );
}

#[test]
fn a_directory_named_like_a_sheet_is_not_a_sheet() {
    let base = tempfile::tempdir().expect("a temporary directory");
    std::fs::create_dir_all(base.path().join("user/adwaita/panel.css")).expect("creates");

    assert_eq!(
        stylesheet_in(&roots(base.path()), "adwaita", "panel.css"),
        None
    );
}

#[test]
fn a_file_named_like_a_theme_is_not_a_theme() {
    let base = tempfile::tempdir().expect("a temporary directory");
    std::fs::create_dir_all(base.path().join("user")).expect("creates");
    std::fs::write(base.path().join("user/nord"), "").expect("writes");
    sheet(base.path(), "data", "nord", "panel.css");

    assert_eq!(
        theme_dir_in(&roots(base.path()), "nord"),
        Some(base.path().join("data/nord"))
    );
}

#[test]
fn the_set_is_every_root_the_theme_could_arrive_in_plus_the_one_it_did() {
    let base = tempfile::tempdir().expect("a temporary directory");
    sheet(base.path(), "data", "nord", "panel.css");
    let user = base.path().join("home");

    assert_eq!(
        watch_dirs_in(&roots(base.path()), Some(&user), "nord"),
        [
            user,
            base.path().join("user"),
            base.path().join("user/nord"),
            base.path().join("data"),
            base.path().join("data/nord"),
        ],
        "the resolved directory is already in the set, so it is not repeated"
    );
}

#[test]
fn the_resolved_default_theme_joins_the_set_when_the_named_one_is_absent() {
    let base = tempfile::tempdir().expect("a temporary directory");
    sheet(base.path(), "data", "adwaita", "panel.css");

    assert_eq!(
        watch_dirs_in(&roots(base.path()), None, "nord"),
        [
            base.path().join("user"),
            base.path().join("user/nord"),
            base.path().join("data"),
            base.path().join("data/nord"),
            base.path().join("data/adwaita"),
        ],
        "edits to the theme actually in use have to be seen"
    );
}

#[test]
fn an_unnamed_theme_watches_the_roots_alone_rather_than_joining_an_empty_name() {
    let base = tempfile::tempdir().expect("a temporary directory");

    assert_eq!(
        watch_dirs_in(&roots(base.path()), None, ""),
        [base.path().join("user"), base.path().join("data")]
    );
}

/// The root has to be a temporary one, not `/usr/share/glimpse/themes`: `theme_dir_in` asks
/// the filesystem whether the default theme is there, so a hardcoded system path asserts that
/// glimpse is not installed on the machine running the test.
#[test]
fn a_platform_naming_no_config_directory_still_watches_the_installed_root() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let root = base.path().join("themes");
    std::fs::create_dir_all(&root).expect("an installed theme root");

    assert_eq!(
        watch_dirs_in(std::slice::from_ref(&root), None, ""),
        std::slice::from_ref(&root),
        "with no theme directory under it there is only the root to watch"
    );

    std::fs::create_dir_all(root.join(DEFAULT_THEME)).expect("the default theme");
    assert_eq!(
        watch_dirs_in(std::slice::from_ref(&root), None, ""),
        [root.clone(), root.join(DEFAULT_THEME)],
        "an installed default theme is watched too, which is what an installed package has"
    );
}

#[tokio::test]
async fn sighup_forces_a_theme_reload() {
    use std::time::Duration;

    use futures_util::StreamExt;
    use tokio::signal::unix::{SignalKind, signal};

    let _installed = signal(SignalKind::hangup()).expect("registers a handler");
    let mut updates = Box::pin(watch_theme("adwaita"));
    let status = tokio::process::Command::new("kill")
        .args(["-HUP", &std::process::id().to_string()])
        .status()
        .await
        .expect("kill runs");
    assert!(status.success(), "kill did not signal this process");

    tokio::time::timeout(Duration::from_secs(5), updates.next())
        .await
        .expect("SIGHUP produces a theme reload")
        .expect("the stream remains alive");
}
