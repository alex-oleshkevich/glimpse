use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::{ConfigDiscovery, services::theme::EffectiveThemeMode};

const SYSTEM_THEMES_DIR: &str = "/usr/share/glimpse/themes";
const PANEL_CSS: &str = "panel.css";
const LOCK_CSS: &str = "lock.css";
const WALLPAPER_LIGHT_STEM: &str = "wallpaper-light";
const WALLPAPER_DARK_STEM: &str = "wallpaper-dark";
const BACKDROP_LIGHT_STEM: &str = "backdrop-light";
const BACKDROP_DARK_STEM: &str = "backdrop-dark";
const LOCK_BG_LIGHT_STEM: &str = "lock-light";
const LOCK_BG_DARK_STEM: &str = "lock-dark";
const WALLPAPER_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "avif"];

pub const GLIMPSE_THEME_ENV: &str = "GLIMPSE_THEME";
pub const GLIMPSE_THEME_NAME_ENV: &str = "GLIMPSE_THEME_NAME";

#[cfg(feature = "dev")]
const DEV_THEMES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../themes");

/// A resolved theme pack: a logical bundle of panel CSS, lock CSS, and
/// light/dark wallpapers/backdrops/lock backgrounds, each resolved
/// independently across a search path.
///
/// File lookup walks the roots returned by [`ThemePack::search_roots`] in
/// order — first hit per file wins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemePack {
    pub name: String,
    pub panel_css: Option<PathBuf>,
    pub lock_css: Option<PathBuf>,
    pub wallpaper_light: Option<PathBuf>,
    pub wallpaper_dark: Option<PathBuf>,
    pub backdrop_light: Option<PathBuf>,
    pub backdrop_dark: Option<PathBuf>,
    pub lock_bg_light: Option<PathBuf>,
    pub lock_bg_dark: Option<PathBuf>,
}

impl ThemePack {
    pub fn resolve(name: &str) -> Self {
        // GLIMPSE_THEME_NAME overrides the configured name but still walks the
        // normal search roots. GLIMPSE_THEME (handled inside search_roots) is
        // stronger and bypasses the search entirely.
        let effective_name = env::var(GLIMPSE_THEME_NAME_ENV)
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| name.to_owned());
        let roots = Self::search_roots(&effective_name);
        Self {
            name: effective_name,
            panel_css: find_in_roots(&roots, |dir| existing_file(&dir.join(PANEL_CSS))),
            lock_css: find_in_roots(&roots, |dir| existing_file(&dir.join(LOCK_CSS))),
            wallpaper_light: find_in_roots(&roots, |dir| find_wallpaper(dir, WALLPAPER_LIGHT_STEM)),
            wallpaper_dark: find_in_roots(&roots, |dir| find_wallpaper(dir, WALLPAPER_DARK_STEM)),
            backdrop_light: find_in_roots(&roots, |dir| find_wallpaper(dir, BACKDROP_LIGHT_STEM)),
            backdrop_dark: find_in_roots(&roots, |dir| find_wallpaper(dir, BACKDROP_DARK_STEM)),
            lock_bg_light: find_in_roots(&roots, |dir| find_wallpaper(dir, LOCK_BG_LIGHT_STEM)),
            lock_bg_dark: find_in_roots(&roots, |dir| find_wallpaper(dir, LOCK_BG_DARK_STEM)),
        }
    }

    pub fn wallpaper_for(&self, mode: EffectiveThemeMode) -> Option<&Path> {
        pick_for_mode(&self.wallpaper_light, &self.wallpaper_dark, mode)
    }

    pub fn backdrop_for(&self, mode: EffectiveThemeMode) -> Option<&Path> {
        pick_for_mode(&self.backdrop_light, &self.backdrop_dark, mode)
    }

    pub fn lock_bg_for(&self, mode: EffectiveThemeMode) -> Option<&Path> {
        pick_for_mode(&self.lock_bg_light, &self.lock_bg_dark, mode)
    }

    /// Pack directories searched in order. The first existing file in any of
    /// these wins for each artifact independently.
    pub fn search_roots(name: &str) -> Vec<PathBuf> {
        if let Some(root) = env::var_os(GLIMPSE_THEME_ENV) {
            let root = PathBuf::from(root);
            if !root.as_os_str().is_empty() {
                return vec![root];
            }
        }

        let mut roots = Vec::with_capacity(3);
        #[cfg(feature = "dev")]
        roots.push(PathBuf::from(DEV_THEMES_DIR).join(name));
        roots.push(
            ConfigDiscovery::from_process()
                .config_dir()
                .join("themes")
                .join(name),
        );
        roots.push(PathBuf::from(SYSTEM_THEMES_DIR).join(name));
        roots
    }
}

fn pick_for_mode<'a>(
    light: &'a Option<PathBuf>,
    dark: &'a Option<PathBuf>,
    mode: EffectiveThemeMode,
) -> Option<&'a Path> {
    let primary = match mode {
        EffectiveThemeMode::Light => light.as_deref(),
        EffectiveThemeMode::Dark => dark.as_deref(),
    };
    primary.or_else(|| light.as_deref().or(dark.as_deref()))
}

fn find_in_roots<F>(roots: &[PathBuf], mut pick: F) -> Option<PathBuf>
where
    F: FnMut(&Path) -> Option<PathBuf>,
{
    roots.iter().find_map(|dir| pick(dir))
}

