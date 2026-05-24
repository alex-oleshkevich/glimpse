use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct IdleConfig {
    pub enabled: bool,
    pub respect_inhibitors: bool,
    pub profiles: IdleProfilesConfig,
}

impl Default for IdleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            respect_inhibitors: true,
            profiles: IdleProfilesConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct IdleProfilesConfig {
    pub ac: IdleProfileConfig,
    pub battery: IdleProfileConfig,
}

impl Default for IdleProfilesConfig {
    fn default() -> Self {
        Self {
            ac: IdleProfileConfig::with_default_lock_listener(),
            battery: IdleProfileConfig::with_default_lock_listener(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct IdleProfileConfig {
    pub listeners: Vec<IdleListenerConfig>,
}

impl Default for IdleProfileConfig {
    fn default() -> Self {
        Self { listeners: vec![] }
    }
}

impl IdleProfileConfig {
    fn with_default_lock_listener() -> Self {
        Self {
            listeners: vec![IdleListenerConfig::new(900, "loginctl lock-session", "")],
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct IdleListenerConfig {
    pub timeout: u64,
    pub on_idle: String,
    pub on_resume: String,
    pub respect_inhibitors: Option<bool>,
}

impl IdleListenerConfig {
    pub fn new(timeout: u64, on_idle: impl Into<String>, on_resume: impl Into<String>) -> Self {
        Self {
            timeout,
            on_idle: on_idle.into(),
            on_resume: on_resume.into(),
            respect_inhibitors: None,
        }
    }
}

impl Default for IdleListenerConfig {
    fn default() -> Self {
        Self {
            timeout: 0,
            on_idle: String::new(),
            on_resume: String::new(),
            respect_inhibitors: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::IdleConfig;

    #[test]
    fn default_idle_config_locks_after_fifteen_minutes() {
        let config = IdleConfig::default();

        assert!(config.enabled);
        assert!(config.respect_inhibitors);
        assert_eq!(config.profiles.ac.listeners.len(), 1);
        assert_eq!(config.profiles.battery.listeners.len(), 1);

        let ac = &config.profiles.ac.listeners[0];
        assert_eq!(ac.timeout, 900);
        assert_eq!(ac.on_idle, "loginctl lock-session");
        assert_eq!(ac.on_resume, "");
        assert_eq!(ac.respect_inhibitors, None);

        let battery = &config.profiles.battery.listeners[0];
        assert_eq!(battery.timeout, 900);
        assert_eq!(battery.on_idle, "loginctl lock-session");
        assert_eq!(battery.on_resume, "");
        assert_eq!(battery.respect_inhibitors, None);
    }

    #[test]
    fn listener_config_parses_optional_inhibitor_override() {
        let config: IdleConfig = toml::from_str(
            r#"
enabled = true
respect_inhibitors = true

[profiles.ac]
listeners = [
  { timeout = 10, on_idle = "notify-send idle", on_resume = "notify-send resume", respect_inhibitors = false },
]

[profiles.battery]
listeners = [
  { timeout = 5, on_idle = "notify-send battery" },
]
"#,
        )
        .expect("idle config should parse");

        assert_eq!(config.profiles.ac.listeners[0].timeout, 10);
        assert_eq!(
            config.profiles.ac.listeners[0].respect_inhibitors,
            Some(false)
        );
        assert_eq!(config.profiles.battery.listeners[0].on_resume, "");
    }
}
