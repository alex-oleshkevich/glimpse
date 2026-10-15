use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Appearance {
    /// The theme directory to load stylesheets from, looked up under `~/.config/glimpse/themes`
    /// first and the installed themes second.
    pub theme: String,
    /// A CSS class added to every window, so one theme can ship several looks. Letters, digits,
    /// `-` and `_` only, not starting with a digit; anything else is ignored with a warning.
    pub theme_variant: String,
    /// Whether glimpse's own surfaces are light or dark. `auto` follows the desktop mode that
    /// `auto-dark` decides; `light` and `dark` hold glimpse in one scheme whatever the desktop does.
    pub color_scheme: ColorScheme,
    /// What decides the desktop mode: `auto` turns dark at sunset and light at sunrise where you
    /// are, `system` follows the color scheme preference set elsewhere, for example in GNOME Settings.
    pub auto_dark: AutoDark,
    /// Which parts of glimpse blur what is behind them: `panel`, `popover`, `notification`, or
    /// `all`. Needs compositor support; without it they stay solid. Setup notes:
    /// https://alex-oleshkevich.github.io/glimpse/config/services/appearance#blur
    pub blur: Vec<BlurSurface>,
    /// How fast every animation runs: `1.0` is normal, `2.0` twice as fast, `0.5` half as fast,
    /// and `0` turns animations off. Anything else below `0.1` is refused, because a fade that
    /// slow leaves the panel ignoring clicks while it runs. The popover and notification fades and every CSS transition
    /// that reads `--gl-duration` follow it; the wallpaper keeps its own `transition-ms`.
    #[serde(deserialize_with = "speed")]
    #[schemars(range(min = 0.0, max = 10.0))]
    pub animation_speed: f64,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: "adwaita".to_owned(),
            theme_variant: String::new(),
            color_scheme: ColorScheme::Auto,
            auto_dark: AutoDark::Auto,
            blur: Vec::new(),
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
#[serde(rename_all = "kebab-case")]
pub enum ColorScheme {
    Light,
    Dark,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AutoDark {
    Auto,
    System,
}

impl From<&crate::Config> for AutoDark {
    fn from(document: &crate::Config) -> Self {
        document.appearance.auto_dark
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BlurSurface {
    Panel,
    Popover,
    Notification,
    All,
}

pub fn blurs(selected: &[BlurSurface], surface: BlurSurface) -> bool {
    selected
        .iter()
        .any(|entry| *entry == BlurSurface::All || *entry == surface)
}

#[cfg(test)]
mod tests;
