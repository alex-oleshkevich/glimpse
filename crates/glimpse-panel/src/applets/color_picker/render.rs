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
mod tests;
