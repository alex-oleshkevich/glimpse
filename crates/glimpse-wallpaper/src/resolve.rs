use std::collections::HashSet;
use std::path::PathBuf;
use std::time::SystemTime;

use glimpse_config::{Fit, Transition, Wallpaper, WallpaperOutput};
use glimpse_widgets::raster::{Target, color};
use gtk4::gdk;

pub type MissingWarned = HashSet<(String, PathBuf)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Wallpaper,
    Backdrop,
}

impl Role {
    pub fn namespace(self) -> &'static str {
        match self {
            Role::Wallpaper => "glimpse-wallpaper",
            Role::Backdrop => "glimpse-backdrop",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key {
    pub connector: String,
    pub role: Role,
}

#[derive(Debug, Clone)]
pub struct Intent {
    pub image: Option<PathBuf>,
    pub color: gdk::RGBA,
    pub fit: Fit,
    pub transition: Transition,
    pub transition_ms: u32,
    pub blur_radius: u32,
    pub downscale_factor: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderKey {
    pub image: PathBuf,
    pub target: Target,
    pub fit: Fit,
    pub blur_radius: u32,
    pub mtime: Option<SystemTime>,
}

pub fn intent(
    wallpaper: &Wallpaper,
    connector: &str,
    role: Role,
    dark: bool,
    warned: &mut MissingWarned,
) -> Option<Intent> {
    match role {
        Role::Wallpaper => Some(wallpaper_intent(wallpaper, connector, dark, warned)),
        Role::Backdrop => backdrop_intent(wallpaper, connector, dark, warned),
    }
}

/// The wallpaper fields as they apply to `connector`, after a matching `[[wallpaper.outputs]]`
/// entry has been merged over the global `[wallpaper]` values. Field by field, not table by
/// table: an override naming only `fit` still takes its image and color from the global table.
struct Effective<'a> {
    image: Option<&'a PathBuf>,
    image_dark: Option<&'a PathBuf>,
    color: &'a str,
    fit: Fit,
    transition: Transition,
    transition_ms: u32,
}

/// The first `[[wallpaper.outputs]]` entry naming `connector`, if any. When two entries name the
/// same connector, the first one in the document wins.
fn find_output<'a>(outputs: &'a [WallpaperOutput], connector: &str) -> Option<&'a WallpaperOutput> {
    outputs.iter().find(|output| output.monitor == connector)
}

fn effective<'a>(wallpaper: &'a Wallpaper, over: Option<&'a WallpaperOutput>) -> Effective<'a> {
    Effective {
        image: over
            .and_then(|over| over.image.as_ref())
            .or(wallpaper.image.as_ref()),
        image_dark: over
            .and_then(|over| over.image_dark.as_ref())
            .or(wallpaper.image_dark.as_ref()),
        color: over
            .and_then(|over| over.color.as_deref())
            .unwrap_or(&wallpaper.color),
        fit: over.and_then(|over| over.fit).unwrap_or(wallpaper.fit),
        transition: over
            .and_then(|over| over.transition)
            .unwrap_or(wallpaper.transition),
        transition_ms: over
            .and_then(|over| over.transition_ms)
            .unwrap_or(wallpaper.transition_ms),
    }
}

fn wallpaper_intent(
    wallpaper: &Wallpaper,
    connector: &str,
    dark: bool,
    warned: &mut MissingWarned,
) -> Intent {
    let over = find_output(&wallpaper.outputs, connector);
    let effective = effective(wallpaper, over);

    let image = resolved_image(
        effective.image,
        effective.image_dark,
        connector,
        dark,
        warned,
    );

    Intent {
        image,
        color: color(effective.color),
        fit: effective.fit,
        transition: effective.transition,
        transition_ms: effective.transition_ms,
        blur_radius: 0,
        downscale_factor: 1,
    }
}

