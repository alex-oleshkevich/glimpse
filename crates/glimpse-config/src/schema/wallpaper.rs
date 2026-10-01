use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Wallpaper {
    /// Painted under everything: the whole wallpaper when no image resolves, and what shows
    /// through `contain`'s letterbox.
    pub color: String,
    /// The wallpaper image, used as-is or under the light color scheme when `image-dark` is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<PathBuf>,
    /// The wallpaper image under the dark color scheme. Falls back to `image` when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_dark: Option<PathBuf>,
    /// How the image meets an output of a different aspect ratio.
    pub fit: Fit,
    /// How one image replaces another.
    pub transition: Transition,
    /// Duration of that transition, in milliseconds.
    #[serde(deserialize_with = "transition_ms")]
    #[schemars(range(min = 0, max = 5_000))]
    pub transition_ms: u32,
    pub backdrop: Backdrop,
    /// Per-output overrides, matched against the connector `gdk::Monitor::connector()` returns
    /// (the same string `[[panels]].monitor` matches against). An output with no entry here
    /// renders the values above. When two entries name the same connector, the first one wins.
    pub outputs: Vec<WallpaperOutput>,
}

fn default_image() -> PathBuf {
    Path::new(crate::load::DATA_DIR)
        .join(crate::theme::THEMES_DIR)
        .join(crate::theme::DEFAULT_THEME)
        .join("default.jpg")
}

impl Default for Wallpaper {
    fn default() -> Self {
        Self {
            color: "#000000".to_owned(),
            image: Some(default_image()),
            image_dark: None,
            fit: Fit::Cover,
            transition: Transition::Fade,
            transition_ms: 500,
            backdrop: Backdrop::default(),
            outputs: Vec::new(),
        }
    }
}

/// A per-output override of the wallpaper (and backdrop) shown on one connector. Every field but
/// `monitor` is optional and falls back to the corresponding value in `[wallpaper]`, or
/// `[wallpaper.backdrop]` for the nested `backdrop` fields, when unset.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct WallpaperOutput {
    /// The connector this override applies to, e.g. `"DP-2"`.
    #[serde(deserialize_with = "required_monitor")]
    pub monitor: String,
    /// Overrides `[wallpaper] color` for this connector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Overrides `[wallpaper] image` for this connector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<PathBuf>,
    /// Overrides `[wallpaper] image-dark` for this connector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_dark: Option<PathBuf>,
    /// Overrides `[wallpaper] fit` for this connector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<Fit>,
    /// Overrides `[wallpaper] transition` for this connector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<Transition>,
    /// Overrides `[wallpaper] transition-ms` for this connector.
    #[serde(default, deserialize_with = "optional_transition_ms")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 5_000))]
    pub transition_ms: Option<u32>,
    /// Overrides `[wallpaper.backdrop]` for this connector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backdrop: Option<BackdropOutput>,
}

/// A per-output override of `[wallpaper.backdrop]`. Every field is optional and falls back to
/// the corresponding value in `[wallpaper.backdrop]` when unset.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct BackdropOutput {
    /// Overrides `[wallpaper.backdrop] enabled` for this connector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Overrides `[wallpaper.backdrop] image` for this connector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<PathBuf>,
    /// Overrides `[wallpaper.backdrop] image-dark` for this connector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_dark: Option<PathBuf>,
    /// Overrides `[wallpaper.backdrop] blur-radius` for this connector.
    #[serde(default, deserialize_with = "optional_blur_radius")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 128))]
    pub blur_radius: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Fit {
    Cover,
    Contain,
    Fill,
    /// The image at its own size, centred on color, downscaled only when it exceeds the
    /// output. contain enlarges a small image and looks soft; this is the honest option.
    Center,
}

/// How one wallpaper image replaces another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Transition {
    /// A cut, with no animation.
    None,
    /// A crossfade between the outgoing and incoming image.
    Fade,
}

/// A second background surface, named `glimpse-backdrop`, shown only where the compositor has
/// somewhere to place it — niri's Overview, through a `place-within-backdrop` layer rule.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Backdrop {
    pub enabled: bool,
    /// Source image. Unset derives it from `[wallpaper] image`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<PathBuf>,
    /// Source image under the dark color scheme. Unset derives it from `[wallpaper] image-dark`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_dark: Option<PathBuf>,
    /// Gaussian blur radius in output pixels; 0 blurs nothing.
    #[serde(deserialize_with = "blur_radius")]
    #[schemars(range(min = 0, max = 128))]
    pub blur_radius: u32,
    /// Divides the output's physical size to get the size the backdrop is decoded at; 1 decodes
    /// at full size. The shorter side never drops below 256 pixels.
    #[serde(deserialize_with = "downscale_factor")]
    #[schemars(range(min = 1, max = 16))]
    pub downscale_factor: u32,
}

impl Default for Backdrop {
    fn default() -> Self {
        Self {
            enabled: true,
            image: None,
            image_dark: None,
            blur_radius: 24,
            downscale_factor: 4,
        }
    }
}

fn transition_ms<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u32::deserialize(deserializer)?;
    (0..=5_000)
        .contains(&value)
        .then_some(value)
        .ok_or_else(|| D::Error::custom("must be between 0 and 5000 milliseconds"))
}

pub(crate) fn blur_radius<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u32::deserialize(deserializer)?;
    (0..=128)
        .contains(&value)
        .then_some(value)
        .ok_or_else(|| D::Error::custom("must be between 0 and 128 pixels"))
}

fn downscale_factor<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u32::deserialize(deserializer)?;
    (1..=16)
        .contains(&value)
        .then_some(value)
        .ok_or_else(|| D::Error::custom("must be between 1 and 16"))
}

fn optional_transition_ms<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<u32>::deserialize(deserializer)?;
    match value {
        Some(value) if !(0..=5_000).contains(&value) => {
            Err(D::Error::custom("must be between 0 and 5000 milliseconds"))
        }
        value => Ok(value),
    }
}

fn optional_blur_radius<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<u32>::deserialize(deserializer)?;
    match value {
        Some(value) if !(0..=128).contains(&value) => {
            Err(D::Error::custom("must be between 0 and 128 pixels"))
        }
        value => Ok(value),
    }
}

fn required_monitor<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    if value.is_empty() || value.trim() != value {
        return Err(D::Error::custom(
            "monitor must be a non-empty exact connector name without surrounding whitespace",
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests;
