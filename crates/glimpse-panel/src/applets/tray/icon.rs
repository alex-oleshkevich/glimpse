use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use glimpse_dbus::status_notifier_item::{TrayPixmap, best_pixmap};
use gtk4::prelude::*;

const MISSING: &str = "image-missing-symbolic";
/// A private theme directory is added to the shared `gtk::IconTheme`, which is process-wide, so the
/// number of them is bounded: an application that rotates its theme path would otherwise grow the
/// search list for the panel's lifetime.
const MOST_THEME_PATHS: usize = 16;
const SUFFIXES: [&str; 4] = [".png", ".svg", ".xpm", ".ico"];

/// What an item's icon should be built from. Separated from building it so the ladder can be
/// tested without a display: every branch below was a bug in the previous implementation, twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A literal file, either named absolutely or found under the item's own theme directory.
    File(PathBuf),
    /// A themed name, possibly resolved through a search path added for this item.
    Themed(String),
    /// The application sent pixels; this is the index of the best size for the target.
    Pixmap(usize),
    Missing,
}

/// The ladder, in order. `exists` is injected so the decision is testable; the caller passes
/// `Path::exists`.
pub fn choose(
    name: Option<&str>,
    theme_path: Option<&str>,
    pixmaps: &[TrayPixmap],
    target: i32,
    exists: &dyn Fn(&Path) -> bool,
) -> Source {
    if let Some(name) = name.filter(|name| !name.is_empty()) {
        // 1. An absolute path. Handing one to the icon theme is never right — the theme has no
        //    entry named `/run/user/1000/…` and renders its own broken-image glyph, which looks
        //    worse than ours. So a path that has gone drops to the pixmaps instead of falling
        //    through. Measured on a live bar: this is what a libayatana item whose icon file was
        //    rotated away actually looks like.
        let literal = Path::new(name);
        if literal.is_absolute() {
            return match exists(literal) {
                true => Source::File(literal.to_path_buf()),
                false => pixels(pixmaps, target),
            };
        }

        // 2. The item's own theme directory, probed as a literal file before the icon theme is
        //    touched — that is what an application shipping `foo.png` beside its binary means.
        if let Some(base) = theme_path.filter(|path| !path.is_empty()) {
            let base = Path::new(base);
            if exists(base) {
                let bare = base.join(name);
                if exists(&bare) {
                    return Source::File(bare);
                }
                for suffix in SUFFIXES {
                    let candidate = base.join(format!("{name}{suffix}"));
                    if exists(&candidate) {
                        return Source::File(candidate);
                    }
                }
            }
        }

        // 3/4. Hand it to the icon theme, which is where a search path added for this item pays off.
        return Source::Themed(name.to_owned());
    }

    // 5. Pixels. `best_pixmap` never upscales unless nothing else fits.
    pixels(pixmaps, target)
}

fn pixels(pixmaps: &[TrayPixmap], target: i32) -> Source {
    match best_pixmap(pixmaps, target) {
        Some(index) => Source::Pixmap(index),
        None => Source::Missing,
    }
}

thread_local! {
    static SEARCHED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static TEXTURES: RefCell<HashMap<u64, gtk4::gdk::Texture>> = RefCell::new(HashMap::new());
    static RESOLVED: RefCell<HashMap<(String, String, i32), Source>> = RefCell::new(HashMap::new());
}

