use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Brightness {
    /// The lowest brightness percentage your display can reach, from 0 to 100. Your keyboard
    /// backlight can still reach 0.
    #[serde(deserialize_with = "percent")]
    #[schemars(range(max = 100))]
    pub minimum: u8,
    /// Control the brightness of your external monitors. Changes apply after you restart the panel.
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