fn backdrop_intent(
    wallpaper: &Wallpaper,
    connector: &str,
    dark: bool,
    warned: &mut MissingWarned,
) -> Option<Intent> {
    let over = find_output(&wallpaper.outputs, connector);
    let effective = effective(wallpaper, over);
    let backdrop = &wallpaper.backdrop;
    let backdrop_over = over.and_then(|over| over.backdrop.as_ref());

    let enabled = backdrop_over
        .and_then(|backdrop_over| backdrop_over.enabled)
        .unwrap_or(backdrop.enabled);
    if !enabled {
        return None;
    }

    let backdrop_image = backdrop_over
        .and_then(|backdrop_over| backdrop_over.image.as_ref())
        .or(backdrop.image.as_ref());
    let backdrop_image_dark = backdrop_over
        .and_then(|backdrop_over| backdrop_over.image_dark.as_ref())
        .or(backdrop.image_dark.as_ref());

    let image = resolved_image(
        backdrop_image.or(effective.image),
        backdrop_image_dark.or(effective.image_dark),
        connector,
        dark,
        warned,
    )?;

    let blur_radius = backdrop_over
        .and_then(|backdrop_over| backdrop_over.blur_radius)
        .unwrap_or(backdrop.blur_radius);

    Some(Intent {
        image: Some(image),
        color: color(effective.color),
        fit: effective.fit,
        transition: effective.transition,
        transition_ms: effective.transition_ms,
        blur_radius,
        downscale_factor: backdrop.downscale_factor,
    })
}

