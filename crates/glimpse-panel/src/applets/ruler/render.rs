use gettextrs::gettext;
use glimpse_services::RulerMeasurement as Measurement;
use glimpse_widgets::HistoryEntry;

pub const ICON: &str = "selection-mode-symbolic";

pub fn history(entries: &[Measurement]) -> Vec<HistoryEntry> {
    entries
        .iter()
        .map(|measurement| HistoryEntry {
            id: u64::from(measurement.id),
            title: format!("{:.1}px", measurement.distance),
            value: format!(
                "{} × {} px · {:.1}°",
                measurement.dx.unsigned_abs(),
                measurement.dy.unsigned_abs(),
                measurement.angle
            ),
        })
        .collect()
}

pub fn tooltip(latest: Option<&Measurement>, template: Option<&str>) -> String {
    let Some(measurement) = latest else {
        return gettext("Measure the screen");
    };
    let value = format!("{:.1}px", measurement.distance);
    match template {
        Some(template) => template.replace("{measurement}", &value),
        None => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measurement(id: u32, distance: f64) -> Measurement {
        Measurement {
            id,
            from_x: 0,
            from_y: 0,
            to_x: 10,
            to_y: 10,
            dx: 10,
            dy: -10,
            distance,
            angle: 45.0,
        }
    }

    #[test]
    fn a_history_row_titles_the_distance_and_carries_the_offset() {
        let entries = history(&[measurement(3, 141.4)]);

        assert_eq!(entries[0].id, 3);
        assert_eq!(entries[0].title, "141.4px");
        assert_eq!(entries[0].value, "10 × 10 px · 45.0°");
    }

    #[test]
    fn the_tooltip_is_the_latest_distance_or_an_invitation() {
        let measurement = measurement(1, 12.3);

        assert_eq!(tooltip(None, None), "Measure the screen");
        assert_eq!(tooltip(Some(&measurement), None), "12.3px");
        assert_eq!(
            tooltip(Some(&measurement), Some("Last: {measurement}")),
            "Last: 12.3px"
        );
    }
}
