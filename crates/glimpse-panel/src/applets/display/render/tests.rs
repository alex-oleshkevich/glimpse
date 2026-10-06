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
    assert_eq!(chip(true, false, &[]), Some((CHIP_ICON, None)));
}

#[test]
fn the_chip_renders_with_no_source_but_a_reachable_night_light() {
    assert_eq!(
        chip(false, true, &[]),
        Some((CHIP_ICON, None)),
        "AC-1: a desktop with no backlight at all is ordinary, not an error"
    );
}

#[test]
fn the_chip_is_gone_with_neither_offering() {
    assert_eq!(chip(false, false, &[]), None, "AC-2");
}

#[test]
fn is_powered_hides_a_display_whose_output_is_disabled() {
    let display = source("dp", BrightnessKind::Display, Some("DP-2"));
    let outputs = vec![OutputInfo {
        enabled: false,
        ..output("DP-2", None)
    }];
    assert!(!is_powered(&display, &outputs));
}

#[test]
fn is_powered_keeps_a_display_with_no_matching_output() {
    let display = source("dp", BrightnessKind::Display, Some("DP-2"));
    assert!(
        is_powered(&display, &[]),
        "unknown power state must not hide a real source"
    );
}

#[test]
fn is_powered_never_hides_the_keyboard() {
    let keyboard = source("keyboard", BrightnessKind::Keyboard, None);
    assert!(is_powered(&keyboard, &[]));
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
    assert_eq!(brightness_tooltip(Some("DP-1"), 72, None), "DP-1: 72%");
}

#[test]
fn the_tooltip_falls_back_to_a_bare_percentage_without_a_name() {
    assert_eq!(brightness_tooltip(None, 72, None), "72%");
}

#[test]
fn the_switch_reads_the_schedule_never_the_applied_temperature() {
    assert!(switch_on("automatic"), "AC-5: on at midday under Automatic");
    assert!(switch_on("schedule"));
    assert!(!switch_on("off"));
}

#[test]
fn night_mode_is_the_mode_turning_the_light_on_would_use() {
    assert_eq!(night_mode("schedule", "automatic"), "schedule");
    assert_eq!(night_mode("off", "schedule"), "schedule");
    assert_eq!(night_mode("off", "off"), "automatic");
}

#[test]
fn a_keyboard_backlight_carries_its_own_icon() {
    assert_eq!(
        source_icon(BrightnessKind::Keyboard),
        "keyboard-brightness-symbolic"
    );
    assert_eq!(
        source_icon(BrightnessKind::Display),
        "display-brightness-symbolic"
    );
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

fn focused_output(connector: &str, label: Option<&str>, focused: bool) -> OutputInfo {
    OutputInfo {
        connector: connector.to_owned(),
        label: label.map(str::to_owned),
        built_in: false,
        focused,
        make: None,
        model: None,
        serial: None,
        current_mode: None,
        logical: None,
        enabled: true,
    }
}

#[test]
fn the_chip_is_gone_with_no_outputs() {
    assert_eq!(chip(false, false, &[]), None, "AC-13");
}

#[test]
fn every_output_count_gets_the_same_single_display_icon() {
    assert_eq!(
        chip(false, false, &[focused_output("DP-1", None, false)]),
        Some((ICON, Some("DP-1".to_owned()))),
        "AC-12"
    );
    assert_eq!(
        chip(
            false,
            false,
            &[
                focused_output("DP-1", None, false),
                focused_output("DP-2", None, false),
                focused_output("DP-3", None, false),
            ]
        ),
        Some((ICON, Some("DP-1".to_owned()))),
        "the chip does not change glyph with the output count"
    );
}

#[test]
fn the_heading_joins_the_connector_and_the_label() {
    assert_eq!(
        heading(&focused_output("DP-2", Some("Dell U2720Q"), false)),
        "DP-2 · Dell U2720Q"
    );
}

#[test]
fn a_hostile_compositor_supplied_label_is_capped_before_it_reaches_a_tooltip() {
    let long = "Дисплей ".repeat(20);
    let shown = heading(&focused_output("DP-2", Some(&long), false));
    assert!(
        shown.chars().count() <= NAME_CAP + 2,
        "clean() can push a pending separator and the next character in the same step that \
             crosses the cap, so its worst case is two past it rather than one"
    );
    assert!(shown.ends_with('…'));
}

#[test]
fn the_heading_is_the_bare_connector_without_a_label() {
    assert_eq!(heading(&focused_output("eDP-1", None, false)), "eDP-1");
}

#[test]
fn the_tooltip_names_the_focused_output_among_several() {
    let outputs = vec![
        focused_output("eDP-1", Some("Built-in"), false),
        focused_output("DP-2", Some("Dell U2720Q"), true),
    ];
    assert_eq!(
        tooltip(&outputs).as_deref(),
        Some("DP-2 · Dell U2720Q"),
        "AC-12"
    );
}

#[test]
fn the_tooltip_falls_back_to_the_first_output_without_a_focused_one() {
    let outputs = vec![focused_output("eDP-1", None, false)];
    assert_eq!(tooltip(&outputs).as_deref(), Some("eDP-1"));
}

#[test]
fn the_tooltip_is_none_with_no_outputs() {
    assert_eq!(tooltip(&[]), None);
}

#[test]
fn outputs_supply_a_chip_and_tooltip_only_without_a_brightness_offering() {
    let outputs = vec![
        focused_output("eDP-1", Some("Built-in"), false),
        focused_output("DP-2", Some("Dell U2720Q"), true),
    ];
    assert_eq!(
        chip(false, false, &outputs),
        Some((ICON, Some("DP-2 · Dell U2720Q".to_owned())))
    );
    for (sources, night_light) in [(true, false), (false, true), (true, true)] {
        assert_eq!(
            chip(sources, night_light, &outputs),
            Some((CHIP_ICON, None))
        );
    }
    assert_eq!(chip(false, false, &[]), None);
}
