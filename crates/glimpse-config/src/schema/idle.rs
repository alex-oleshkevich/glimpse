use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Idle {
    /// Whether the idle timers run at all.
    pub enabled: bool,
    /// Whether an app holding the screen awake, such as a video player or a browser playing a
    /// video, pauses the listeners. A listener's own `respect-inhibitors` overrides this.
    pub respect_inhibitors: bool,
    /// The listeners to run on AC power and on battery. The battery profile applies while UPower
    /// reports the machine on battery; without UPower the AC profile always applies.
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
    /// Each listener runs a command once the session has been idle for its `timeout`. Setting this
    /// list replaces the default listeners rather than adding to them.
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
