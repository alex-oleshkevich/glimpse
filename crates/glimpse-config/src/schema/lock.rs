use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use super::wallpaper::{Fit, blur_radius};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Lock {
    /// The password-checking rules for your lock screen, from `/etc/pam.d/<name>`.
    pub pam_service: String,
    /// The monitor you enter your password on, by connector name such as `eDP-1`. Use `focused` to
    /// follow your current monitor.
    pub prompt_output: String,
    /// Your lock screen's background image and color.
    pub background: Background,
    /// The time and date on your lock screen.
    pub clock: Clock,
    /// The battery level on your lock screen.
    pub status: Section,
    /// Notifications received while the screen is locked.
    pub notifications: Notifications,
    /// Playback controls on your lock screen.
    pub media: Section,
    /// Power options on your lock screen.
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
    /// Show the clock on your lock screen.
    pub enabled: bool,
    /// A `strftime` pattern for the time, e.g. `%H:%M` or `%-I:%M %p`.
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
    /// Show this item on your lock screen.
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
    /// Show notifications on your lock screen.
    pub enabled: bool,
    /// How much you reveal about notifications: `count` shows a total, and `apps` also shows the
    /// senders. Message text stays hidden.
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
    /// A notification count only.
    Count,
    /// A count and application names.
    Apps,
}

/// The session actions offered from the lock stage.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
#[schemars(rename = "LockSession")]
pub struct Session {
    /// Show power options on your lock screen.
    pub enabled: bool,
    /// The power actions you offer on your lock screen. Actions your system doesn't allow stay
    /// hidden.
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
    /// Put your computer to sleep.
    Suspend,
    /// Restart your computer.
    Reboot,
    /// Power the system off.
    PowerOff,
}

#[cfg(test)]
mod tests;
