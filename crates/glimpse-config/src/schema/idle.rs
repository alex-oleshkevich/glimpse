use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Idle {
    pub enabled: bool,
    pub respect_inhibitors: bool,
    pub profiles: Profiles,
}

impl Default for Idle {
    fn default() -> Self {
        Self {
            enabled: true,
            respect_inhibitors: true,
            profiles: Profiles::default(),
        }
    }
}

fn monitors(state: &str) -> String {
    format!("{}/scripts/monitors {state}", crate::DATA_DIR)
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Profiles {
    pub ac: Profile,
    pub battery: Profile,
}

impl Default for Profiles {
    fn default() -> Self {
        Self {
            ac: Profile::mains(),
            battery: Profile::battery(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Profile {
    pub listeners: Vec<Listener>,
}

impl Profile {
    fn mains() -> Self {
        Self {
            listeners: vec![
                Listener::new(600, &monitors("off"), &monitors("on")),
                Listener::new(900, "loginctl lock-session", ""),
                Listener::new(3600, "systemctl suspend", ""),
            ],
        }
    }

    fn battery() -> Self {
        Self {
            listeners: vec![
                Listener::new(300, &monitors("off"), &monitors("on")),
                Listener::new(900, "loginctl lock-session", ""),
                Listener::new(1800, "systemctl suspend", ""),
            ],
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Listener {
    pub timeout: u64,
    pub on_idle: String,
    pub on_resume: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub respect_inhibitors: Option<bool>,
}

impl Listener {
    fn new(timeout: u64, on_idle: &str, on_resume: &str) -> Self {
        Self {
            timeout,
            on_idle: on_idle.to_owned(),
            on_resume: on_resume.to_owned(),
            respect_inhibitors: None,
        }
    }
}