/// `choose` walks the filesystem — up to seven `exists` probes for an icon and seven more for an
/// overlay — and `paint` runs per chip on every state change. `.claude/rules/ui.md` forbids
/// blocking the GTK thread, and an icon on a slow or automounted path would do exactly that. The
/// answer depends only on the name, the item's theme directory and the target size, so it is
/// memoised on those three; `forget_resolutions` clears it when the icon theme or the scale moves,
/// which are the only other things that can change it.
pub fn resolve(
    name: Option<&str>,
    theme_path: Option<&str>,
    pixmaps: &[TrayPixmap],
    target: i32,
) -> Source {
    // Pixels are decided from the pixmaps in hand, not from disk, so they are never memoised —
    // the buffers can change under the same name.
    let Some(named) = name.filter(|name| !name.is_empty()) else {
        return choose(name, theme_path, pixmaps, target, &Path::exists);
    };
    let key = (
        named.to_owned(),
        theme_path.unwrap_or_default().to_owned(),
        target,
    );
    if let Some(held) = RESOLVED.with_borrow(|cache| cache.get(&key).cloned()) {
        return held;
    }
    let source = choose(name, theme_path, pixmaps, target, &Path::exists);
    RESOLVED.with_borrow_mut(|cache| {
        if cache.len() > MOST_THEME_PATHS * 8 {
            cache.clear();
        }
        cache.insert(key, source.clone());
    });
    source
}

/// The icon theme changed or the output's scale moved: every name-based answer has to be walked
/// again. Pixmaps are bytes and immune.
pub fn forget_resolutions() {
    RESOLVED.with_borrow_mut(HashMap::clear);
}

/// Add a private theme directory once. A path that does not exist is skipped and logged at `debug`,
/// never `warn`: a Flatpak application names `/app/share/icons`, which is real inside its sandbox
/// and absent out here, and warning about it once per item makes the journal useless.
pub fn learn_theme_path(path: &str) {
    if path.is_empty() {
        return;
    }
    // The dedupe check comes first: this runs per chip per paint, and `is_dir` is a syscall.
    SEARCHED.with_borrow_mut(|known| {
        if known.iter().any(|seen| seen == path) || known.len() >= MOST_THEME_PATHS {
            return;
        }
        if !Path::new(path).is_dir() {
            tracing::debug!(
                path,
                "a tray item named an icon directory that is not there"
            );
            known.push(path.to_owned());
            return;
        }
        if let Some(display) = gtk4::gdk::Display::default() {
            gtk4::IconTheme::for_display(&display).add_search_path(path);
            known.push(path.to_owned());
        }
    });
}

/// Build the icon the ladder chose. `gdk::Texture` implements `gio::Icon`, which is why a themed
/// name, a file and raw pixels all leave here as one `Option<gio::Icon>`.
pub fn build(source: Source, pixmaps: &[TrayPixmap]) -> Option<gio::Icon> {
    match source {
        Source::File(path) => Some(gio::FileIcon::new(&gio::File::for_path(path)).upcast()),
        Source::Themed(name) => Some(gio::ThemedIcon::new(&name).upcast()),
        Source::Pixmap(index) => pixmaps.get(index).map(texture).map(Cast::upcast),
        Source::Missing => Some(gio::ThemedIcon::new(MISSING).upcast()),
    }
}

/// "ARGB32 in network byte order" is byte order A,R,G,B, which is exactly GDK's `A8r8g8b8`. No
/// swizzle and no PNG round-trip. Premultiplication is unspecified by the tray protocol; a dark
/// halo around an icon is the symptom of guessing wrong.
///
/// Keyed on a content hash, which is what makes an application that rewrites its icon per message
/// cost nothing after the first time.
fn texture(pixmap: &TrayPixmap) -> gtk4::gdk::Texture {
    let key = fingerprint(pixmap);
    TEXTURES.with_borrow_mut(|cache| {
        if cache.len() > MOST_THEME_PATHS * 8 {
            cache.clear();
        }
        cache
            .entry(key)
            .or_insert_with(|| {
                gtk4::gdk::MemoryTexture::new(
                    pixmap.width,
                    pixmap.height,
                    gtk4::gdk::MemoryFormat::A8r8g8b8,
                    &glib::Bytes::from(&pixmap.argb),
                    (pixmap.width * 4) as usize,
                )
                .upcast()
            })
            .clone()
    })
}

fn fingerprint(pixmap: &TrayPixmap) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    pixmap.width.hash(&mut hasher);
    pixmap.height.hash(&mut hasher);
    pixmap.argb.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
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
}
