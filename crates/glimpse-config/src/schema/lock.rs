use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use super::wallpaper::{Fit, blur_radius};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Lock {
    pub pam_service: String,
    /// `"focused"` follows the compositor's focused output; anything else is a connector name.
    pub prompt_output: String,
    pub background: Background,
    pub clock: Clock,
    /// The status island: weather, battery, keyboard layout, bluetooth and network.
    pub status: Section,
    /// Notifications received while the screen is locked.
    pub notifications: Notifications,
    /// The now-playing transport, when something is playing.
    pub media: Section,
    /// The suspend, reboot and power-off actions offered from the lock stage.
    pub session: Session,
}

impl Default for Lock {
    fn default() -> Self {
        Self {
            pam_service: "glimpse-lock".to_owned(),
            prompt_output: "focused".to_owned(),
            background: Background::default(),
            clock: Clock::default(),
            status: Section::default(),
            notifications: Notifications::default(),
            media: Section::default(),
            session: Session::default(),
        }
    }
}

/// The lock stage's background: an image over a color, blurred and dimmed.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
#[schemars(rename = "LockBackground")]
pub struct Background {
    /// The background image, used as-is or under the light color scheme when `image-dark` is set.
    /// With neither this nor `image-dark` set, the lock shows `[wallpaper]`'s `image` and
    /// `image-dark`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<PathBuf>,
    /// The background image under the dark color scheme. Falls back to `image` when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_dark: Option<PathBuf>,
    /// Painted under everything: the whole background when no image resolves, and what shows
    /// through `contain`'s letterbox.
    pub color: String,
    /// How the image meets an output of a different aspect ratio.
    pub fit: Fit,
    /// Gaussian blur radius in output pixels; 0 blurs nothing.
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
    pub enabled: bool,
    pub time_format: String,
    /// `{day}` is the day of the month with its ordinal suffix, e.g. "3rd".
    pub date_format: String,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            enabled: true,
            time_format: "%H:%M".to_owned(),
            date_format: "%A, {day} %B".to_owned(),
        }
    }
}

/// A lock stage element with nothing to configure besides whether it shows at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
#[schemars(rename = "LockSection")]
pub struct Section {
    /// Whether this element shows on the lock stage at all.
    pub enabled: bool,
}

impl Default for Section {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Which notifications reach the lock stage, and how much of each one is shown.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
#[schemars(rename = "LockNotifications")]
pub struct Notifications {
    /// Whether notifications show on the lock stage at all.
    pub enabled: bool,
    /// A locked screen never shows a notification's body. `count` shows only a total; `apps`
    /// also names which applications sent them.
    pub privacy: Privacy,
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            enabled: true,
            privacy: Privacy::Apps,
        }
    }
}

/// How much of a notification's own content shows on the lock stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(rename = "LockPrivacy")]
pub enum Privacy {
    /// Only a total count, naming nothing about any individual notification.
    Count,
    /// Also names which application each notification came from, never a summary or body.
    Apps,
}

/// The session actions offered from the lock stage.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
#[schemars(rename = "LockSession")]
pub struct Session {
    /// Whether the session actions show on the lock stage at all.
    pub enabled: bool,
    /// Every action here is also gated by logind: one it refuses does not appear even when listed.
    pub actions: Vec<SessionAction>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            enabled: true,
            actions: vec![
                SessionAction::Suspend,
                SessionAction::Reboot,
                SessionAction::PowerOff,
            ],
        }
    }
}

/// One system action offered from the lock stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(rename = "LockSessionAction")]
pub enum SessionAction {
    /// Suspend the system.
    Suspend,
    /// Reboot the system.
    Reboot,
    /// Power the system off.
    PowerOff,
}

#[cfg(test)]
mod tests;
