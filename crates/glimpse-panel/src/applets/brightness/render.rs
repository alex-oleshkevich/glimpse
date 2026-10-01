use gettextrs::gettext;
use glimpse_services::{BrightnessKind, BrightnessSource, OutputInfo};

pub const CHIP_ICON: &str = "display-brightness-symbolic";
pub const NAME_CAP: usize = 32;

pub fn chip(has_sources: bool, night_light_reachable: bool) -> Option<&'static str> {
    (has_sources || night_light_reachable).then_some(CHIP_ICON)
}

pub fn is_powered(source: &BrightnessSource, outputs: &[OutputInfo]) -> bool {
    let Some(connector) = source.connector.as_deref() else {
        return true;
    };
    outputs
        .iter()
        .find(|output| output.connector == connector)
        .is_none_or(|output| output.enabled)
}

pub fn current_display<'a>(
    sources: &'a [BrightnessSource],
    focused: Option<&str>,
) -> Option<&'a BrightnessSource> {
    let displays: Vec<&BrightnessSource> = sources
        .iter()
        .filter(|source| source.kind == BrightnessKind::Display)
        .collect();

    if let Some(focused) = focused
        && let Some(found) = displays
            .iter()
            .find(|source| source.connector.as_deref() == Some(focused))
    {
        return Some(found);
    }

    let mut internal = displays.iter().filter(|source| source.connector.is_none());
    if let Some(only) = internal.next()
        && internal.next().is_none()
    {
        return Some(only);
    }

    displays.first().copied()
}

pub fn ordered_sources<'a>(
    sources: &'a [BrightnessSource],
    focused: Option<&str>,
    show_keyboard: bool,
    pinned: Option<&str>,
) -> Vec<&'a BrightnessSource> {
    let current = pinned
        .and_then(|id| sources.iter().find(|source| source.id == id))
        .or_else(|| current_display(sources, focused));
    let mut ordered = Vec::with_capacity(sources.len());
    ordered.extend(current);
    for source in sources {
        if current.is_some_and(|picked| picked.id == source.id) {
            continue;
        }
        if source.kind == BrightnessKind::Keyboard && !show_keyboard {
            continue;
        }
        ordered.push(source);
    }
    ordered
}

pub fn source_name(source: &BrightnessSource, outputs: &[OutputInfo]) -> String {
    let name = match source.kind {
        BrightnessKind::Keyboard => return gettext("Keyboard"),
        BrightnessKind::Display => source
            .connector
            .as_deref()
            .and_then(|connector| {
                outputs
                    .iter()
                    .find(|output| output.connector == connector)
                    .and_then(|output| output.label.clone())
            })
            .or_else(|| source.connector.clone())
            .unwrap_or_default(),
    };
    glimpse_utils::clean(&name, NAME_CAP)
}

pub fn percent_of(current: u32, max: u32) -> u32 {
    if max == 0 {
        return 0;
    }
    ((current as f64 / max as f64) * 100.0).round() as u32
}

pub fn tooltip(name: Option<&str>, percent: u32, format: Option<&str>) -> String {
    let percent_text = percent.to_string();
    let name = name.filter(|value| !value.is_empty());

    if let Some(format) = format {
        return crate::applets::tokens::render(format, |token| match token {
            "percent" => Some(percent_text.as_str()),
            "name" => name,
            _ => None,
        });
    }

    match name {
        Some(name) => gettext("{name}: {percent}%")
            .replace("{name}", name)
            .replace("{percent}", &percent_text),
        None => gettext("{percent}%").replace("{percent}", &percent_text),
    }
}

pub fn switch_on(schedule: &str) -> bool {
    schedule != "off"
}

pub fn night_mode<'a>(schedule: &'a str, configured: &'a str) -> &'a str {
    [schedule, configured]
        .into_iter()
        .find(|mode| switch_on(mode))
        .unwrap_or("automatic")
}

pub fn source_icon(kind: BrightnessKind) -> &'static str {
    match kind {
        BrightnessKind::Keyboard => "keyboard-brightness-symbolic",
        BrightnessKind::Display => "display-brightness-symbolic",
    }
}

pub fn native_step(percent: u8, max: u32) -> u32 {
    if percent == 0 || max == 0 {
        return 0;
    }
    let raw = (f64::from(percent) / 100.0) * f64::from(max);
    raw.ceil().max(1.0) as u32
}

#[derive(Debug, Default)]
pub struct Coalescer<T> {
    in_flight: bool,
    pending: Option<T>,
}

impl<T> Coalescer<T> {
    pub fn new() -> Self {
        Self {
            in_flight: false,
            pending: None,
        }
    }

    /// A new value to apply. `Some` is the value to send now; `None` means a call is already in
    /// flight and this one was folded into `pending` instead.
    pub fn request(&mut self, value: T) -> Option<T> {
        if self.in_flight {
            self.pending = Some(value);
            return None;
        }
        self.in_flight = true;
        Some(value)
    }

    /// The in-flight call has settled. `Some` is the coalesced value to send next, and the
    /// caller is again in flight for it; `None` means nothing arrived while it was busy.
    pub fn completed(&mut self) -> Option<T> {
        self.in_flight = false;
        match self.pending.take() {
            Some(value) => {
                self.in_flight = true;
                Some(value)
            }
            None => None,
        }
    }
}

#[cfg(test)]
mod tests;
