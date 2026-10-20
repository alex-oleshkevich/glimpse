use std::path::Path;
use std::time::SystemTime;

use anyhow::{Context, Result};
use glimpse_config::Fit;
use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::prelude::*;
use gtk4::{gdk, glib, graphene, gsk};

const CEILING: i32 = 8192;
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

    let wanted = match fit {
        Fit::Fill => output,
        Fit::Cover => scaled(source, horizontal.max(vertical)),
        Fit::Contain => scaled(source, horizontal.min(vertical)),
        Fit::Center => scaled(source, horizontal.min(vertical).min(1.0)),
    };
    let shrink = match fit {
        Fit::Cover => (f64::from(CEILING) / f64::from(wanted.width.max(wanted.height))).min(1.0),
        Fit::Fill | Fit::Contain | Fit::Center => 1.0,
    };
    let load = clamp(scaled(wanted, shrink));
    let view = scaled(output, shrink);

    let crop = match fit {
        Fit::Cover => Some(Region {
            x: ((load.width - view.width) / 2).max(0),
            y: ((load.height - view.height) / 2).max(0),
            width: load.width.min(view.width),
            height: load.height.min(view.height),
        }),
        Fit::Fill | Fit::Contain | Fit::Center => None,
    };

    Some(Plan { load, crop })
}

pub fn backdrop_target(output: Target, downscale_factor: u32) -> Target {
    let divisor = i32::try_from(downscale_factor).unwrap_or(i32::MAX).max(1);
    let divided = Target {
        width: (output.width / divisor).max(1),
        height: (output.height / divisor).max(1),
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

pub fn raster(path: &Path, output: Target, fit: Fit) -> Result<Raster> {
    let mtime = std::fs::metadata(path)?.modified()?;
    let (_, width, height) =
        Pixbuf::file_info(path).context("no image loader could read the header")?;
    let plan = plan(Target { width, height }, output, fit).context("image has no size")?;

    let loaded = Pixbuf::from_file_at_scale(path, plan.load.width, plan.load.height, false)?;
    let cropped = match plan.crop {
        Some(region) => loaded.new_subpixbuf(region.x, region.y, region.width, region.height),
        None => loaded,
    };
    Ok(Raster {
        bytes: cropped.read_pixel_bytes(),
        width: cropped.width(),
        height: cropped.height(),
        rowstride: cropped.rowstride().max(0) as usize,
        has_alpha: cropped.has_alpha(),
        mtime,
    })
}

pub fn blur_sigma(radius: u32, texture_width: i32, output_width: i32) -> Option<f32> {
    (radius > 0 && output_width > 0)
        .then(|| radius as f32 * texture_width as f32 / output_width as f32)
}

pub fn blurred(
    renderer: &gsk::Renderer,
    texture: &gdk::Texture,
    sigma: f32,
) -> Option<gdk::Texture> {
    let (width, height) = (texture.width() as f32, texture.height() as f32);
    let bounds = graphene::Rect::new(0.0, 0.0, width, height);
    let bleed = sigma * 3.0;
    let snapshot = gtk4::Snapshot::new();
    snapshot.push_clip(&bounds);
    snapshot.push_blur(f64::from(sigma));
    snapshot.append_texture(
        texture,
        &graphene::Rect::new(-bleed, -bleed, width + 2.0 * bleed, height + 2.0 * bleed),
    );
    snapshot.pop();
    snapshot.pop();
    Some(renderer.render_texture(snapshot.to_node()?, Some(&bounds)))
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

pub fn color(text: &str) -> gdk::RGBA {
    match gdk::RGBA::parse(text) {
        Ok(color) => color,
        Err(_) => {
            tracing::warn!(
                color = text,
                "wallpaper color did not parse; using opaque black"
            );
            gdk::RGBA::BLACK
        }
    }
}

#[cfg(test)]
mod tests;
