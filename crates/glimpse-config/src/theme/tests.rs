use std::pin::Pin;
use std::time::Duration;

use futures_util::StreamExt;

use super::*;
use crate::watch::Hub;

const SETTLE: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_millis(1500);

fn roots(base: &Path) -> Vec<PathBuf> {
    vec![base.join("user"), base.join("etc")]
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("a parent directory")).expect("creates");
    std::fs::write(path, text).expect("writes");
}

fn read(base: &Path, name: &str) -> Result<(Theme, Option<ConfigError>), ConfigError> {
    read_theme(
        &roots(base),
        &base.join("data"),
        None,
        name,
        PANEL_STYLESHEET,
    )
}

fn strict(base: &Path, name: &str) -> Result<Theme, ConfigError> {
    load_theme_in(
        &roots(base),
        &base.join("data"),
        None,
        name,
        PANEL_STYLESHEET,
    )
}

fn load(base: &Path, name: &str) -> Result<Theme, ConfigError> {
    read(base, name).map(|(theme, _)| theme)
}

#[test]
fn a_theme_only_a_root_has_is_read_from_that_root() {
    let base = tempfile::tempdir().expect("a temporary directory");
    write(&base.path().join("etc/themes/x/panel.css"), "etc");

    assert_eq!(
        load(base.path(), "x").expect("loads").sheet.as_deref(),
        Some("etc")
    );
}

#[test]
fn the_highest_roots_copy_of_a_theme_wins_over_the_installed_one() {
    let base = tempfile::tempdir().expect("a temporary directory");
    write(&base.path().join("user/themes/adwaita/panel.css"), "user");
    write(&base.path().join("etc/themes/adwaita/panel.css"), "etc");
    write(&base.path().join("data/themes/adwaita/panel.css"), "data");

    assert_eq!(
        load(base.path(), "adwaita")
            .expect("loads")
            .sheet
            .as_deref(),
        Some("user")
    );
}

#[test]
fn an_unknown_theme_falls_back_to_the_installed_default() {
    let base = tempfile::tempdir().expect("a temporary directory");
    write(&base.path().join("data/themes/adwaita/panel.css"), "data");

    assert_eq!(
        load(base.path(), "nord").expect("loads").sheet.as_deref(),
        Some("data")
    );
    assert_eq!(
        load(base.path(), "").expect("loads").sheet.as_deref(),
        Some("data"),
        "an unnamed theme goes straight to the default"
    );
}

#[test]
fn a_theme_that_exists_answers_alone_and_does_not_borrow_from_another_root() {
    let base = tempfile::tempdir().expect("a temporary directory");
    write(&base.path().join("user/themes/nord/panel.css"), "nord");
    write(&base.path().join("data/themes/nord/dark.css"), "other root");
    write(&base.path().join("data/themes/adwaita/dark.css"), "default");

    let theme = load(base.path(), "nord").expect("loads");

    assert_eq!(theme.sheet.as_deref(), Some("nord"));
    assert_eq!(
        theme.dark, None,
        "the directory is the unit, so a dark sheet cannot arrive from another one"
    );
}

#[test]
fn a_theme_directory_override_replaces_every_root() {
    let base = tempfile::tempdir().expect("a temporary directory");
    write(&base.path().join("user/themes/x/panel.css"), "user");
    write(&base.path().join("override/x/panel.css"), "override");

    let theme = load_theme_in(
        &roots(base.path()),
        &base.path().join("data"),
        Some(base.path().join("override")),
        "x",
        PANEL_STYLESHEET,
    )
    .expect("loads");

    assert_eq!(theme.sheet.as_deref(), Some("override"));
}

#[test]
fn the_users_sheets_come_from_the_roots_and_never_from_the_installed_data() {
    let base = tempfile::tempdir().expect("a temporary directory");
    write(&base.path().join("data/styles.css"), "data");
    write(&base.path().join("data/dark.css"), "data");
    assert_eq!(load(base.path(), "x").expect("loads"), Theme::default());

    write(&base.path().join("etc/styles.css"), "etc");
    write(&base.path().join("user/dark.css"), "user dark");
    let theme = load(base.path(), "x").expect("a dark sheet needs no light one");
    assert_eq!(theme.user.as_deref(), Some("etc"));
    assert_eq!(theme.user_dark.as_deref(), Some("user dark"));
}

