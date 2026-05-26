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
            ac: IdleProfileConfig::default_ac(),
            battery: IdleProfileConfig::default_battery(),
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
    fn default_ac() -> Self {
        Self {
            listeners: vec![
                IdleListenerConfig::new(
                    600,
                    "/usr/share/glimpse/scripts/monitors off",
                    "/usr/share/glimpse/scripts/monitors on",
                ),
                IdleListenerConfig::new(900, "loginctl lock-session", ""),
                IdleListenerConfig::new(3600, "systemctl suspend", ""),
            ],
        }
    }

    fn default_battery() -> Self {
        Self {
            listeners: vec![
                IdleListenerConfig::new(
                    300,
                    "/usr/share/glimpse/scripts/monitors off",
                    "/usr/share/glimpse/scripts/monitors on",
                ),
                IdleListenerConfig::new(900, "loginctl lock-session", ""),
                IdleListenerConfig::new(1800, "systemctl suspend", ""),
            ],
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
    fn default_idle_config_pins_the_ladder() {
        let config = IdleConfig::default();

        assert!(config.enabled);
        assert!(config.respect_inhibitors);

        let ac_steps: Vec<_> = config
            .profiles
            .ac
            .listeners
            .iter()
            .map(|l| (l.timeout, l.on_idle.as_str(), l.on_resume.as_str()))
            .collect();
        assert_eq!(
            ac_steps,
            vec![
                (
                    600,
                    "/usr/share/glimpse/scripts/monitors off",
                    "/usr/share/glimpse/scripts/monitors on",
                ),
                (900, "loginctl lock-session", ""),
                (3600, "systemctl suspend", ""),
            ]
        );

        let battery_steps: Vec<_> = config
            .profiles
            .battery
            .listeners
            .iter()
            .map(|l| (l.timeout, l.on_idle.as_str(), l.on_resume.as_str()))
            .collect();
        assert_eq!(
            battery_steps,
            vec![
                (
                    300,
                    "/usr/share/glimpse/scripts/monitors off",
                    "/usr/share/glimpse/scripts/monitors on",
                ),
                (900, "loginctl lock-session", ""),
                (1800, "systemctl suspend", ""),
            ]
        );
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
