use gettextrs::gettext;
use glimpse_config::ColorFormat;
use glimpse_services::PickedColor;
use glimpse_widgets::{Notation, Shade, rgba};

pub const ICON: &str = "color-select-symbolic";

pub fn shades(colors: &[PickedColor], format: ColorFormat) -> Vec<Shade> {
    colors
        .iter()
        .map(|color| Shade {
            id: u64::from(color.id),
            color: rgba(color.rgb),
            title: format.render(color.rgb),
            notations: ColorFormat::ALL
                .into_iter()
                .map(|notation| Notation {
                    key: notation.name().to_owned(),
                    label: notation.label().to_owned(),
                    value: notation.render(color.rgb),
                })
                .collect(),
        })
        .collect()
}

pub fn tooltip(
    latest: Option<&PickedColor>,
    format: ColorFormat,
    template: Option<&str>,
) -> String {
    let Some(color) = latest else {
        return gettext("Pick a color");
    };
    let value = format.render(color.rgb);
    match template {
        Some(template) => template.replace("{color}", &value),
        None => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_titles_in_the_configured_format_and_carries_every_notation() {
        let color = PickedColor {
            id: 7,
            rgb: [224, 86, 63],
        };
        let shades = shades(&[color], ColorFormat::Rgb);

        assert_eq!(shades[0].id, 7);
        assert_eq!(shades[0].title, "rgb(224 86 63)");
        let keys: Vec<_> = shades[0].notations.iter().map(|n| n.key.as_str()).collect();
        assert_eq!(keys, ["hex", "rgb", "hsl", "hsv", "oklch", "cmyk"]);
        assert_eq!(shades[0].notations[0].value, "#E0563F");
    }

    #[test]
    fn the_tooltip_is_the_latest_value_or_an_invitation() {
        let color = PickedColor {
            id: 1,
            rgb: [0, 0, 0],
        };

        assert_eq!(tooltip(None, ColorFormat::Hex, None), "Pick a color");
        assert_eq!(tooltip(Some(&color), ColorFormat::Hex, None), "#000000");
        assert_eq!(
            tooltip(Some(&color), ColorFormat::Hex, Some("Last: {color}")),
            "Last: #000000"
        );
    }
}
