use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Appearance {
    /// Choose a theme from `~/.config/glimpse/themes` or the installed themes.
    pub theme: String,
    /// Choose an alternate look provided by your theme.
    pub theme_variant: String,
    /// Choose `desktop` to follow your desktop mode, or `light` or `dark` to keep glimpse in one mode.
    pub color_scheme: ColorScheme,
    /// Choose `sun` to turn your desktop dark at sunset and light at sunrise, `light` or `dark` to keep it in one mode, or `external` to let another tool decide.
    /// Without a known location, `sun` leaves your desktop mode alone.
    pub desktop_color_scheme: DesktopColorScheme,
    /// Blur what is behind the bar, popovers, notifications and dialogs. Your compositor must support blur.
    pub blur: bool,
    /// Set your animation speed from 0.1 to 10, or `0` to turn animations off.
    /// Use `2.0` for twice as fast or `0.5` for half as fast; your wallpaper uses `transition-ms` instead.
    #[serde(deserialize_with = "speed")]
    #[schemars(range(min = 0.0, max = 10.0))]
    pub animation_speed: f64,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: "adwaita".to_owned(),
            theme_variant: String::new(),
            color_scheme: ColorScheme::Dark,
            desktop_color_scheme: DesktopColorScheme::Sun,
            blur: false,
            animation_speed: 1.0,
        }
    }
}

fn speed<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = f64::deserialize(deserializer)?;
    (value == 0.0 || (0.1..=10.0).contains(&value))
        .then_some(value)
        .ok_or_else(|| D::Error::custom("must be 0, or between 0.1 and 10"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case", try_from = "String")]
#[schemars(!try_from)]
pub enum ColorScheme {
    Light,
    Dark,
    Desktop,
}

impl TryFrom<String> for ColorScheme {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "desktop" => Ok(Self::Desktop),
            "light" => Ok(Self::Light),
            "dark" => Ok(Self::Dark),
            _ => Err("color-scheme must be desktop, light or dark".into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DesktopColorScheme {
    Sun,
    Light,
    Dark,
    External,
}

impl From<&crate::Config> for DesktopColorScheme {
    fn from(document: &crate::Config) -> Self {
        document.appearance.desktop_color_scheme
    }
}

#[cfg(test)]
mod tests;
