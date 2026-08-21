use std::path::PathBuf;

pub struct Config {}

impl Default for Config {
    fn default() -> Self {
        Self {}
    }
}

impl Config {
    pub fn discover() -> Vec<PathBuf> {
        // discover configs in /etc/glimpse/{config.toml, config.d/*.toml}, ~/.config/glimpse/{config.toml, config.d/*.toml}
        vec![]
    }

    pub fn load(override_path: Option<PathBuf>) -> Self {
        let configs = Config::discover();
        // build final merged config here
        Config::default()
    }

    pub fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

pub struct BatteryConfig {}

impl Default for BatteryConfig {
    fn default() -> Self {
        Self {}
    }
}
