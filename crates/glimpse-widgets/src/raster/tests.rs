use std::sync::{Arc, Mutex};

use super::*;

#[test]
fn fill_loads_the_output_exactly_and_never_crops() {
    let plan = plan(
        Target {
            width: 1920,
            height: 1080,
        },
        Target {
            width: 800,
            height: 600,
        },
        Fit::Fill,
    )
    .expect("a plan");
    assert_eq!(
        plan.load,
        Target {
            width: 800,
            height: 600
        }
    );
    assert_eq!(plan.crop, None);
}

#[test]
fn cover_scales_by_the_larger_ratio_and_crops_to_the_output() {
    let plan = plan(
        Target {
            width: 1920,
            height: 1080,
        },
        Target {
            width: 800,
            height: 800,
        },
        Fit::Cover,
    )
    .expect("a plan");
    assert!(plan.load.width >= 800 && plan.load.height >= 800);
    let crop = plan.crop.expect("cover always crops");
    assert_eq!((crop.width, crop.height), (800, 800));
}

#[test]
fn contain_scales_by_the_smaller_ratio_and_never_crops() {
    let plan = plan(
        Target {
            width: 1920,
            height: 1080,
        },
        Target {
            width: 800,
            height: 800,
        },
        Fit::Contain,
    )
    .expect("a plan");
    assert!(plan.load.width <= 800 && plan.load.height <= 800);
    assert_eq!(plan.crop, None);
}

#[test]
fn center_never_enlarges_a_source_smaller_than_the_output() {
    let plan = plan(
        Target {
            width: 64,
            height: 64,
        },
        Target {
            width: 800,
            height: 800,
        },
        Fit::Center,
    )
    .expect("a plan");
    assert_eq!(
        plan.load,
        Target {
            width: 64,
            height: 64
        }
    );
    assert_eq!(plan.crop, None);
}

#[test]
fn center_downscales_a_source_larger_than_the_output() {
    let plan = plan(
        Target {
            width: 1600,
            height: 1600,
        },
        Target {
            width: 800,
            height: 800,
        },
        Fit::Center,
    )
    .expect("a plan");
    assert_eq!(
        plan.load,
        Target {
            width: 800,
            height: 800
        }
    );
}

#[test]
fn a_zero_extent_source_yields_nothing() {
    assert_eq!(
        plan(
            Target {
                width: 0,
                height: 100
            },
            Target {
                width: 800,
                height: 600
            },
            Fit::Cover
        ),
        None
    );
    assert_eq!(
        plan(
            Target {
                width: 100,
                height: -1
            },
            Target {
                width: 800,
                height: 600
            },
            Fit::Cover
        ),
        None
    );
}

#[test]
fn a_zero_extent_output_yields_nothing() {
    assert_eq!(
        plan(
            Target {
                width: 1920,
                height: 1080
            },
            Target {
                width: 0,
                height: 600
            },
            Fit::Cover
        ),
        None
    );
}

#[test]
fn a_huge_reported_output_never_loads_past_the_ceiling() {
    let plan = plan(
        Target {
            width: 1920,
            height: 1080,
        },
        Target {
            width: 100_000,
            height: 100_000,
        },
        Fit::Cover,
    )
    .expect("a plan");
    assert!(plan.load.width <= CEILING);
    assert!(plan.load.height <= CEILING);
}

#[test]
fn the_cover_crop_never_reaches_outside_the_loaded_image() {
    for (source, output) in [
        (
            Target {
                width: 1920,
                height: 1080,
            },
            Target {
                width: 3440,
                height: 1440,
            },
        ),
        (
            Target {
                width: 1080,
                height: 1920,
            },
            Target {
                width: 1920,
                height: 1080,
            },
        ),
        (
            Target {
                width: 7,
                height: 4000,
            },
            Target {
                width: 640,
                height: 480,
            },
        ),
    ] {
        let plan = plan(source, output, Fit::Cover).expect("a plan");
        let crop = plan.crop.expect("cover always crops");
        assert!(crop.x >= 0 && crop.y >= 0);
        assert!(crop.x + crop.width <= plan.load.width);
        assert!(crop.y + crop.height <= plan.load.height);
    }
}

#[test]
fn backdrop_target_divides_a_large_output_and_preserves_aspect() {
    let output = Target {
        width: 3840,
        height: 2160,
    };
    assert_eq!(
        backdrop_target(output, 4),
        Target {
            width: 960,
            height: 540
        }
    );
    assert_eq!(backdrop_target(output, 1), output);
}

#[test]
fn backdrop_target_floors_a_small_output_at_256_and_keeps_its_aspect() {
    let target = backdrop_target(
        Target {
            width: 800,
            height: 600,
        },
        4,
    );
    assert!(target.width.min(target.height) == BACKDROP_FLOOR);
    let source_ratio = 800.0 / 600.0;
    let target_ratio = f64::from(target.width) / f64::from(target.height);
    assert!((source_ratio - target_ratio).abs() < 0.01);
}

#[test]
fn output_target_applies_a_fractional_scale_and_rounds() {
    assert_eq!(
        output_target(2560, 1440, 1.25),
        Target {
            width: 3200,
            height: 1800
        }
    );
}

#[test]
fn output_target_never_exceeds_the_ceiling() {
    let target = output_target(100_000, 100_000, 2.0);
    assert!(target.width <= CEILING);
    assert!(target.height <= CEILING);
}

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
fn color_parses_a_hex_value() {
    let parsed = color("#336699");
    assert!((parsed.red() - 0x33 as f32 / 255.0).abs() < 0.01);
    assert!((parsed.green() - 0x66 as f32 / 255.0).abs() < 0.01);
    assert!((parsed.blue() - 0x99 as f32 / 255.0).abs() < 0.01);
}

#[test]
fn color_parses_a_named_value() {
    let parsed = color("red");
    assert_eq!(
        (parsed.red(), parsed.green(), parsed.blue()),
        (1.0, 0.0, 0.0)
    );
}

#[test]
fn color_falls_back_to_opaque_black_on_garbage() {
    let parsed = color("not a color");
    assert_eq!(parsed, gdk::RGBA::BLACK);
}

#[test]
fn color_logs_exactly_one_warning_on_garbage() {
    let logged = Logged::default();
    let _guards = logged.capture();

    color("not a color");

    let lines = logged.lines();
    assert_eq!(
        lines.matches("wallpaper color did not parse").count(),
        1,
        "{lines}"
    );
}
