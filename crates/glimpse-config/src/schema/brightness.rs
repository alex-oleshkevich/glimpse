use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Brightness {
    /// The lowest percent a display's backlight is allowed to reach; the keyboard backlight ignores
    /// it and may still go to 0. Every write glimpse makes to a display source is clamped here,
    /// because a screen driven all the way dark is indistinguishable from one that has failed.
    #[serde(deserialize_with = "percent")]
    #[schemars(range(max = 100))]
    pub minimum: u8,
    /// Whether to probe external displays for DDC/CI brightness control over `/dev/i2c-*`. DDC/CI
    /// is protocol-mandated slow (tens of milliseconds per read or write) and not every monitor
    /// implements it correctly, so this stays a document-level escape hatch rather than something
    /// a user has to diagnose by unplugging a display.
    pub ddc: bool,
}

impl Default for Brightness {
    fn default() -> Self {
        Self {
            minimum: 3,
            ddc: true,
        }
    }
}

fn percent<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u8::deserialize(deserializer)?;
    (value <= 100)
        .then_some(value)
        .ok_or_else(|| D::Error::custom("must be between 0 and 100 percent"))
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
    fn the_document_sets_the_floor() {
        let parsed = load("[brightness]\nminimum = 10\n")
            .expect("minimum is a key of this table")
            .brightness;

        assert_eq!(parsed.minimum, 10);
    }

    #[test]
    fn an_absent_table_defaults_to_three() {
        let parsed = load("").expect("an absent table is fine").brightness;

        assert_eq!(parsed.minimum, 3);
        assert!(parsed.ddc, "DDC/CI probing defaults to on");
    }

    #[test]
    fn ddc_can_be_turned_off() {
        let parsed = load("[brightness]\nddc = false\n")
            .expect("ddc is a key of this table")
            .brightness;

        assert!(!parsed.ddc);
    }

    #[test]
    fn the_cadence_knobs_udev_removed_are_not_keys_of_this_table() {
        let rendered = load("[brightness]\npoll-interval = 5\n")
            .expect_err("`poll-interval` is not a key of this table")
            .to_string();

        assert!(
            rendered.contains("poll-interval") && rendered.contains("brightness"),
            "the error must name the table and `poll-interval`, got {rendered}"
        );
    }

    #[test]
    fn a_floor_above_the_hardware_maximum_is_refused() {
        let rendered = load("[brightness]\nminimum = 200\n")
            .expect_err("200 is not a percent")
            .to_string();

        assert!(
            rendered.contains("minimum"),
            "the error must name `minimum`, got {rendered}"
        );
    }
}
