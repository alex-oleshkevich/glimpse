use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::wallpaper::Fit;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Lock {
    pub pam_service: String,
    pub css_path: PathBuf,
    pub background: Background,
    pub clock: Clock,
    pub controls: Controls,
}

impl Default for Lock {
    fn default() -> Self {
        Self {
            pam_service: "glimpse-lock".to_owned(),
            css_path: PathBuf::from("themes/lock.css"),
            background: Background::default(),
            clock: Clock::default(),
            controls: Controls::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Background {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<Fit>,
    pub blur_radius: u32,
    pub dim: f32,
}

impl Default for Background {
    fn default() -> Self {
        Self {
            color: None,
            path: None,
            fit: None,
            blur_radius: 0,
            dim: 0.35,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Clock {
    pub enabled: bool,
    pub time_format: String,
    pub date_format: String,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            enabled: true,
            time_format: "%H:%M".to_owned(),
            date_format: "%A, %B %-d".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Controls {
    pub buttons: Vec<Button>,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            buttons: vec![
                Button::Wifi,
                Button::Input,
                Button::Weather,
                Button::Battery,
                Button::Power,
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    Wifi,
    Input,
    Weather,
    Battery,
    Power,
}
