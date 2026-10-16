use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Wallpaper {
    /// Your wallpaper's background color, visible when an image is missing or doesn't fill the
    /// screen.
    pub color: String,
    /// The wallpaper image, used as-is or under the light color scheme when `image-dark` is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<PathBuf>,
    /// The wallpaper image under the dark color scheme. Falls back to `image` when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_dark: Option<PathBuf>,
    /// How your wallpaper image fits the screen.
    pub fit: Fit,
    /// How one image replaces another.
    pub transition: Transition,
    /// Duration of that transition, in milliseconds.
    #[serde(deserialize_with = "transition_ms")]
    #[schemars(range(min = 0, max = 5_000))]
    pub transition_ms: u32,
    pub backdrop: Backdrop,
    /// Separate wallpaper settings for each monitor, matched by connector name such as `DP-1`.
    /// Settings you leave unset use the values above.
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
    /// Center your image without stretching it.
    Center,
}

/// How one wallpaper image replaces another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Transition {
    /// Switch immediately.
    None,
    /// Fade between images.
    Fade,
}

/// Show a second background surface, `glimpse-backdrop`, behind your Overview in `niri`.
/// Add a `place-within-backdrop` layer rule matching this namespace to see it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Backdrop {
    /// Show a separate background in your overview.
    pub enabled: bool,
    /// Source image. Unset derives it from `[wallpaper] image`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<PathBuf>,
    /// Source image under the dark color scheme. Unset derives it from `[wallpaper] image-dark`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_dark: Option<PathBuf>,
    /// How much you blur your overview background, in pixels, from 0 to 128.
    #[serde(deserialize_with = "blur_radius")]
    #[schemars(range(min = 0, max = 128))]
    pub blur_radius: u32,
    /// How much you shrink your overview background before blurring it, from 1 to 16. Higher values
    /// use less memory but lose detail.
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
