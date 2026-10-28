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

#[test]
fn a_cover_plan_past_the_ceiling_keeps_the_source_and_output_aspects() {
    let source = Target {
        width: 1080,
        height: 2400,
    };
    let output = Target {
        width: 3840,
        height: 2160,
    };
    let plan = plan(source, output, Fit::Cover).expect("a plan");
    let crop = plan.crop.expect("cover always crops");

    let aspect = |width: i32, height: i32| f64::from(width) / f64::from(height);
    assert!(
        (aspect(plan.load.width, plan.load.height) - aspect(source.width, source.height)).abs()
            < 0.01
    );
    assert!((aspect(crop.width, crop.height) - aspect(output.width, output.height)).abs() < 0.01);
}

#[test]
fn a_blurred_copy_keeps_the_aspect_and_is_never_enlarged() {
    assert_eq!(blurred_size(3840, 2160, 480), (480.0, 270.0));
    assert_eq!(blurred_size(300, 200, 480), (300.0, 200.0));
    assert_eq!(blurred_size(1000, 1, 10), (10.0, 1.0));
    assert_eq!(blurred_size(100, 100, 0), (1.0, 1.0));
}

#[test]
fn the_middle_of_an_axis_is_the_image_itself() {
    let axis = axis(0, 480.0, 30.0);
    assert_eq!((axis.start, axis.length), (0.0, 480.0));
    assert_eq!((axis.shift, axis.scale, axis.origin), (0.0, 1.0, 0.0));
}

fn world(axis: &Axis, local: f32) -> f32 {
    axis.shift + axis.scale * local
}

#[test]
fn the_border_cells_mirror_the_image_across_its_edge() {
    let (size, bleed) = (480.0_f32, 30.0_f32);

    let low = axis(-1, size, bleed);
    assert_eq!((low.start, low.length), (-bleed, bleed));
    assert_eq!(
        world(&low, low.origin),
        0.0,
        "the image's first column stays at the edge"
    );
    assert_eq!(
        world(&low, low.origin + size),
        -size,
        "and the rest folds outwards"
    );

    let high = axis(1, size, bleed);
    assert_eq!((high.start, high.length), (size, bleed));
    assert_eq!(
        world(&high, high.origin + size),
        size,
        "the last column stays at the edge"
    );
    assert_eq!(
        world(&high, high.origin),
        2.0 * size,
        "and the rest folds outwards"
    );
}

#[test]
fn every_tile_is_no_larger_than_the_image() {
    for side in [-1, 0, 1] {
        let axis = axis(side, 480.0, 30.0);
        assert!(axis.length <= 480.0);
        assert!(
            axis.scale.abs() == 1.0,
            "no tile is stretched, so none exceeds the image"
        );
    }
}

#[test]
fn a_bleed_wider_than_the_image_is_cut_to_it_so_no_gap_opens() {
    let low = axis(-1, 10.0, 30.0);
    assert_eq!((low.start, low.length), (-10.0, 10.0));
    let high = axis(1, 10.0, 30.0);
    assert_eq!((high.start, high.length), (10.0, 10.0));
    assert_eq!(
        world(&high, high.origin),
        20.0,
        "the mirrored tile still ends at the cell"
    );
}