#[test]
fn a_user_sheet_that_is_a_directory_empties_only_its_own_layer() {
    let base = tempfile::tempdir().expect("a temporary directory");
    write(
        &base.path().join("data/themes/adwaita/panel.css"),
        "shipped",
    );
    write(&base.path().join("user/dark.css"), "user dark");
    std::fs::create_dir_all(base.path().join("user/styles.css")).expect("creates");

    let (theme, failure) = read(base.path(), "adwaita").expect("reads");

    assert_eq!(theme.sheet.as_deref(), Some("shipped"));
    assert_eq!(theme.user, None);
    assert_eq!(theme.user_dark.as_deref(), Some("user dark"));
    assert!(
        failure
            .expect("the failure is reported")
            .to_string()
            .contains("styles.css")
    );
    assert!(strict(base.path(), "adwaita").is_err());
}

#[test]
fn an_unknown_theme_is_reported_and_the_default_still_loads() {
    let base = tempfile::tempdir().expect("a temporary directory");
    write(
        &base.path().join("data/themes/adwaita/panel.css"),
        "shipped",
    );

    let (theme, failure) = read(base.path(), "nope").expect("reads");

    assert_eq!(theme.sheet.as_deref(), Some("shipped"));
    assert!(
        failure
            .expect("reported")
            .to_string()
            .contains("unknown theme `nope`")
    );
    assert!(read(base.path(), "adwaita").expect("reads").1.is_none());
    assert!(read(base.path(), "").expect("reads").1.is_none());
}

fn hub_over(dirs: Vec<PathBuf>) -> Hub {
    Hub::start(move || dirs.clone())
}

fn theme_reloads(
    hub: &Hub,
    base: &Path,
    name: &'static str,
    current: Theme,
) -> Pin<Box<dyn futures_util::Stream<Item = Reload<Theme>> + Send>> {
    let receiver = hub.subscribe();
    let base = base.to_path_buf();
    Box::pin(crate::watch::reloads(
        move || receiver,
        Arc::new(move || strict(&base, name)),
        current,
        None,
        true,
    ))
}

async fn next(
    reloads: &mut Pin<Box<dyn futures_util::Stream<Item = Reload<Theme>> + Send>>,
) -> Reload<Theme> {
    tokio::time::timeout(SETTLE, reloads.next())
        .await
        .expect("a reload within the settle window")
        .expect("the stream is still alive")
}

#[tokio::test]
async fn a_stylesheet_replaced_by_a_directory_is_rejected_and_the_last_theme_is_held() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let user = base.path().join("user");
    write(&user.join("styles.css"), "a { color: red }");
    let current = load(base.path(), "adwaita").expect("loads");
    let hub = hub_over(vec![user.clone()]);
    let mut reloads = theme_reloads(&hub, base.path(), "adwaita", current);

    std::fs::remove_file(user.join("styles.css")).expect("removes");
    std::fs::create_dir(user.join("styles.css")).expect("creates");
    assert!(matches!(next(&mut reloads).await, Reload::Rejected(_)));

    std::fs::remove_dir(user.join("styles.css")).expect("removes");
    write(&user.join("styles.css"), "a { color: red }");
    assert_eq!(
        next(&mut reloads).await,
        Reload::Recovered,
        "the held theme equals the restored one"
    );
}

#[tokio::test]
async fn an_editor_swap_file_beside_an_unchanged_stylesheet_reloads_nothing() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let user = base.path().join("user");
    write(&user.join("styles.css"), "a { color: red }");
    let current = load(base.path(), "adwaita").expect("loads");
    let hub = hub_over(vec![user.clone()]);
    let mut reloads = theme_reloads(&hub, base.path(), "adwaita", current);

    write(&user.join(".styles.css.swp"), "swap");

    assert!(
        tokio::time::timeout(QUIET, reloads.next()).await.is_err(),
        "an identical theme yields nothing"
    );

    write(&user.join("styles.css"), "a { color: blue }");
    assert!(
        matches!(next(&mut reloads).await, Reload::Changed(theme) if theme.user.as_deref() == Some("a { color: blue }"))
    );
}

#[tokio::test]
async fn a_different_theme_than_the_one_held_arrives_without_a_file_event() {
    let base = tempfile::tempdir().expect("a temporary directory");
    write(&base.path().join("user/themes/x/panel.css"), "x");
    write(&base.path().join("user/themes/y/panel.css"), "y");
    let current = load(base.path(), "x").expect("loads");
    let hub = hub_over(vec![base.path().join("user")]);

    let mut renamed = theme_reloads(&hub, base.path(), "y", current.clone());
    assert!(
        matches!(next(&mut renamed).await, Reload::Changed(theme) if theme.sheet.as_deref() == Some("y"))
    );

    let mut same = theme_reloads(&hub, base.path(), "x", current);
    assert!(
        tokio::time::timeout(QUIET, same.next()).await.is_err(),
        "an equal theme waits for a change"
    );
}
