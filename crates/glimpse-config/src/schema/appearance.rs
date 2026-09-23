use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Appearance {
    pub theme: String,
    /// A CSS class added to every window, so one theme can ship several looks. Letters, digits,
    /// `-` and `_` only, not starting with a digit; anything else is ignored with a warning.
    pub theme_variant: String,
    pub color_scheme: ColorScheme,
    /// Surfaces that blur whatever lies behind them, where the compositor offers
    /// ext-background-effect-v1; elsewhere they stay opaque. The strength of the blur is the
    /// compositor's setting, not glimpse's. niri blurs a layer surface in xray mode unless told
    /// otherwise, which shows the wallpaper rather than the windows under a popover or a
    /// notification; `layer-rule { match namespace="^glimpse-(popover|notifications)$";
    /// background-effect { xray false; } }` in the niri config changes that.
    pub blur: Vec<BlurSurface>,
    /// How fast every animation runs: `1.0` is normal, `2.0` twice as fast, `0.5` half as fast,
    /// and `0` turns animations off. Anything else below `0.1` is refused, because a fade that
    /// long keeps the full-screen popover catcher swallowing clicks. The popover and notification fades and every CSS transition
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
pub enum BlurSurface {
    Panel,
    Popover,
    Notification,
}

#[cfg(test)]
mod tests {
    use super::{Appearance, BlurSurface};

    #[test]
    fn blur_names_the_surfaces_it_applies_to() {
        let parsed: Appearance =
            toml::from_str("blur = [\"panel\", \"notification\"]\n").expect("the surfaces parse");

        assert_eq!(parsed.blur, [BlurSurface::Panel, BlurSurface::Notification]);
    }

    #[test]
    fn blur_is_off_until_asked_for() {
        assert!(Appearance::default().blur.is_empty());
    }

    #[test]
    fn animation_speed_is_normal_until_changed_and_zero_is_allowed() {
        assert_eq!(Appearance::default().animation_speed, 1.0);
        let off: Appearance = toml::from_str("animation-speed = 0\n").expect("zero parses");
        assert_eq!(off.animation_speed, 0.0);
        let fast: Appearance =
            toml::from_str("animation-speed = 1.5\n").expect("a fraction parses");
        assert_eq!(fast.animation_speed, 1.5);
    }

    #[test]
    fn animation_speed_rejects_a_negative_a_crawl_an_absurd_and_a_non_number() {
        for text in [
            "animation-speed = -1\n",
            "animation-speed = 11\n",
            "animation-speed = nan\n",
            "animation-speed = 0.01\n",
        ] {
            assert!(toml::from_str::<Appearance>(text).is_err(), "{text} parsed");
        }
    }

    #[test]
    fn blur_rejects_a_surface_it_does_not_know() {
        assert!(toml::from_str::<Appearance>("blur = [\"lock\"]\n").is_err());
    }
}
