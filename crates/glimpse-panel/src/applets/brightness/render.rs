use gettextrs::gettext;
use glimpse_services::{BrightnessKind, BrightnessSource, OutputInfo};

pub const CHIP_ICON: &str = "display-brightness-symbolic";
pub const NAME_CAP: usize = 32;

pub fn chip(has_sources: bool, night_light_reachable: bool) -> Option<&'static str> {
    (has_sources || night_light_reachable).then_some(CHIP_ICON)
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
mod tests {
    use super::*;

    fn source(id: &str, kind: BrightnessKind, connector: Option<&str>) -> BrightnessSource {
        BrightnessSource {
            id: id.to_owned(),
            kind,
            connector: connector.map(str::to_owned),
            current: 50,
            max: 100,
            floor: 0,
        }
    }

    fn output(connector: &str, label: Option<&str>) -> OutputInfo {
        OutputInfo {
            connector: connector.to_owned(),
            label: label.map(str::to_owned),
            built_in: false,
            focused: false,
            make: None,
            model: None,
            serial: None,
            current_mode: None,
            logical: None,
            enabled: true,
        }
    }

    #[test]
    fn the_chip_renders_with_a_source_and_no_night_light() {
        assert_eq!(chip(true, false), Some(CHIP_ICON));
    }

    #[test]
    fn the_chip_renders_with_no_source_but_a_reachable_night_light() {
        assert_eq!(
            chip(false, true),
            Some(CHIP_ICON),
            "AC-1: a desktop with no backlight at all is ordinary, not an error"
        );
    }

    #[test]
    fn the_chip_is_gone_with_neither_offering() {
        assert_eq!(chip(false, false), None, "AC-2");
    }

    #[test]
    fn current_display_prefers_the_focused_output_connector() {
        let sources = vec![
            source("edp", BrightnessKind::Display, Some("eDP-1")),
            source("dp", BrightnessKind::Display, Some("DP-2")),
        ];
        let picked = current_display(&sources, Some("DP-2")).expect("a display exists");
        assert_eq!(
            picked.id, "dp",
            "AC-3 rung 1: the focused output's connector wins"
        );
    }

    #[test]
    fn current_display_falls_back_to_the_single_internal_source() {
        let sources = vec![
            source("edp", BrightnessKind::Display, None),
            source("dp", BrightnessKind::Display, Some("DP-2")),
        ];
        let picked = current_display(&sources, Some("HDMI-A-1")).expect("a display exists");
        assert_eq!(
            picked.id, "edp",
            "AC-3 rung 2: no source matches the focused connector, but exactly one has none at \
             all, which can only be the internal panel"
        );
    }

    #[test]
    fn current_display_falls_back_to_the_first_display_source() {
        let sources = vec![
            source("dp1", BrightnessKind::Display, Some("DP-1")),
            source("dp2", BrightnessKind::Display, Some("DP-2")),
        ];
        let picked = current_display(&sources, None).expect("a display exists");
        assert_eq!(
            picked.id, "dp1",
            "AC-3 rung 3: nothing internal and nothing focused, so the first display wins"
        );
    }

    #[test]
    fn current_display_is_none_without_a_single_display_source() {
        let sources = vec![source("keyboard", BrightnessKind::Keyboard, None)];
        assert_eq!(
            current_display(&sources, Some("eDP-1")),
            None,
            "AC-3 last rung: a keyboard-only state resolves no current display"
        );
    }

    #[test]
    fn current_display_does_not_treat_two_connectorless_sources_as_the_single_internal_one() {
        let sources = vec![
            source("a", BrightnessKind::Display, None),
            source("b", BrightnessKind::Display, None),
        ];
        let picked = current_display(&sources, Some("nowhere")).expect("a display exists");
        assert_eq!(
            picked.id, "a",
            "two connectorless sources are ambiguous, so rung 2 does not apply and the first \
             display source rung decides instead"
        );
    }

    #[test]
    fn ordered_sources_puts_the_current_display_first() {
        let sources = vec![
            source("dp1", BrightnessKind::Display, Some("DP-1")),
            source("dp2", BrightnessKind::Display, Some("DP-2")),
            source("keyboard", BrightnessKind::Keyboard, None),
        ];
        let ordered = ordered_sources(&sources, Some("DP-2"), true, None);
        assert_eq!(
            ordered
                .iter()
                .map(|source| source.id.as_str())
                .collect::<Vec<_>>(),
            vec!["dp2", "dp1", "keyboard"]
        );
    }

    #[test]
    fn ordered_sources_drops_the_keyboard_when_it_is_configured_off() {
        let sources = vec![
            source("dp1", BrightnessKind::Display, Some("DP-1")),
            source("keyboard", BrightnessKind::Keyboard, None),
        ];
        let ordered = ordered_sources(&sources, None, false, None);
        assert_eq!(ordered.len(), 1);
        assert_eq!(ordered[0].id, "dp1");
    }

