use std::sync::{Arc, Mutex};

use glimpse_config::{Backdrop, BackdropOutput};

use super::*;

static LOG_CAPTURE: Mutex<()> = Mutex::new(());

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

    let result = intent(&wallpaper, "eDP-1", Role::Backdrop, false, &mut warned).expect("resolves");
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
