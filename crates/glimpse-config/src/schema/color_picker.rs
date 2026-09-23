use palette::{FromColor, Hsl, Hsv, Oklch, Srgb};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The screen color picker. `glimpse-picker` reads it for the lens and the notation it prints;
/// the panel reads it for the palette, which lives in the panel process and in memory only.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct ColorPicker {
    /// The notation a picked color is copied in, and the one the palette shows it in. Every other
    /// notation stays one click away in the palette.
    pub format: ColorFormat,
    /// How many colors the palette keeps, newest first. A pick beyond it drops the oldest.
    /// Clamped to 1..=64.
    pub limit: usize,
    /// The lens's radius in logical pixels. Clamped to 40..=400.
    pub lens_radius: u32,
    /// How far the scroll wheel zooms the lens in, as a magnification: `30` shows one screen
    /// pixel thirty logical pixels wide. The lens opens at x8, or at this if it is lower.
    /// Clamped to 2..=64.
    pub max_zoom: u32,
}

impl Default for ColorPicker {
    fn default() -> Self {
        Self {
            format: ColorFormat::Hex,
            limit: 8,
            lens_radius: 106,
            max_zoom: 30,
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
mod tests {
    fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, text).expect("writes");
        crate::load(Some(&path))
    }

    #[test]
    fn an_absent_table_copies_hex_and_keeps_eight() {
        let picker = load("").expect("an absent table is fine").color_picker;

        assert_eq!(picker.format, super::ColorFormat::Hex);
        assert_eq!(picker.limit, 8);
        assert_eq!(picker.lens_radius, 106);
        assert_eq!(picker.max_zoom, 30);
    }

    #[test]
    fn every_key_reads_kebab_case() {
        let picker = load(
            "[color-picker]\nformat = \"oklch\"\nlimit = 3\nlens-radius = 150\nmax-zoom = 12\n",
        )
        .expect("kebab-case keys")
        .color_picker;

        assert_eq!(picker.format, super::ColorFormat::Oklch);
        assert_eq!(picker.limit, 3);
        assert_eq!(picker.lens_radius, 150);
        assert_eq!(picker.max_zoom, 12);
    }

    #[test]
    fn every_format_renders_the_known_values_of_a_reference_color() {
        let rendered: Vec<String> = super::ColorFormat::ALL
            .into_iter()
            .map(|format| format.render([0xE0, 0x56, 0x3F]))
            .collect();
        assert_eq!(
            rendered,
            [
                "#E0563F",
                "rgb(224 86 63)",
                "hsl(9 72% 56%)",
                "hsv(9 72% 88%)",
                "oklch(0.63 0.18 32)",
                "cmyk(0% 62% 72% 12%)",
            ]
        );
    }

    #[test]
    fn black_and_white_have_no_hue_to_invent() {
        use super::ColorFormat;
        assert_eq!(
            ColorFormat::Oklch.render([255, 255, 255]),
            "oklch(1.00 0 0)"
        );
        assert_eq!(ColorFormat::Oklch.render([0, 0, 0]), "oklch(0.00 0 0)");
        assert_eq!(ColorFormat::Cmyk.render([0, 0, 0]), "cmyk(0% 0% 0% 100%)");
        assert_eq!(ColorFormat::Hsl.render([255, 255, 255]), "hsl(0 0% 100%)");
        assert_eq!(ColorFormat::Hsl.render([255, 0, 1]), "hsl(0 100% 50%)");
    }

    #[test]
    fn every_format_name_parses_back_to_itself_and_matches_the_document() {
        use super::ColorFormat;
        for format in ColorFormat::ALL {
            assert_eq!(ColorFormat::parse(format.name()), Some(format));
            let written = format!("[color-picker]\nformat = \"{}\"\n", format.name());
            assert_eq!(
                load(&written).expect("a named format").color_picker.format,
                format
            );
        }
        assert_eq!(ColorFormat::parse("HEX"), None);
    }

    #[test]
    fn an_unknown_format_is_refused_by_name() {
        let rendered = load("[color-picker]\nformat = \"lab\"\n")
            .expect_err("`lab` is not a format")
            .to_string();

        assert!(
            rendered.contains("lab"),
            "the error must name `lab`, got {rendered}"
        );
    }
}
