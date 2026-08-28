use std::path::{Path, PathBuf};

use futures_util::{Stream, StreamExt};

use crate::load::{DATA_DIR, user_dir};
use crate::watch::{Update, watch_all};

const THEMES_DIR: &str = "themes";
const STYLES_FILE: &str = "styles.css";

pub const DEFAULT_THEME: &str = "adwaita";
pub const PANEL_STYLESHEET: &str = "panel.css";
pub const WALLPAPER_STYLESHEET: &str = "wallpaper.css";
pub const LOCK_STYLESHEET: &str = "lock.css";

fn theme_dirs_from(user_dir: Option<&Path>, theme: &str) -> Vec<PathBuf> {
    let Some(user) = user_dir else {
        return Vec::new();
    };

    let themes = user.join(THEMES_DIR);
    let mut dirs = vec![user.to_path_buf(), themes.clone()];
    if !theme.is_empty() {
        dirs.push(themes.join(theme));
    }
    dirs
}

fn theme_dirs(theme: &str) -> Vec<PathBuf> {
    theme_dirs_from(user_dir().as_deref(), theme)
}

fn is_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
}

fn theme_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(user) = user_dir() {
        roots.push(user.join(THEMES_DIR));
    }
    roots.push(Path::new(DATA_DIR).join(THEMES_DIR));
    roots
}

fn stylesheet_in(roots: &[PathBuf], theme: &str, name: &str) -> Option<PathBuf> {
    let mut themes = Vec::new();
    if !theme.is_empty() {
        themes.push(theme);
    }
    if theme != DEFAULT_THEME {
        themes.push(DEFAULT_THEME);
    }

    themes
        .into_iter()
        .flat_map(|theme| roots.iter().map(move |root| root.join(theme).join(name)))
        .find(|path| is_file(path))
}

pub fn stylesheet(theme: &str, name: &str) -> Option<PathBuf> {
    stylesheet_in(&theme_roots(), theme, name)
}

pub fn user_stylesheet() -> Option<PathBuf> {
    let path = user_dir()?.join(STYLES_FILE);
    is_file(&path).then_some(path)
}

pub fn watch_theme(theme: &str) -> impl Stream<Item = ()> + Send + 'static {
    watch_all(theme_dirs(theme)).filter_map(|update| async move {
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
    fn the_user_copy_of_a_sheet_wins_over_the_installed_one() {
        let base = tempfile::tempdir().expect("a temporary directory");
        sheet(base.path(), "user", "adwaita", "panel.css");
        sheet(base.path(), "data", "adwaita", "panel.css");

        assert_eq!(
            stylesheet_in(&roots(base.path()), "adwaita", "panel.css"),
            Some(base.path().join("user/adwaita/panel.css"))
        );
    }

    #[test]
    fn a_theme_supplying_one_sheet_inherits_the_rest_from_the_default_theme() {
        let base = tempfile::tempdir().expect("a temporary directory");
        sheet(base.path(), "user", "nord", "panel.css");
        sheet(base.path(), "data", "adwaita", "panel.css");
        sheet(base.path(), "data", "adwaita", "lock.css");

        assert_eq!(
            stylesheet_in(&roots(base.path()), "nord", "panel.css"),
            Some(base.path().join("user/nord/panel.css"))
        );
        assert_eq!(
            stylesheet_in(&roots(base.path()), "nord", "lock.css"),
            Some(base.path().join("data/adwaita/lock.css")),
            "the selected theme has no lock.css, so the default theme answers"
        );
    }

    #[test]
    fn a_sheet_no_theme_supplies_resolves_to_nothing() {
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
        sheet(base.path(), "data", "adwaita", "panel.css");

        assert_eq!(
            stylesheet_in(&roots(base.path()), "adwaita", "panel.css"),
            Some(base.path().join("data/adwaita/panel.css"))
        );
    }

    #[test]
    fn the_set_is_the_theme_directory_and_the_ancestors_it_falls_back_onto() {
        assert_eq!(
            theme_dirs_from(Some(Path::new("/home/u/.config/glimpse")), "adwaita"),
            [
                PathBuf::from("/home/u/.config/glimpse"),
                PathBuf::from("/home/u/.config/glimpse/themes"),
                PathBuf::from("/home/u/.config/glimpse/themes/adwaita"),
            ]
        );
    }

    #[test]
    fn an_unnamed_theme_watches_the_ancestors_alone_rather_than_the_themes_directory_twice() {
        assert_eq!(
            theme_dirs_from(Some(Path::new("/home/u/.config/glimpse")), ""),
            [
                PathBuf::from("/home/u/.config/glimpse"),
                PathBuf::from("/home/u/.config/glimpse/themes"),
            ]
        );
    }

    #[test]
    fn a_platform_naming_no_config_directory_watches_nothing() {
        assert!(theme_dirs_from(None, "adwaita").is_empty());
    }
}