    #[test]
    fn ordered_sources_keeps_a_pinned_source_first_even_after_focus_moves() {
        let sources = vec![
            source("dp1", BrightnessKind::Display, Some("DP-1")),
            source("dp2", BrightnessKind::Display, Some("DP-2")),
        ];
        let ordered = ordered_sources(&sources, Some("DP-2"), true, Some("dp1"));
        assert_eq!(
            ordered[0].id, "dp1",
            "a source pinned when the popover opened must stay the primary fader even though \
             DP-2 is now focused, or a press-to-release focus change would write to the wrong \
             display"
        );
    }

    #[test]
    fn ordered_sources_falls_back_to_the_ladder_when_the_pinned_source_is_gone() {
        let sources = vec![source("dp1", BrightnessKind::Display, Some("DP-1"))];
        let ordered = ordered_sources(&sources, None, true, Some("unplugged"));
        assert_eq!(
            ordered[0].id, "dp1",
            "a pinned source that has since disappeared must not leave the primary fader empty"
        );
    }

    #[test]
    fn source_name_reads_the_output_label_by_connector() {
        let sources = source("dp", BrightnessKind::Display, Some("DP-2"));
        let outputs = vec![output("DP-2", Some("Dell U2720Q"))];
        assert_eq!(source_name(&sources, &outputs), "Dell U2720Q");
    }

    #[test]
    fn source_name_falls_back_to_the_bare_connector_without_a_label() {
        let sources = source("dp", BrightnessKind::Display, Some("DP-2"));
        assert_eq!(source_name(&sources, &[]), "DP-2");
    }

    #[test]
    fn source_name_names_the_keyboard_regardless_of_outputs() {
        let sources = source("keyboard", BrightnessKind::Keyboard, None);
        assert_eq!(source_name(&sources, &[]), gettext("Keyboard"));
    }

    #[test]
    fn a_hostile_compositor_supplied_label_is_capped_before_it_reaches_a_label() {
        let sources = source("dp", BrightnessKind::Display, Some("DP-2"));
        let outputs = vec![output("DP-2", Some(&"Дисплей ".repeat(20)))];
        let shown = source_name(&sources, &outputs);
        assert!(
            shown.chars().count() <= NAME_CAP + 2,
            "clean() can push a pending separator and the next character in the same step that \
             crosses the cap, so its worst case is two past it rather than one"
        );
        assert!(shown.ends_with('…'));
    }

    #[test]
    fn the_tooltip_carries_the_exact_percentage_and_the_source_name() {
        assert_eq!(tooltip(Some("DP-1"), 72, None), "DP-1: 72%");
    }

    #[test]
    fn the_tooltip_falls_back_to_a_bare_percentage_without_a_name() {
        assert_eq!(tooltip(None, 72, None), "72%");
    }

    #[test]
    fn the_switch_reads_the_schedule_never_the_applied_temperature() {
        assert!(switch_on("automatic"), "AC-5: on at midday under Automatic");
        assert!(switch_on("schedule"));
        assert!(!switch_on("off"));
    }

    #[test]
    fn a_small_step_still_moves_a_display_with_a_tiny_native_range() {
        assert_eq!(
            native_step(5, 3),
            1,
            "AC-4: 5% of 3 rounds to zero unless rounded away from zero"
        );
    }

    #[test]
    fn a_step_on_a_wide_native_range_is_the_exact_percentage() {
        assert_eq!(native_step(5, 400_000), 20_000, "AC-4");
    }

    #[test]
    fn coalescer_sends_the_first_request_immediately() {
        let mut coalescer = Coalescer::new();
        assert_eq!(coalescer.request(1), Some(1));
    }

    #[test]
    fn coalescer_queues_a_request_arriving_while_one_is_in_flight() {
        let mut coalescer = Coalescer::new();
        assert_eq!(coalescer.request(1), Some(1));
        assert_eq!(
            coalescer.request(2),
            None,
            "AC-7: a second value while one call is in flight is coalesced, not sent"
        );
    }

    #[test]
    fn coalescer_collapses_several_queued_requests_to_the_last_one() {
        let mut coalescer = Coalescer::new();
        assert_eq!(coalescer.request(1), Some(1));
        assert_eq!(coalescer.request(2), None);
        assert_eq!(coalescer.request(3), None);
        assert_eq!(
            coalescer.completed(),
            Some(3),
            "AC-7: always the final value, not every value in between"
        );
    }

    #[test]
    fn coalescer_is_idle_again_once_nothing_was_queued() {
        let mut coalescer = Coalescer::new();
        assert_eq!(coalescer.request(1), Some(1));
        assert_eq!(coalescer.completed(), None);
        assert_eq!(
            coalescer.request(2),
            Some(2),
            "no call is in flight any more, so the next request sends straight away"
        );
    }
}