fn resolved_image(
    image: Option<&PathBuf>,
    image_dark: Option<&PathBuf>,
    connector: &str,
    dark: bool,
    warned: &mut MissingWarned,
) -> Option<PathBuf> {
    let raw = match dark {
        true => image_dark.or(image),
        false => image,
    };
    raw.and_then(|raw| {
        let resolved = glimpse_config::resolve_image(raw);
        match &resolved {
            Some(_) => {
                warned.remove(&(connector.to_owned(), raw.clone()));
            }
            None if warned.insert((connector.to_owned(), raw.clone())) => {
                tracing::warn!(connector, path = %raw.display(), "wallpaper image not found");
            }
            None => {}
        }
        resolved
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use glimpse_config::{Backdrop, BackdropOutput};

    use super::*;

    /// Serializes every test that installs a thread-local default subscriber. `tracing`'s
    /// process-wide max-level hint is rebuilt whenever a dispatcher is set or dropped, so two of
    /// these tests racing on different threads can transiently filter each other's events out.
    static LOG_CAPTURE: Mutex<()> = Mutex::new(());

    /// Captures what this module logs on this thread. `set_default` is thread-local, so a test
    /// running on its own thread reports only its own lines into this buffer.
    #[derive(Clone, Default)]
    struct Logged(Arc<Mutex<Vec<u8>>>);

    impl Logged {
        fn lines(&self) -> String {
            let bytes = self.0.lock().expect("not poisoned").clone();
            String::from_utf8(bytes).expect("tracing writes utf-8")
        }

        fn capture(
            &self,
        ) -> (
            std::sync::MutexGuard<'static, ()>,
            tracing::subscriber::DefaultGuard,
        ) {
            let serialize = LOG_CAPTURE
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let sink = self.clone();
            let dispatch = tracing::subscriber::set_default(
                tracing_subscriber::fmt()
                    .with_writer(move || sink.clone())
                    .with_max_level(tracing::Level::WARN)
                    .without_time()
                    .finish(),
            );
            (serialize, dispatch)
        }
    }

    impl std::io::Write for Logged {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("not poisoned").extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_missing_image_warns_once_across_repeated_reconciles() {
        let logged = Logged::default();
        let _guards = logged.capture();

        let wallpaper = Wallpaper {
            image: Some(PathBuf::from("/nonexistent/city.jpg")),
            ..Wallpaper::default()
        };
        let mut warned = MissingWarned::default();

        for _ in 0..3 {
            let result = intent(&wallpaper, "eDP-1", Role::Wallpaper, false, &mut warned)
                .expect("wallpaper role always returns an intent");
            assert_eq!(result.image, None);
        }

        let lines = logged.lines();
        assert_eq!(
            lines.matches("wallpaper image not found").count(),
            1,
            "{lines}"
        );
    }

    #[test]
    fn a_missing_image_warns_again_once_it_has_recovered_and_gone_missing_a_second_time() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("city.jpg");
        let wallpaper = Wallpaper {
            image: Some(file.clone()),
            ..Wallpaper::default()
        };
        let mut warned = MissingWarned::default();

        let logged = Logged::default();
        let _guards = logged.capture();

        intent(&wallpaper, "eDP-1", Role::Wallpaper, false, &mut warned);
        std::fs::write(&file, b"").expect("writes");
        intent(&wallpaper, "eDP-1", Role::Wallpaper, false, &mut warned);
        std::fs::remove_file(&file).expect("removes");
        intent(&wallpaper, "eDP-1", Role::Wallpaper, false, &mut warned);

        let lines = logged.lines();
        assert_eq!(
            lines.matches("wallpaper image not found").count(),
            2,
            "{lines}"
        );
    }

    #[test]
    fn backdrop_intent_is_none_when_disabled() {
        let wallpaper = Wallpaper {
            image: Some(PathBuf::from("city.jpg")),
            backdrop: Backdrop {
                enabled: false,
                ..Backdrop::default()
            },
            ..Wallpaper::default()
        };
        let mut warned = MissingWarned::default();

        assert!(intent(&wallpaper, "eDP-1", Role::Backdrop, false, &mut warned).is_none());
    }

    #[test]
    fn backdrop_intent_is_none_when_enabled_but_no_image_resolves() {
        let wallpaper = Wallpaper {
            image: None,
            backdrop: Backdrop {
                enabled: true,
                ..Backdrop::default()
            },
            ..Wallpaper::default()
        };
        let mut warned = MissingWarned::default();

        assert!(intent(&wallpaper, "eDP-1", Role::Backdrop, false, &mut warned).is_none());
    }

    #[test]
    fn backdrop_intent_derives_its_image_from_the_wallpaper_table() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("city.jpg");
        std::fs::write(&file, b"").expect("writes");

        let wallpaper = Wallpaper {
            image: Some(file.clone()),
            backdrop: Backdrop {
                enabled: true,
                ..Backdrop::default()
            },
            ..Wallpaper::default()
        };
        let mut warned = MissingWarned::default();

        let result = intent(&wallpaper, "eDP-1", Role::Backdrop, false, &mut warned)
            .expect("an image resolved from the wallpaper table");
        assert_eq!(result.image, Some(file));
    }

    #[test]
    fn backdrop_intent_prefers_its_own_image_over_the_wallpaper_table() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let wallpaper_file = dir.path().join("wallpaper.jpg");
        let backdrop_file = dir.path().join("backdrop.jpg");
        std::fs::write(&wallpaper_file, b"").expect("writes");
        std::fs::write(&backdrop_file, b"").expect("writes");

        let wallpaper = Wallpaper {
            image: Some(wallpaper_file),
            backdrop: Backdrop {
                enabled: true,
                image: Some(backdrop_file.clone()),
                ..Backdrop::default()
            },
            ..Wallpaper::default()
        };
        let mut warned = MissingWarned::default();

        let result = intent(&wallpaper, "eDP-1", Role::Backdrop, false, &mut warned)
            .expect("the backdrop's own image resolves");
        assert_eq!(result.image, Some(backdrop_file));
    }

    #[test]
    fn backdrop_intent_derives_its_dark_image_from_the_wallpaper_table() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("city-dark.jpg");
        std::fs::write(&file, b"").expect("writes");

        let wallpaper = Wallpaper {
            image_dark: Some(file.clone()),
            backdrop: Backdrop {
                enabled: true,
                ..Backdrop::default()
            },
            ..Wallpaper::default()
        };
        let mut warned = MissingWarned::default();

        let result = intent(&wallpaper, "eDP-1", Role::Backdrop, true, &mut warned)
            .expect("an image resolved from the wallpaper table");
        assert_eq!(result.image, Some(file));
    }

    #[test]
    fn backdrop_intent_carries_its_own_blur_radius() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("city.jpg");
        std::fs::write(&file, b"").expect("writes");

        let wallpaper = Wallpaper {
            image: Some(file),
            backdrop: Backdrop {
                enabled: true,
                blur_radius: 40,
                ..Backdrop::default()
            },
            ..Wallpaper::default()
        };
        let mut warned = MissingWarned::default();

        let result =
            intent(&wallpaper, "eDP-1", Role::Backdrop, false, &mut warned).expect("resolves");
        assert_eq!(result.blur_radius, 40);
    }

    #[test]
    fn render_key_equality_is_field_by_field() {
        let base = RenderKey {
            image: PathBuf::from("/a.jpg"),
            target: Target {
                width: 100,
                height: 100,
            },
            fit: Fit::Cover,
            blur_radius: 0,
            mtime: None,
        };

        assert_eq!(base, base.clone());
        assert_ne!(
            base,
            RenderKey {
                image: PathBuf::from("/b.jpg"),
                ..base.clone()
            }
        );
        assert_ne!(
            base,
            RenderKey {
                target: Target {
                    width: 200,
                    height: 100
                },
                ..base.clone()
            }
        );
        assert_ne!(
            base,
            RenderKey {
                fit: Fit::Contain,
                ..base.clone()
            }
        );
        assert_ne!(
            base,
            RenderKey {
                blur_radius: 1,
                ..base.clone()
            }
        );
        assert_ne!(
            base,
            RenderKey {
                mtime: Some(SystemTime::UNIX_EPOCH),
                ..base.clone()
            }
        );
    }

    #[test]
    fn an_override_naming_an_absent_connector_is_ignored() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let global_file = dir.path().join("city.jpg");
        let override_file = dir.path().join("other.jpg");
        std::fs::write(&global_file, b"").expect("writes");
        std::fs::write(&override_file, b"").expect("writes");

        let wallpaper = Wallpaper {
            image: Some(global_file.clone()),
            outputs: vec![WallpaperOutput {
                monitor: "DP-99".to_owned(),
                image: Some(override_file),
                ..WallpaperOutput::default()
            }],
            ..Wallpaper::default()
        };
        let mut warned = MissingWarned::default();

        let result = intent(&wallpaper, "eDP-1", Role::Wallpaper, false, &mut warned)
            .expect("wallpaper role always returns an intent");
        assert_eq!(result.image, Some(global_file));
    }

    #[test]
    fn an_override_setting_only_fit_still_takes_image_and_color_from_the_global_table() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("city.jpg");
        std::fs::write(&file, b"").expect("writes");

        let wallpaper = Wallpaper {
            image: Some(file.clone()),
            color: "#336699".to_owned(),
            outputs: vec![WallpaperOutput {
                monitor: "DP-2".to_owned(),
                fit: Some(Fit::Contain),
                ..WallpaperOutput::default()
            }],
            ..Wallpaper::default()
        };
        let mut warned = MissingWarned::default();

        let result = intent(&wallpaper, "DP-2", Role::Wallpaper, false, &mut warned)
            .expect("wallpaper role always returns an intent");
        assert_eq!(result.image, Some(file));
        assert_eq!(result.color, color("#336699"));
        assert_eq!(result.fit, Fit::Contain);
    }

    #[test]
    fn two_overrides_naming_the_same_connector_the_first_one_wins() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let first_file = dir.path().join("first.jpg");
        let second_file = dir.path().join("second.jpg");
        std::fs::write(&first_file, b"").expect("writes");
        std::fs::write(&second_file, b"").expect("writes");

        let wallpaper = Wallpaper {
            outputs: vec![
                WallpaperOutput {
                    monitor: "DP-2".to_owned(),
                    image: Some(first_file.clone()),
                    ..WallpaperOutput::default()
                },
                WallpaperOutput {
                    monitor: "DP-2".to_owned(),
                    image: Some(second_file),
                    ..WallpaperOutput::default()
                },
            ],
            ..Wallpaper::default()
        };
        let mut warned = MissingWarned::default();

        let result = intent(&wallpaper, "DP-2", Role::Wallpaper, false, &mut warned)
            .expect("wallpaper role always returns an intent");
        assert_eq!(result.image, Some(first_file));
    }

    #[test]
    fn an_output_override_reaches_the_backdrop_role_too() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("city.jpg");
        std::fs::write(&file, b"").expect("writes");

        let wallpaper = Wallpaper {
            outputs: vec![WallpaperOutput {
                monitor: "DP-2".to_owned(),
                image: Some(file.clone()),
                backdrop: Some(BackdropOutput {
                    enabled: Some(true),
                    blur_radius: Some(40),
                    ..BackdropOutput::default()
                }),
                ..WallpaperOutput::default()
            }],
            image: None,
            ..Wallpaper::default()
        };
        let mut warned = MissingWarned::default();

        assert!(intent(&wallpaper, "eDP-1", Role::Backdrop, false, &mut warned).is_none());

        let result = intent(&wallpaper, "DP-2", Role::Backdrop, false, &mut warned)
            .expect("the override enables the backdrop and resolves its image");
        assert_eq!(result.image, Some(file));
        assert_eq!(result.blur_radius, 40);
    }
}
