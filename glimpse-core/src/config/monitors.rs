use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct MonitorsConfig {
    pub builtin_connector: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::MonitorsConfig;

    #[test]
    fn monitors_config_defaults_to_none() {
        let config = MonitorsConfig::default();
        assert!(config.builtin_connector.is_none());
    }

    #[test]
    fn monitors_config_parses_builtin_connector_override() {
        let config: MonitorsConfig = toml::from_str(
            r#"
builtin_connector = "eDP-1"
"#,
        )
        .expect("monitors config should parse");

        assert_eq!(config.builtin_connector.as_deref(), Some("eDP-1"));
    }
}
