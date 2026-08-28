use std::path::{Path, PathBuf};

use futures_util::{Stream, StreamExt};

use crate::load::user_dir;
use crate::watch::{Update, watch_all};

const THEMES_DIR: &str = "themes";

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
