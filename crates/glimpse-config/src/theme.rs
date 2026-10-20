use std::borrow::Cow;
use std::path::{Path, PathBuf};

use futures_util::{Stream, StreamExt, stream};

use crate::load::{DATA_DIR, push, user_dir};
use crate::watch::{Update, hangups, watch_all};

pub(crate) const THEMES_DIR: &str = "themes";
const STYLES_FILE: &str = "styles.css";
const THEMES_DIR_ENV: &str = "GLIMPSE_THEMES_DIR";
const THEME_ENV: &str = "GLIMPSE_THEME";

pub const DEFAULT_THEME: &str = "adwaita";
pub const PANEL_STYLESHEET: &str = "panel.css";
pub const WALLPAPER_STYLESHEET: &str = "wallpaper.css";
pub const LOCK_STYLESHEET: &str = "lock.css";
pub const DARK_STYLESHEET: &str = "dark.css";

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

fn theme_dir_in(roots: &[PathBuf], theme: &str) -> Option<PathBuf> {
    let mut names = Vec::new();
    if !theme.is_empty() {
        names.push(theme);
    }
    if theme != DEFAULT_THEME {
        names.push(DEFAULT_THEME);
    }

    names
        .into_iter()
        .flat_map(|name| roots.iter().map(move |root| root.join(name)))
        .find(|dir| dir.is_dir())
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

fn stylesheet_in(roots: &[PathBuf], theme: &str, name: &str) -> Option<PathBuf> {
    let path = theme_dir_in(roots, theme)?.join(name);
    path.is_file().then_some(path)
}

pub fn stylesheet(theme: &str, name: &str) -> Option<PathBuf> {
    stylesheet_in(&theme_dirs(), &selected(theme), name)
}

fn user_sheet_in(dir: &Path, name: &str) -> Option<PathBuf> {
    let path = dir.join(name);
    path.is_file().then_some(path)
}

pub fn user_stylesheet() -> Option<PathBuf> {
    user_sheet_in(&user_dir()?, STYLES_FILE)
}

pub fn user_dark_stylesheet() -> Option<PathBuf> {
    user_sheet_in(&user_dir()?, DARK_STYLESHEET)
}

pub fn watch_theme(theme: &str) -> impl Stream<Item = ()> + Send + 'static {
    let user = user_dir();
    let dirs = watch_dirs_in(&theme_dirs(), user.as_deref(), &selected(theme));
    let watched = watch_all(dirs).filter_map(move |update| {
        let reload = reload(update, user.as_deref());
        async move { reload }
    });
    stream::select(Box::pin(watched), Box::pin(hangups()))
}

fn reads_theme(user: Option<&Path>, path: &Path) -> bool {
    path.parent() != user
        || path.file_name().is_some_and(|name| {
            name == STYLES_FILE || name == DARK_STYLESHEET || name == THEMES_DIR
        })
}

fn reload(update: Update, user: Option<&Path>) -> Option<()> {
    match update {
        Update::Changed(paths) => paths
            .iter()
            .any(|path| reads_theme(user, path))
            .then_some(()),
        Update::Rearmed => Some(()),
        Update::Unavailable(reason) => {
            tracing::warn!(
                reason,
                "the theme is not being watched; SIGHUP still reloads it"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests;
