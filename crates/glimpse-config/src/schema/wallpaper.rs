use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
    pub transition_ms: u32,
    pub backdrop: Backdrop,
}

impl Default for Wallpaper {
    fn default() -> Self {
        Self {
            color: "#000000".to_owned(),
            image: None,
            image_dark: None,
            fit: Fit::Cover,
            transition: Transition::Fade,
            transition_ms: 500,
            backdrop: Backdrop::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Fit {
    Cover,
    Contain,
    Fill,
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
    pub blur_radius: u32,
}

impl Default for Backdrop {
    fn default() -> Self {
        Self {
            enabled: false,
            image: None,
            image_dark: None,
            blur_radius: 24,
        }
    }
}
