use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use super::wallpaper::{Fit, blur_radius};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Lock {
    /// The monitor you enter your password on, by connector name such as `eDP-1`. Use `focused` to
    /// follow your current monitor.
    pub prompt_output: String,
    /// Your lock screen's background image and color.
    pub background: Background,
    /// The time and date on your lock screen.
    pub clock: Clock,
}

impl Default for Lock {
    fn default() -> Self {
        Self {
            prompt_output: "focused".to_owned(),
            background: Background::default(),
            clock: Clock::default(),
        }
    }
}

/// The lock stage's background: an image over a color, blurred and dimmed.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
#[schemars(rename = "LockBackground")]
pub struct Background {
    /// Your lock screen's background image. If you leave both this and `image-dark` unset, it uses
    /// your wallpaper.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<PathBuf>,
    /// The background image under the dark color scheme. Falls back to `image` when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_dark: Option<PathBuf>,
    /// The background color you see when an image is missing or doesn't fill your screen.
    pub color: String,
    /// How your image fits the screen.
    pub fit: Fit,
    /// How much you blur your background, in pixels; `0` leaves it sharp.
    #[serde(deserialize_with = "blur_radius")]
    #[schemars(range(min = 0, max = 128))]
    pub blur_radius: u32,
    /// How much the background is darkened, from 0 (untouched) to 1 (black).
    #[serde(deserialize_with = "dim")]
    #[schemars(range(min = 0.0, max = 1.0))]
    pub dim: f64,
    /// Overrides `dim` under the dark color scheme.
    #[serde(deserialize_with = "dim")]
    #[schemars(range(min = 0.0, max = 1.0))]
    pub dim_dark: f64,
}

impl Default for Background {
    fn default() -> Self {
        Self {
            image: None,
            image_dark: None,
            color: "#000000".to_owned(),
            fit: Fit::Cover,
            blur_radius: 0,
            dim: 0.35,
            dim_dark: 0.5,
        }
    }
}

fn dim<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = f64::deserialize(deserializer)?;
    (0.0..=1.0)
        .contains(&value)
        .then_some(value)
        .ok_or_else(|| D::Error::custom("must be between 0.0 and 1.0"))
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Clock {
    /// A strftime pattern, e.g. `%-d` for the day of the month without padding.
    pub date_format: String,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            date_format: "%A, %-d %B".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests;
