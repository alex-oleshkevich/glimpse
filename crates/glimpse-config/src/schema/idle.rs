use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Idle {
    /// Whether the idle timers run at all.
    pub enabled: bool,
    /// Pause idle timers while an app keeps your screen awake, such as during a video. Each
    /// listener can override this.
    pub respect_inhibitors: bool,
    /// Your idle actions while plugged in and while on battery.
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
    /// The listeners used on AC power.
    pub ac: Profile,
    /// The listeners used on battery.
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
    /// The actions you run after a period without input. Setting this list replaces the default
    /// actions.
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
    /// Seconds without input before `on-idle` runs. `0` disables the listener.
    pub timeout: u64,
    /// The shell command run when the timeout passes, e.g. `loginctl lock-session`.
    pub on_idle: String,
    /// The shell command run on the next input after `on-idle` ran. Empty runs nothing.
    pub on_resume: String,
    /// Overrides `[idle] respect-inhibitors` for this listener.
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
