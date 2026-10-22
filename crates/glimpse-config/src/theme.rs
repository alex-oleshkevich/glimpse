use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::Stream;

use crate::error::ConfigError;
use crate::load::{DATA_DIR, find_in_roots, is_missing, read, roots};
use crate::watch::{Reload, reloads, shared};

pub(crate) const THEMES_DIR: &str = "themes";
const STYLES_FILE: &str = "styles.css";
const THEMES_DIR_ENV: &str = "GLIMPSE_THEMES_DIR";
const THEME_ENV: &str = "GLIMPSE_THEME";

pub const DEFAULT_THEME: &str = "adwaita";
pub const PANEL_STYLESHEET: &str = "panel.css";
pub const WALLPAPER_STYLESHEET: &str = "wallpaper.css";
pub const LOCK_STYLESHEET: &str = "lock.css";
pub const DARK_STYLESHEET: &str = "dark.css";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Theme {
    pub sheet: Option<String>,
    pub dark: Option<String>,
    pub user: Option<String>,
    pub user_dark: Option<String>,
}

pub(crate) fn themes_dir_from_env() -> Option<PathBuf> {
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

fn read_optional(path: Option<PathBuf>) -> Result<Option<String>, ConfigError> {
    match path.map(|path| read(&path)) {
        Some(Ok(text)) => Ok(Some(text)),
        Some(Err(error)) if !is_missing(&error) => Err(error),
        _ => Ok(None),
    }
}

pub fn load_theme(name: &str, sheet: &str) -> Result<Theme, ConfigError> {
    let (theme, failure) = read_theme(
        &roots(),
        Path::new(DATA_DIR),
        themes_dir_from_env(),
        &selected(name),
        sheet,
    )?;
    if let Some(error) = failure {
        tracing::error!(%error, "starting with the theme layers that could be read");
    }
    Ok(theme)
}

fn load_theme_strict(name: &str, sheet: &str) -> Result<Theme, ConfigError> {
    load_theme_in(
        &roots(),
        Path::new(DATA_DIR),
        themes_dir_from_env(),
        &selected(name),
        sheet,
    )
}

fn load_theme_in(
    roots: &[PathBuf],
    data: &Path,
    themes: Option<PathBuf>,
    name: &str,
    sheet: &str,
) -> Result<Theme, ConfigError> {
    match read_theme(roots, data, themes, name, sheet)? {
        (theme, None) => Ok(theme),
        (_, Some(error)) => Err(error),
    }
}

fn read_theme(
    roots: &[PathBuf],
    data: &Path,
    themes: Option<PathBuf>,
    name: &str,
    sheet: &str,
) -> Result<(Theme, Option<ConfigError>), ConfigError> {
    let theme_roots = match themes {
        Some(dir) => vec![dir],
        None => roots
            .iter()
            .map(|root| root.join(THEMES_DIR))
            .chain([data.join(THEMES_DIR)])
            .collect(),
    };
    let dir = theme_dir_in(&theme_roots, name);
    let themed = |file: &str| dir.as_ref().map(|dir| dir.join(file));
    let mut failure = None;
    if !name.is_empty()
        && name != DEFAULT_THEME
        && dir.as_ref().and_then(|dir| dir.file_name()) != Some(OsStr::new(name))
    {
        failure = Some(ConfigError::Schema {
            message: format!("unknown theme `{name}`"),
        });
    }
    let mut layer = |read: Result<Option<String>, ConfigError>| {
        read.unwrap_or_else(|error| {
            failure.get_or_insert(error);
            None
        })
    };
    let user = layer(read_optional(find_in_roots(roots, Path::new(STYLES_FILE))));
    let user_dark = layer(read_optional(find_in_roots(
        roots,
        Path::new(DARK_STYLESHEET),
    )));
    Ok((
        Theme {
            sheet: read_optional(themed(sheet))?,
            dark: read_optional(themed(DARK_STYLESHEET))?,
            user,
            user_dark,
        },
        failure,
    ))
}

pub fn watch_theme(
    name: &str,
    sheet: &'static str,
    current: Theme,
) -> impl Stream<Item = Reload<Theme>> + Send + 'static {
    let name = name.to_owned();
    reloads(
        || shared().subscribe(),
        Arc::new(move || load_theme_strict(&name, sheet)),
        current,
        None,
        true,
    )
}

#[cfg(test)]
mod tests;
