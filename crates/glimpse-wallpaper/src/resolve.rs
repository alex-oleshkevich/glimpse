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
mod tests;
