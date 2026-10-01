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
mod tests;
