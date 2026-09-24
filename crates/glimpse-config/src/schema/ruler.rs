use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The screen ruler. `glimpse-ruler` reads it for the lens and the history it keeps.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Ruler {
    /// How many measurements history keeps, newest first. A measurement beyond it drops the
    /// oldest. Clamped to 1..=64.
    pub history_limit: usize,
    /// The lens's radius in logical pixels. Clamped to 40..=400.
    pub lens_radius: u32,
    /// How far the scroll wheel zooms the lens in, as a magnification. Clamped to 2..=64.
    pub max_zoom: u32,
}

impl Default for Ruler {
    fn default() -> Self {
        Self {
            history_limit: 8,
            lens_radius: 106,
            max_zoom: 30,
        }
    }
}

#[cfg(test)]
mod tests {
    fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, text).expect("writes");
        crate::load(Some(&path))
    }

    #[test]
    fn an_absent_table_uses_the_documented_defaults() {
        let ruler = load("").expect("an absent table is fine").ruler;

        assert_eq!(ruler.history_limit, 8);
        assert_eq!(ruler.lens_radius, 106);
        assert_eq!(ruler.max_zoom, 30);
    }

    #[test]
    fn every_key_reads_kebab_case() {
        let ruler = load("[ruler]\nhistory-limit = 3\nlens-radius = 150\nmax-zoom = 12\n")
            .expect("kebab-case keys")
            .ruler;

        assert_eq!(ruler.history_limit, 3);
        assert_eq!(ruler.lens_radius, 150);
        assert_eq!(ruler.max_zoom, 12);
    }

    #[test]
    fn an_unknown_key_is_refused_naming_the_table_and_the_key() {
        let rendered = load("[ruler]\nbogus = 1\n")
            .expect_err("`bogus` is not a ruler setting")
            .to_string();

        assert!(
            rendered.contains("ruler") && rendered.contains("bogus"),
            "the error must name the table and the key, got {rendered}"
        );
    }
}
