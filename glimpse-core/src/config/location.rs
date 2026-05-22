use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum LocationConfig {
    #[default]
    GeoClue,
}

impl std::fmt::Display for LocationConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GeoClue => f.write_str("geoclue"),
        }
    }
}
