use std::path::Path;
use std::time::SystemTime;

use glimpse_config::Fit;
use gtk4::gdk_pixbuf::{InterpType, Pixbuf};
use gtk4::prelude::*;
use gtk4::{gdk, glib};

pub const CEILING: i32 = 8192;
const BACKDROP_DIVISOR: i32 = 4;
const BACKDROP_FLOOR: i32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Target {
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub load: Target,
    pub crop: Option<Region>,
}

#[derive(Debug, Clone)]
pub struct Raster {
    pub bytes: glib::Bytes,
    pub width: i32,
    pub height: i32,
    pub rowstride: usize,
    pub has_alpha: bool,
    pub mtime: SystemTime,
}

pub fn output_target(width: i32, height: i32, scale: f64) -> Target {
    clamp(Target {
        width: (f64::from(width) * scale).round() as i32,
        height: (f64::from(height) * scale).round() as i32,
    })
}

pub fn plan(source: Target, output: Target, fit: Fit) -> Option<Plan> {
    if source.width <= 0 || source.height <= 0 || output.width <= 0 || output.height <= 0 {
        return None;
    }

    let horizontal = f64::from(output.width) / f64::from(source.width);
    let vertical = f64::from(output.height) / f64::from(source.height);

    let load = clamp(match fit {
        Fit::Fill => output,
        Fit::Cover => scaled(source, horizontal.max(vertical)),
        Fit::Contain => scaled(source, horizontal.min(vertical)),
        Fit::Center => scaled(source, horizontal.min(vertical).min(1.0)),
    });

    let crop = match fit {
        Fit::Cover => Some(Region {
            x: ((load.width - output.width) / 2).max(0),
            y: ((load.height - output.height) / 2).max(0),
            width: load.width.min(output.width),
            height: load.height.min(output.height),
        }),
        Fit::Fill | Fit::Contain | Fit::Center => None,
    };

    Some(Plan { load, crop })
}

pub(crate) fn backdrop_target(output: Target) -> Target {
    let divided = Target {
        width: (output.width / BACKDROP_DIVISOR).max(1),
        height: (output.height / BACKDROP_DIVISOR).max(1),
    };
    let shorter = divided.width.min(divided.height);
    if shorter >= BACKDROP_FLOOR {
        return clamp(divided);
    }
    clamp(scaled(
        divided,
        f64::from(BACKDROP_FLOOR) / f64::from(shorter),
    ))
}

fn scaled(source: Target, ratio: f64) -> Target {
    Target {
        width: ((f64::from(source.width) * ratio).round() as i32).max(1),
        height: ((f64::from(source.height) * ratio).round() as i32).max(1),
    }
}

fn clamp(target: Target) -> Target {
    Target {
        width: target.width.clamp(1, CEILING),
        height: target.height.clamp(1, CEILING),
    }
}

pub fn raster(path: &Path, output: Target, fit: Fit, blur_radius: u32) -> Option<Raster> {
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
    let (_, width, height) = Pixbuf::file_info(path)?;
    let plan = plan(Target { width, height }, output, fit)?;

    let loaded = Pixbuf::from_file_at_scale(path, plan.load.width, plan.load.height, false).ok()?;
    let cropped = match plan.crop {
        Some(region) => loaded.new_subpixbuf(region.x, region.y, region.width, region.height),
        None => loaded,
    };
    let blurred = blur(&cropped, blur_radius)?;
    Some(Raster {
        bytes: blurred.read_pixel_bytes(),
        width: blurred.width(),
        height: blurred.height(),
        rowstride: blurred.rowstride().max(0) as usize,
        has_alpha: blurred.has_alpha(),
        mtime,
    })
}

fn blur(image: &Pixbuf, radius: u32) -> Option<Pixbuf> {
    if radius == 0 {
        return Some(image.clone());
    }
    let factor = (radius as i32).max(1) + 1;
    let width = (image.width() / factor).max(1);
    let height = (image.height() / factor).max(1);
    image
        .scale_simple(width, height, InterpType::Bilinear)?
        .scale_simple(image.width(), image.height(), InterpType::Bilinear)
}

pub fn texture(raster: &Raster) -> gdk::Texture {
    gdk::MemoryTexture::new(
        raster.width,
        raster.height,
        match raster.has_alpha {
            true => gdk::MemoryFormat::R8g8b8a8,
            false => gdk::MemoryFormat::R8g8b8,
        },
        &raster.bytes,
        raster.rowstride,
    )
    .upcast()
}

pub fn solid(color: gdk::RGBA) -> gdk::Texture {
    let pixel: [u8; 4] = [
        (color.red() * 255.0).round() as u8,
        (color.green() * 255.0).round() as u8,
        (color.blue() * 255.0).round() as u8,
        (color.alpha() * 255.0).round() as u8,
    ];
    gdk::MemoryTexture::new(
        1,
        1,
        gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from(&pixel[..]),
        4,
    )
    .upcast()
}

pub fn content_fit(fit: Fit) -> gtk4::ContentFit {
    match fit {
        Fit::Fill | Fit::Cover => gtk4::ContentFit::Fill,
        Fit::Contain => gtk4::ContentFit::Contain,
        Fit::Center => gtk4::ContentFit::ScaleDown,
    }
}

#[cfg(test)]
mod tests {
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
        let target = backdrop_target(Target {
            width: 3840,
            height: 2160,
        });
        assert_eq!(
            target,
            Target {
                width: 960,
                height: 540
            }
        );
    }

    #[test]
    fn backdrop_target_floors_a_small_output_at_256_and_keeps_its_aspect() {
        let target = backdrop_target(Target {
            width: 800,
            height: 600,
        });
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
}
