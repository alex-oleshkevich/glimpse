use chrono::{DateTime, Local};
use gettextrs::gettext;
use glimpse_config::ColorFormat;
use glimpse_services::PickedColor;
use glimpse_widgets::{Notation, Shade, rgba};

pub const ICON: &str = "color-select-symbolic";

pub fn when(now: DateTime<Local>, picked: DateTime<Local>, twelve_hour: bool) -> String {
    let minutes = now.signed_duration_since(picked).num_minutes();
    if let Some(recent) = crate::applets::ago::within_an_hour(minutes) {
        return recent;
    }
    let days = (now.date_naive() - picked.date_naive()).num_days();
    match days {
        0 => picked
            .format(glimpse_config::clock(twelve_hour))
            .to_string(),
        1 => gettext("yesterday"),
        _ => picked.format("%Y-%m-%d").to_string(),
    }
}

pub fn shades(
    colors: &[PickedColor],
    format: ColorFormat,
    now: DateTime<Local>,
    twelve_hour: bool,
) -> Vec<Shade> {
    colors
        .iter()
        .map(|color| Shade {
            id: u64::from(color.id),
            color: rgba(color.rgb),
            title: format.render(color.rgb),
            subtitle: when(now, color.picked_at.with_timezone(&Local), twelve_hour),
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
    use chrono::TimeZone;

    use super::*;

    fn at(day: u32, hour: u32, minute: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 9, day, hour, minute, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn a_pick_is_described_by_how_long_ago_it_was() {
        let now = at(23, 15, 42);

        assert_eq!(when(now, at(23, 15, 42), false), "just now");
        assert_eq!(when(now, at(23, 15, 38), false), "4 minutes ago");
        assert_eq!(when(now, at(23, 15, 41), false), "1 minute ago");
        assert_eq!(when(now, at(23, 9, 5), false), "09:05");
        assert_eq!(when(now, at(23, 9, 5), true), "9:05 AM");
        assert_eq!(when(now, at(22, 20, 0), false), "yesterday");
        assert_eq!(when(now, at(20, 20, 0), false), "2026-09-20");
    }

    #[test]
    fn a_clock_that_ran_backwards_reads_as_just_now() {
        assert_eq!(when(at(23, 15, 0), at(23, 16, 0), false), "just now");
    }

    #[test]
    fn a_row_titles_in_the_configured_format_and_carries_every_notation() {
        let color = PickedColor {
            id: 7,
            rgb: [224, 86, 63],
            picked_at: at(23, 15, 0).with_timezone(&chrono::Utc),
        };
        let shades = shades(&[color], ColorFormat::Rgb, at(23, 15, 0), false);

        assert_eq!(shades[0].id, 7);
        assert_eq!(shades[0].title, "rgb(224 86 63)");
        assert_eq!(shades[0].subtitle, "just now");
        let keys: Vec<_> = shades[0].notations.iter().map(|n| n.key.as_str()).collect();
        assert_eq!(keys, ["hex", "rgb", "hsl", "hsv", "oklch", "cmyk"]);
        assert_eq!(shades[0].notations[0].value, "#E0563F");
    }

    #[test]
    fn the_tooltip_is_the_latest_value_or_an_invitation() {
        let color = PickedColor {
            id: 1,
            rgb: [0, 0, 0],
            picked_at: chrono::Utc::now(),
        };

        assert_eq!(tooltip(None, ColorFormat::Hex, None), "Pick a color");
        assert_eq!(tooltip(Some(&color), ColorFormat::Hex, None), "#000000");
        assert_eq!(
            tooltip(Some(&color), ColorFormat::Hex, Some("Last: {color}")),
            "Last: #000000"
        );
    }
}
