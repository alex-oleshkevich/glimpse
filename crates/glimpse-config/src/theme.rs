use std::borrow::Cow;
use std::path::{Path, PathBuf};

use futures_util::{Stream, StreamExt};

use crate::load::{DATA_DIR, push, user_dir};
use crate::watch::{Update, watch_all};

const THEMES_DIR: &str = "themes";
const STYLES_FILE: &str = "styles.css";
const THEMES_DIR_ENV: &str = "GLIMPSE_THEMES_DIR";
const THEME_ENV: &str = "GLIMPSE_THEME";

pub const DEFAULT_THEME: &str = "adwaita";
pub const PANEL_STYLESHEET: &str = "panel.css";
pub const WALLPAPER_STYLESHEET: &str = "wallpaper.css";
pub const LOCK_STYLESHEET: &str = "lock.css";

fn themes_dir_from_env() -> Option<PathBuf> {
    match std::env::var(THEMES_DIR_ENV) {
        Ok(dir) => Some(PathBuf::from(dir)),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => {
            tracing::warn!(%error, "{THEMES_DIR_ENV} is not readable; falling back to the usual roots");
            None
        }
    }
}

fn theme_from_env() -> Option<String> {
    match std::env::var(THEME_ENV) {
        Ok(theme) => Some(theme),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => {
            tracing::warn!(%error, "{THEME_ENV} is not readable; using the configured theme");
            None
        }
    }
}

fn selected(theme: &str) -> Cow<'_, str> {
    theme_from_env().map_or(Cow::Borrowed(theme), Cow::Owned)
}

fn theme_dirs() -> Vec<PathBuf> {
    if let Some(dir) = themes_dir_from_env() {
        return vec![dir];
    }

    let mut roots: Vec<PathBuf> = user_dir()
        .map(|dir| dir.join(THEMES_DIR))
        .into_iter()
        .collect();
    roots.push(Path::new(DATA_DIR).join(THEMES_DIR));
    roots
}

fn is_dir(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

fn is_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
}

fn wanted(theme: &str) -> Vec<&str> {
    let mut wanted = Vec::new();
    if !theme.is_empty() {
        wanted.push(theme);
    }
    if theme != DEFAULT_THEME {
        wanted.push(DEFAULT_THEME);
    }
    wanted
}

fn theme_dir_in(roots: &[PathBuf], theme: &str) -> Option<PathBuf> {
    wanted(theme)
        .into_iter()
        .flat_map(|name| roots.iter().map(move |root| root.join(name)))
        .find(|dir| is_dir(dir))
}

fn watch_dirs_in(roots: &[PathBuf], user_dir: Option<&Path>, theme: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = user_dir {
        push(&mut dirs, dir.to_path_buf());
    }
    for root in roots {
        push(&mut dirs, root.clone());
        if !theme.is_empty() {
            push(&mut dirs, root.join(theme));
        }
    }
    if let Some(dir) = theme_dir_in(roots, theme) {
        push(&mut dirs, dir);
    }
    dirs
}

pub fn theme_dir_for(theme: &str) -> Option<PathBuf> {
    theme_dir_in(&theme_dirs(), &selected(theme))
}

fn stylesheet_in(roots: &[PathBuf], theme: &str, name: &str) -> Option<PathBuf> {
    let path = theme_dir_in(roots, theme)?.join(name);
    is_file(&path).then_some(path)
}

pub fn stylesheet(theme: &str, name: &str) -> Option<PathBuf> {
    stylesheet_in(&theme_dirs(), &selected(theme), name)
}

pub fn user_stylesheet() -> Option<PathBuf> {
    let path = user_dir()?.join(STYLES_FILE);
    is_file(&path).then_some(path)
}

pub fn watch_theme(theme: &str) -> impl Stream<Item = ()> + Send + 'static {
    let dirs = watch_dirs_in(&theme_dirs(), user_dir().as_deref(), &selected(theme));
    watch_all(dirs).filter_map(|update| async move {
        match update {
            Update::Changed(_) | Update::Rearmed => Some(()),
            Update::Unavailable(reason) => {
                tracing::warn!(
                    reason,
                    "the theme is not being watched; restart to pick up edits"
                );
                None
            }
        }
    })
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn a_platform_naming_no_config_directory_still_watches_the_installed_root() {
        assert_eq!(
            watch_dirs_in(&[PathBuf::from("/usr/share/glimpse/themes")], None, ""),
            [PathBuf::from("/usr/share/glimpse/themes")]
        );
    }
}
