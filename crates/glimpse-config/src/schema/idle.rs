use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
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
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Profile {
    pub listeners: Vec<Listener>,
}

impl Profile {
    fn mains() -> Self {
        Self {
            listeners: vec![
                Listener::new(900, "glimpse-dpms off", "glimpse-dpms on"),
                Listener::new(1800, "loginctl lock-session", ""),
                Listener::new(3600, "systemctl suspend", ""),
            ],
        }
    }

    fn battery() -> Self {
        Self {
            listeners: vec![
                Listener::new(900, "glimpse-dpms off", "glimpse-dpms on"),
                Listener::new(1800, "systemctl suspend", ""),
            ],
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
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
