use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Lock {
    pub pam_service: String,
    pub background: String,
    pub background_dark: String,
    pub dim: f32,
    pub dim_dark: f32,
    pub clock: Clock,
    pub controls: Controls,
}

impl Default for Lock {
    fn default() -> Self {
        Self {
            pam_service: "glimpse-lock".to_owned(),
            background: String::new(),
            background_dark: String::new(),
            dim: 1.0,
            dim_dark: 1.0,
            clock: Clock::default(),
            controls: Controls::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Button {
    Wifi,
    Input,
    Weather,
    Battery,
    Power,
}
