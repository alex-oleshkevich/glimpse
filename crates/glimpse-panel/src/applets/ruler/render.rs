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
mod tests;