fn existing_file(path: &Path) -> Option<PathBuf> {
    fs::metadata(path)
        .ok()
        .filter(|m| m.is_file())
        .map(|_| path.to_path_buf())
}

fn find_wallpaper(dir: &Path, stem: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut best: Option<(usize, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if file_stem != stem {
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let ext_lower = ext.to_ascii_lowercase();
        let Some(rank) = WALLPAPER_EXTS.iter().position(|e| *e == ext_lower) else {
            continue;
        };
        // Use fs::metadata (follows symlinks) rather than entry.file_type
        // (which doesn't) so symlinked assets in a theme pack are accepted.
        if !fs::metadata(&path).is_ok_and(|m| m.is_file()) {
            continue;
        }
        if best.as_ref().is_none_or(|(r, _)| rank < *r) {
            best = Some((rank, path));
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File, create_dir_all};
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    struct TestDir {
        root: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("glimpse-theme-pack-{name}-{suffix}"));
            create_dir_all(&root).unwrap();
            Self { root }
        }

        fn path(&self) -> &Path {
            &self.root
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn touch(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            create_dir_all(parent).unwrap();
        }
        File::create(path)
            .unwrap()
            .write_all(content.as_bytes())
            .unwrap();
    }

    fn resolve_with_roots(roots: &[PathBuf]) -> ThemePack {
        ThemePack {
            name: "test".into(),
            panel_css: find_in_roots(roots, |d| existing_file(&d.join(PANEL_CSS))),
            lock_css: find_in_roots(roots, |d| existing_file(&d.join(LOCK_CSS))),
            wallpaper_light: find_in_roots(roots, |d| find_wallpaper(d, WALLPAPER_LIGHT_STEM)),
            wallpaper_dark: find_in_roots(roots, |d| find_wallpaper(d, WALLPAPER_DARK_STEM)),
            backdrop_light: find_in_roots(roots, |d| find_wallpaper(d, BACKDROP_LIGHT_STEM)),
            backdrop_dark: find_in_roots(roots, |d| find_wallpaper(d, BACKDROP_DARK_STEM)),
            lock_bg_light: find_in_roots(roots, |d| find_wallpaper(d, LOCK_BG_LIGHT_STEM)),
            lock_bg_dark: find_in_roots(roots, |d| find_wallpaper(d, LOCK_BG_DARK_STEM)),
        }
    }

    #[test]
    fn user_pack_wins_over_system_for_each_file_independently() {
        let dir = TestDir::new("user-vs-system");
        let user_pack = dir.path().join("user/mytheme");
        let system_pack = dir.path().join("system/mytheme");
        touch(&user_pack.join(PANEL_CSS), ".panel {}");
        touch(&system_pack.join(LOCK_CSS), ".lock {}");
        touch(&system_pack.join("wallpaper-light.jpg"), "");

        let pack = resolve_with_roots(&[user_pack.clone(), system_pack.clone()]);

        assert_eq!(pack.panel_css, Some(user_pack.join(PANEL_CSS)));
        assert_eq!(pack.lock_css, Some(system_pack.join(LOCK_CSS)));
        assert_eq!(
            pack.wallpaper_light,
            Some(system_pack.join("wallpaper-light.jpg"))
        );
        assert_eq!(pack.wallpaper_dark, None);
    }

    #[test]
    fn missing_pack_dir_yields_empty_pack() {
        let dir = TestDir::new("missing");
        let pack = resolve_with_roots(&[dir.path().join("does-not-exist")]);
        assert_eq!(pack.panel_css, None);
        assert_eq!(pack.lock_css, None);
        assert_eq!(pack.wallpaper_light, None);
        assert_eq!(pack.wallpaper_dark, None);
    }

    #[test]
    fn find_wallpaper_prefers_earlier_extensions_and_ignores_case() {
        let dir = TestDir::new("ext-order");
        touch(&dir.path().join("wallpaper-light.webp"), "");
        touch(&dir.path().join("wallpaper-light.PNG"), "");
        let resolved = find_wallpaper(dir.path(), WALLPAPER_LIGHT_STEM).unwrap();
        assert_eq!(resolved.extension().and_then(|e| e.to_str()), Some("PNG"));
    }

    #[test]
    fn find_wallpaper_ignores_other_stems_and_unsupported_exts() {
        let dir = TestDir::new("unsupported");
        touch(&dir.path().join("wallpaper-light.bmp"), "");
        touch(&dir.path().join("other.png"), "");
        assert_eq!(find_wallpaper(dir.path(), WALLPAPER_LIGHT_STEM), None);
    }

    #[test]
    fn wallpaper_for_falls_back_to_other_mode_when_only_one_is_present() {
        let only_light = PathBuf::from("/tmp/wallpaper-light.png");
        let pack = ThemePack {
            name: "x".into(),
            wallpaper_light: Some(only_light.clone()),
            ..ThemePack::default()
        };
        assert_eq!(
            pack.wallpaper_for(EffectiveThemeMode::Dark),
            Some(only_light.as_path())
        );
        assert_eq!(
            pack.wallpaper_for(EffectiveThemeMode::Light),
            Some(only_light.as_path())
        );
    }

    #[test]
    fn wallpaper_for_returns_none_when_pack_has_no_wallpapers() {
        let pack = ThemePack::default();
        assert_eq!(pack.wallpaper_for(EffectiveThemeMode::Light), None);
        assert_eq!(pack.wallpaper_for(EffectiveThemeMode::Dark), None);
    }
}
