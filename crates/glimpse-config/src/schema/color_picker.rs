use palette::{FromColor, Hsl, Hsv, Oklch, Srgb};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The screen color picker. The panel's color picker reads it for the notation it copies and for
/// the palette, which lives in the panel process and in memory only. The lens is configured by
/// its own flags.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct ColorPicker {
    /// The notation a picked color is copied in, and the one the palette shows it in. Every other
    /// notation stays one click away in the palette.
    pub format: ColorFormat,
    /// How many colors the palette keeps, newest first. A pick beyond it drops the oldest.
    /// Clamped to 1..=64.
    pub limit: usize,
}

impl Default for ColorPicker {
    fn default() -> Self {
        Self {
            format: ColorFormat::Hex,
            limit: 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ColorFormat {
    /// `#E0563F`
    Hex,
    /// `rgb(224 86 63)`
    Rgb,
    /// `hsl(9 72% 56%)`
    Hsl,
    /// `hsv(9 72% 88%)`
    Hsv,
    /// `oklch(0.63 0.18 32)`
    Oklch,
    /// `cmyk(0% 62% 72% 12%)`, a naive conversion with no color profile.
    Cmyk,
}

impl ColorFormat {
    pub const ALL: [Self; 6] = [
        Self::Hex,
        Self::Rgb,
        Self::Hsl,
        Self::Hsv,
        Self::Oklch,
        Self::Cmyk,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Hex => "hex",
            Self::Rgb => "rgb",
            Self::Hsl => "hsl",
            Self::Hsv => "hsv",
            Self::Oklch => "oklch",
            Self::Cmyk => "cmyk",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Hex => "HEX",
            Self::Rgb => "RGB",
            Self::Hsl => "HSL",
            Self::Hsv => "HSV",
            Self::Oklch => "OKLCH",
            Self::Cmyk => "CMYK",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|format| format.name() == name)
    }

    pub fn render(self, [red, green, blue]: [u8; 3]) -> String {
        let color = Srgb::new(red, green, blue).into_format::<f32>();
        match self {
            Self::Hex => format!("#{red:02X}{green:02X}{blue:02X}"),
            Self::Rgb => format!("rgb({red} {green} {blue})"),
            Self::Hsl => {
                let hsl = Hsl::from_color(color);
                format!(
                    "hsl({} {}% {}%)",
                    degrees(hsl.hue.into_positive_degrees()),
                    percent(hsl.saturation),
                    percent(hsl.lightness)
                )
            }
            Self::Hsv => {
                let hsv = Hsv::from_color(color);
                format!(
                    "hsv({} {}% {}%)",
                    degrees(hsv.hue.into_positive_degrees()),
                    percent(hsv.saturation),
                    percent(hsv.value)
                )
            }
            Self::Oklch => {
                let oklch = Oklch::from_color(color.into_linear::<f32>());
                if oklch.chroma < 0.005 {
                    format!("oklch({:.2} 0 0)", oklch.l)
                } else {
                    format!(
                        "oklch({:.2} {:.2} {})",
                        oklch.l,
                        oklch.chroma,
                        degrees(oklch.hue.into_positive_degrees())
                    )
                }
            }
            Self::Cmyk => {
                let key = 1.0 - color.red.max(color.green).max(color.blue);
                let ink = |channel: f32| match key >= 1.0 {
                    true => 0,
                    false => percent((1.0 - channel - key) / (1.0 - key)),
                };
                format!(
                    "cmyk({}% {}% {}% {}%)",
                    ink(color.red),
                    ink(color.green),
                    ink(color.blue),
                    percent(key)
                )
            }
        }
    }
}

fn degrees(value: f32) -> u32 {
    (value.round() as u32) % 360
}

fn percent(value: f32) -> u32 {
    (value * 100.0).round().clamp(0.0, 100.0) as u32
}

#[cfg(test)]
mod tests;
