use super::*;

pub(super) fn check() {
    let displays = DisplayList::new();
    assert!(
        displays.first_child().is_none(),
        "a display list renders nothing before the first snapshot"
    );

    let mode = DisplayMode {
        width: 1920,
        height: 1080,
        refresh_mhz: 60_000,
    };
    let logical = DisplayLogical {
        x: 0,
        y: 0,
        scale: 1.5,
    };
    let built_in = Display {
        connector: "eDP-1".to_owned(),
        label: "Built-in display".to_owned(),
        current_mode: Some(mode.clone()),
        logical: Some(logical.clone()),
        enabled: true,
        built_in: true,
        ..Display::default()
    };
    let external = Display {
        connector: "DP-1".to_owned(),
        label: "DELL U2720Q".to_owned(),
        make: Some("Dell Inc.".to_owned()),
        model: Some("U2720Q".to_owned()),
        serial: Some("8QK1P93".to_owned()),
        current_mode: Some(mode.clone()),
        logical: Some(logical.clone()),
        enabled: true,
        built_in: false,
    };

    let detail_of = |row: &ExpanderRow| -> (FactList, ToggleRow) {
        let body = row.details();
        let facts = body
            .first_child()
            .and_downcast::<FactList>()
            .expect("the facts lead the detail");
        let toggle = facts
            .next_sibling()
            .and_downcast::<ToggleRow>()
            .expect("the enable switch follows the facts");
        (facts, toggle)
    };
    let head_of = |row: &ExpanderRow| child_named::<ActionRow>(row, "expander-row__head");
    let drawer_of = |row: &ExpanderRow| child_named::<Expandable>(row, "expander-row__expandable");

    displays.set_displays(&[built_in.clone(), external.clone()]);
    let rows: Vec<ExpanderRow> = children_of(&displays);
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert!(
            child_named::<gtk4::Image>(&head_of(row), "row__chevron").get_visible(),
            "the expander supplies the chevron"
        );
    }
    assert_eq!(rows[0].icon_name().as_deref(), Some("computer-symbolic"));
    assert_eq!(
        rows[1].icon_name().as_deref(),
        Some("video-display-symbolic")
    );
    assert!(
        rows.iter().all(|row| row.subtitle().is_none()),
        "a display's head is one line"
    );

    let (built_in_facts, built_in_switch) = detail_of(&rows[0]);
    let (external_facts, external_switch) = detail_of(&rows[1]);
    let built_in_fact_rows: Vec<Row> = children_of(&built_in_facts);
    let external_fact_rows: Vec<Row> = children_of(&external_facts);
    assert_eq!(
        built_in_fact_rows.len(),
        4,
        "the mode and the connector always lead the detail; a display with no make, model or \
             serial omits those lines rather than rendering Unknown or an empty row"
    );
    assert_eq!(external_fact_rows.len(), 7);
    assert_eq!(
        built_in_fact_rows[0].value().as_deref(),
        Some("1920 × 1080 · 60 Hz"),
        "an enabled display's card leads with its current mode"
    );
    assert!(
        !built_in_fact_rows
            .iter()
            .any(|row| row.value().as_deref() == Some("8QK1P93"))
    );
    assert!(
        external_fact_rows
            .iter()
            .any(|row| row.value().as_deref() == Some("8QK1P93"))
    );
    assert!(
        built_in_fact_rows
            .iter()
            .any(|row| row.title().as_deref() == Some("Connector")),
        "the head names the display, so the connector is a line of the detail"
    );

    assert!(built_in_switch.knob().is_sensitive());
    assert!(
        external_switch.knob().is_sensitive(),
        "two enabled outputs: the guard bites on neither"
    );

    let solo = built_in.clone();
    let mut off = external.clone();
    off.enabled = false;
    displays.set_displays(&[solo.clone(), off.clone()]);
    let rows: Vec<ExpanderRow> = children_of(&displays);
    let (_, solo_switch) = detail_of(&rows[0]);
    let (_, off_switch) = detail_of(&rows[1]);
    assert!(solo_switch.locked());
    assert!(!off_switch.locked());
    assert!(
        !solo_switch.knob().is_sensitive(),
        "the only enabled output's switch cannot be turned off"
    );
    assert!(
        solo_switch.is_sensitive(),
        "the row itself stays sensitive so its explanation is not dimmed along with the knob"
    );
    assert_eq!(
        solo_switch.upcast_ref::<Row>().subtitle().as_deref(),
        Some("The last enabled display can't be turned off")
    );
    assert!(
        off_switch.knob().is_sensitive(),
        "a disabled output is never locked, or its user would be stranded"
    );
    assert!(!off_switch.active());
    let (off_facts, _) = detail_of(&rows[1]);
    assert_eq!(
        children_of::<Row>(&off_facts)[0].value().as_deref(),
        Some("Off"),
        "a disabled display's card says so instead of a mode"
    );

    let enable_requests = Rc::new(RefCell::new(Vec::<(String, bool)>::new()));
    displays.connect_enable_requested({
        let enable_requests = Rc::clone(&enable_requests);
        move |_, connector, enabled| enable_requests.borrow_mut().push((connector, enabled))
    });
    off_switch.activate();
    assert_eq!(
        *enable_requests.borrow(),
        vec![("DP-1".to_owned(), true)],
        "toggling a disabled output's switch requests it be enabled, naming its own connector"
    );
    let flips = Rc::new(Cell::new(0u32));
    solo_switch.connect_notify_local(Some("active"), {
        let flips = Rc::clone(&flips);
        move |_, _| flips.set(flips.get() + 1)
    });
    solo_switch.activate();
    assert_eq!(
        enable_requests.borrow().len(),
        1,
        "a locked toggle does not request to be turned off"
    );
    assert!(
        solo_switch.active() && flips.get() == 0,
        "a locked toggle refuses the click instead of flipping and snapping back"
    );
    let requests_before = enable_requests.borrow().len();
    off_switch.set_active(false);
    solo_switch.set_active(true);
    assert_eq!(
        enable_requests.borrow().len(),
        requests_before,
        "programmatic set_active never requests a change"
    );

    displays.set_displays(&[built_in.clone(), external.clone()]);
    let rows: Vec<ExpanderRow> = children_of(&displays);
    assert!(!rows[0].open());
    assert!(!drawer_of(&rows[0]).has_css_class(crate::drawer::OPEN));

    head_of(&rows[0]).emit_by_name::<()>("activated", &[]);
    assert!(
        rows[0].open() && drawer_of(&rows[0]).has_css_class(crate::drawer::OPEN),
        "activating a head opens its own detail"
    );

    head_of(&rows[0]).emit_by_name::<()>("activated", &[]);
    assert!(
        !rows[0].open() && !drawer_of(&rows[0]).has_css_class(crate::drawer::OPEN),
        "activating the open head again closes it"
    );

    head_of(&rows[0]).emit_by_name::<()>("activated", &[]);
    assert!(rows[0].open());
    let kept_row = rows[0].clone();
    let renders_before = displays.imp().renders.get();
    displays.set_displays(&[built_in.clone(), external.clone()]);
    assert_eq!(
        displays.imp().renders.get(),
        renders_before,
        "an identical slice returns early and never re-renders"
    );
    let rows_after: Vec<ExpanderRow> = children_of(&displays);
    assert_eq!(rows_after.len(), 2);
    assert_eq!(rows_after[0], kept_row, "rows are reused, not rebuilt");
    assert!(
        kept_row.open(),
        "an unchanged slice must not close a detail the user has open"
    );

    let hostile = Display {
        connector: "DP-2".to_owned(),
        make: Some("ё".repeat(300)),
        model: Some("ё".repeat(300)),
        serial: Some("ё".repeat(300)),
        enabled: true,
        ..Display::default()
    };
    displays.set_displays(&[hostile]);
    let rows: Vec<ExpanderRow> = children_of(&displays);
    let (hostile_facts, _) = detail_of(&rows[0]);
    assert_eq!(
        children_of::<Row>(&hostile_facts)[0].title().as_deref(),
        Some("Connector"),
        "an enabled display with no current mode has no mode line"
    );
    assert!(
        children_of::<Row>(&hostile_facts).iter().all(|row| {
            row.value()
                .map(|value| value.chars().count() <= TEXT_MAX_CHARS)
                .unwrap_or(true)
        }),
        "an unbounded, multi-byte EDID string is capped without slicing a character"
    );

    displays.set_displays(&[]);
    assert!(
        displays.first_child().is_none(),
        "an emptied display list renders no children"
    );

    let gate = DisplayList::new();
    let mut gate_off = external.clone();
    gate_off.enabled = false;
    gate.set_displays(&[built_in.clone(), gate_off.clone()]);
    let gate_rows: Vec<ExpanderRow> = children_of(&gate);
    assert_eq!(gate_rows.len(), 2);

    let facts_of = |row: &ExpanderRow| -> FactList {
        row.details()
            .first_child()
            .and_downcast::<FactList>()
            .expect("a fact list")
    };

    let (_, gate_built_in_switch) = detail_of(&gate_rows[0]);
    assert!(
        !gate_built_in_switch.knob().is_sensitive(),
        "AC-1: output_power true, the default, keeps today's behaviour unchanged"
    );

    head_of(&gate_rows[0]).emit_by_name::<()>("activated", &[]);
    let gate_panel = gate_rows[0].clone();
    assert!(
        gate_panel.open(),
        "the detail opens before the gate changes"
    );

    gate.set_output_power(false);
    let gate_rows_off: Vec<ExpanderRow> = children_of(&gate);
    assert_eq!(
        gate_rows_off, gate_rows,
        "gating the switches off reuses the existing rows rather than rebuilding them"
    );
    assert!(
        gate_panel.open(),
        "AC-6: closing the gate on a display that is not changing must not shut an open \
             detail under the user's hand"
    );
    for row in &gate_rows_off {
        assert!(
            facts_of(row)
                .next_sibling()
                .is_none_or(|toggle| !toggle.get_visible()),
            "AC-2/AC-5: output_power false omits the enable switch entirely, even on the \
                 sole enabled output, rather than leaving it present and locked"
        );
    }
    assert_eq!(
        children_of::<Row>(&facts_of(&gate_rows_off[0])).len(),
        4,
        "AC-3: the built-in display's own facts survive the gate"
    );
    assert_eq!(
        children_of::<Row>(&facts_of(&gate_rows_off[1])).len(),
        7,
        "AC-3: so do the external display's, mode through position"
    );

    gate.set_output_power(true);
    assert!(
        gate_panel.open(),
        "AC-6: reopening the gate must not shut the detail either"
    );
    let (_, gate_built_in_switch_again) = detail_of(&gate_rows[0]);
    let (_, gate_off_switch_again) = detail_of(&gate_rows[1]);
    assert_eq!(
        gate_built_in_switch_again, gate_built_in_switch,
        "AC-4: the switch that comes back is the same row, not a rebuilt one"
    );
    assert!(
        !gate_built_in_switch_again.knob().is_sensitive(),
        "AC-4: the sole enabled output relocks once the gate reopens"
    );
    assert!(
        gate_off_switch_again.knob().is_sensitive(),
        "AC-4: a disabled output's switch is never locked"
    );

    let brightness = DisplayPopover::new();
    assert!(
        !brightness.imp().primary.get_visible() && !brightness.imp().devices.get_visible(),
        "AC-1: with no sources, neither the primary fader nor the source list renders"
    );

    let level = |key: &str, name: &str, value: f64| Source {
        key: key.to_owned(),
        name: name.to_owned(),
        icon: String::new(),
        value,
        maximum: 100.0,
        floor: 0.0,
    };

    brightness.set_sources(&[level("built-in", "Built-in", 42.0)]);
    assert!(
        brightness.imp().primary.get_visible(),
        "AC-1: one source renders the primary fader"
    );
    assert!(
        !brightness.imp().primary_name.get_visible(),
        "a lone source needs no name above its rail"
    );
    assert!(
        !brightness.imp().devices.get_visible(),
        "AC-1: one source keeps the source list hidden"
    );
    assert_eq!(brightness.imp().primary.value(), 42.0);
    assert_eq!(
        brightness.imp().readout.value().as_deref(),
        Some("42"),
        "AC-2: the hero readout carries the percentage"
    );
    assert_eq!(brightness.imp().readout.unit().as_deref(), Some("%"));
    assert!(
        brightness
            .imp()
            .primary
            .tooltip_text()
            .is_some_and(|text| text.contains("42")),
        "AC-2: the fader carries its exact value in a tooltip instead of a readout"
    );

    brightness.set_sources(&[Source {
        key: "built-in".to_owned(),
        name: "Built-in".to_owned(),
        icon: String::new(),
        value: 200_000.0,
        maximum: 400_000.0,
        floor: 0.0,
    }]);
    assert_eq!(
        brightness.imp().primary.value(),
        200_000.0,
        "the fader keeps the hardware's own native range"
    );
    assert_eq!(
        brightness.imp().readout.value().as_deref(),
        Some("50"),
        "the readout is a percentage of the maximum, not the raw native value"
    );
    assert!(
        brightness
            .imp()
            .primary
            .tooltip_text()
            .is_some_and(|text| text.contains("50") && !text.contains("200000")),
        "the tooltip is a percentage too; a native value would read 200000%"
    );

    brightness.set_sources(&[
        level("built-in", "Built-in", 40.0),
        level("dp-1", "DP-1", 60.0),
        level("dp-2", "DP-2", 80.0),
    ]);
    assert!(
        brightness.imp().primary.get_visible() && brightness.imp().devices.get_visible(),
        "AC-1: three sources render both the primary fader and the source list"
    );
    assert!(
        brightness.imp().primary_name.get_visible(),
        "beside other sources the primary rail is named, or nothing says which screen it drives"
    );
    assert_eq!(
        brightness.imp().primary_name.title().as_deref(),
        Some("Built-in")
    );
    assert_eq!(
        brightness.imp().primary.value(),
        40.0,
        "the primary rail mirrors the caller's own first, current source"
    );
    let non_primary_faders: Vec<SliderRow> = children_of(&*brightness.imp().devices);
    assert_eq!(
        non_primary_faders.len(),
        2,
        "the source list renders only sources[1..], or the current source would get two \
             faders that do not track each other"
    );
    assert_eq!(non_primary_faders[0].value(), 60.0);
    assert_eq!(non_primary_faders[1].value(), 80.0);
    assert!(
        non_primary_faders
            .iter()
            .all(|fader| fader.tooltip_text().is_some_and(|text| !text.is_empty())),
        "AC-2: every source-list fader carries its exact value in a tooltip too, not only \
             the primary rail"
    );

    brightness.set_sources(&[
        level("built-in", "Built-in", 40.0),
        Source {
            icon: "keyboard-brightness-symbolic".to_owned(),
            ..level("kbd", "Keyboard", 30.0)
        },
    ]);
    let keyboard: Vec<SliderRow> = children_of(&*brightness.imp().devices);
    assert_eq!(
        keyboard[0].lead_icon().as_deref(),
        Some("keyboard-brightness-symbolic"),
        "a keyboard backlight carries its own glyph rather than a display's"
    );
    assert_eq!(
        brightness.imp().primary.lead_icon().as_deref(),
        Some("display-brightness-symbolic"),
        "an empty icon falls back to the display glyph"
    );

    brightness.set_sources(&[]);
    assert!(
        !brightness.imp().primary.get_visible() && !brightness.imp().devices.get_visible(),
        "AC-1: going back to no sources hides both again, without panicking"
    );
    brightness.set_sources(&[level("built-in", "Built-in", 55.0)]);

    assert!(
        !brightness.imp().night_light.get_visible(),
        "AC-4: with no night light snapshot ever seen, the whole section is absent"
    );

    brightness.set_night_light(Some(&NightLight {
        enabled: false,
        temperature: 6500,
        ..Default::default()
    }));
    assert!(brightness.imp().night_light.get_visible());
    assert!(brightness.imp().night_light.is_sensitive());
    let night_facts = brightness.imp().night_facts.get();
    let night_values = || -> Vec<(Option<String>, Option<String>)> {
        children_of::<Row>(&night_facts)
            .iter()
            .map(|row| (row.title(), row.value()))
            .collect()
    };
    assert!(
        brightness.imp().night_light.subtitle().is_none(),
        "the night light row is one line"
    );
    assert!(
        !brightness.imp().enabled.active(),
        "the card's switch says the light is off"
    );
    assert!(
        brightness.imp().temperature.get_visible() && !brightness.imp().temperature.is_sensitive(),
        "a light that is off keeps its rail in the card, disabled rather than hidden"
    );

    brightness.set_night_light(Some(&NightLight {
        enabled: true,
        temperature: 4200,
        schedule: "automatic".to_owned(),
        serving: true,
        ..Default::default()
    }));
    assert!(brightness.imp().enabled.active());
    assert!(brightness.imp().temperature.is_sensitive());
    assert!(
        !brightness.imp().schedules.get_visible(),
        "one schedule is nothing to choose"
    );
    assert_eq!(
        night_values(),
        [
            (
                Some("Schedule".to_owned()),
                Some("Sunset to sunrise".to_owned())
            ),
            (Some("Temperature".to_owned()), Some("4200 K".to_owned())),
        ],
        "the card says which schedule is running and how warm it is now"
    );
    brightness
        .imp()
        .temperature
        .emit_by_name::<()>("moved", &[&3400.0f64]);
    assert_eq!(
        night_values()[1].1.as_deref(),
        Some("3400 K"),
        "the line follows the rail while it is dragged, before any reply"
    );
    assert_eq!(brightness.imp().temperature.value(), 4200.0);
    assert!(
        brightness
            .imp()
            .temperature
            .tooltip_text()
            .is_some_and(|text| text.contains("3400")),
        "the temperature rail carries its exact value in a tooltip too, following the drag"
    );
    assert!(
        brightness
            .imp()
            .temperature
            .has_css_class("slider-row--warm"),
        "AC-6: the warm tint is a class scoped to this one fader"
    );
    assert!(
        !brightness.imp().temperature.has_css_class("accent"),
        "AC-6: the warm tint is not the accent colour the audio popover's own fader uses"
    );

    brightness.set_night_light(None);
    assert!(
        brightness.imp().night_light.get_visible() && !brightness.imp().night_light.is_sensitive(),
        "AC-5: losing the provider keeps the last-good section visible but greys it"
    );
    assert_eq!(
        brightness.imp().temperature.value(),
        4200.0,
        "AC-5: the last-good values stay on screen rather than resetting"
    );

    brightness.set_night_light(Some(&NightLight {
        enabled: true,
        temperature: 3000,
        ..Default::default()
    }));
    assert!(
        brightness.imp().night_light.is_sensitive(),
        "a fresh snapshot lifts the greyed state"
    );
    assert_eq!(
        brightness.imp().night_light.subtitle().as_deref(),
        Some("Not reaching any display"),
        "a provider that applies nothing says so on its row rather than looking broken"
    );
    assert_eq!(brightness.imp().night_light.tone(), RowTone::Warning);
    assert!(brightness.imp().night_light.has_css_class("row--warning"));

    let brightness_changed = Rc::new(RefCell::new(Vec::<(String, f64)>::new()));
    brightness.connect_changed({
        let brightness_changed = Rc::clone(&brightness_changed);
        move |_, key, value| {
            brightness_changed
                .borrow_mut()
                .push((key.to_owned(), value))
        }
    });
    let before_drag = brightness.imp().primary.value();
    brightness
        .imp()
        .primary
        .emit_by_name::<()>("moved", &[&27.0f64]);
    assert_eq!(brightness.imp().readout.value().as_deref(), Some("27"));
    assert_eq!(brightness.imp().primary.value(), before_drag);
    assert!(brightness_changed.borrow().is_empty());

    brightness.set_sources(&[Source {
        maximum: 400_000.0,
        value: 200_000.0,
        ..level("built-in", "Built-in", 50.0)
    }]);
    brightness
        .imp()
        .primary
        .emit_by_name::<()>("moved", &[&300_000.0f64]);
    assert_eq!(brightness.imp().readout.value().as_deref(), Some("75"));
    assert!(brightness_changed.borrow().is_empty());
    brightness.set_sources(&[level("built-in", "Built-in", 55.0)]);
    brightness
        .imp()
        .primary
        .emit_by_name::<()>("changed", &[&33.0f64]);
    assert_eq!(
        *brightness_changed.borrow(),
        [("built-in".to_owned(), 33.0)],
        "the primary fader reports the key of the source it mirrors"
    );
    brightness_changed.borrow_mut().clear();
    brightness
        .imp()
        .devices
        .emit_by_name::<()>("changed", &[&"dp-1".to_owned(), &70.0f64]);
    assert_eq!(
        *brightness_changed.borrow(),
        [("dp-1".to_owned(), 70.0)],
        "a source list change is forwarded with its own key"
    );

    let night_toggled = Rc::new(RefCell::new(Vec::<bool>::new()));
    brightness.connect_night_light_toggled({
        let night_toggled = Rc::clone(&night_toggled);
        move |_, on| night_toggled.borrow_mut().push(on)
    });
    brightness
        .imp()
        .enabled
        .emit_by_name::<()>("toggled", &[&false]);
    assert_eq!(*night_toggled.borrow(), [false]);

    brightness
        .imp()
        .enabled
        .emit_by_name::<()>("toggled", &[&true]);
    assert_eq!(*night_toggled.borrow(), [false, true]);

    brightness.set_night_light(Some(&NightLight {
        enabled: true,
        temperature: 3000,
        schedule: "schedule".to_owned(),
        fixed_hours: true,
        serving: true,
    }));
    assert!(brightness.imp().schedules.get_visible());
    let night_head = child_named::<ActionRow>(&*brightness.imp().night_light, "expander-row__head");
    night_head.emit_by_name::<()>("activated", &[]);
    assert!(
        brightness.imp().night_light.open()
            && child_named::<Expandable>(
                &*brightness.imp().night_light,
                "expander-row__expandable"
            )
            .has_css_class(crate::drawer::OPEN),
        "the row opens the night light card"
    );
    brightness
        .imp()
        .enabled
        .emit_by_name::<()>("toggled", &[&false]);
    assert!(
        brightness.imp().night_light.open() && !brightness.imp().temperature.is_sensitive(),
        "switching the light off disables the rail at once and leaves the card open"
    );
    brightness
        .imp()
        .enabled
        .emit_by_name::<()>("toggled", &[&true]);
    assert!(brightness.imp().temperature.is_sensitive());
    night_head.emit_by_name::<()>("activated", &[]);
    assert!(!brightness.imp().night_light.open(), "and closes it again");
    assert_eq!(
        brightness.imp().schedules.selected(),
        Some(1),
        "the configured hours are the schedule in force"
    );
    assert_eq!(
        night_values(),
        [(Some("Temperature".to_owned()), Some("3000 K".to_owned()))],
        "a schedule that can be chosen is not repeated as a fact"
    );
    let scheduled = Rc::new(RefCell::new(Vec::<String>::new()));
    brightness.connect_night_light_scheduled({
        let scheduled = Rc::clone(&scheduled);
        move |_, schedule| scheduled.borrow_mut().push(schedule.to_owned())
    });
    brightness
        .imp()
        .schedules
        .emit_by_name::<()>("activated", &[&0u32]);
    assert_eq!(
        *scheduled.borrow(),
        ["automatic"],
        "a choice reports the provider's own spelling of its mode"
    );

    let night_changed = Rc::new(RefCell::new(Vec::<f64>::new()));
    brightness.connect_night_light_changed({
        let night_changed = Rc::clone(&night_changed);
        move |_, value| night_changed.borrow_mut().push(value)
    });
    brightness
        .imp()
        .temperature
        .emit_by_name::<()>("changed", &[&3400.0f64]);
    assert_eq!(*night_changed.borrow(), [3400.0]);

    brightness.set_footer(Some("Display settings"));
    assert!(brightness.imp().footer.get_visible());
    let brightness_footer = Rc::new(Cell::new(0u32));
    brightness.connect_footer_activated({
        let brightness_footer = Rc::clone(&brightness_footer);
        move |_| brightness_footer.set(brightness_footer.get() + 1)
    });
    brightness.imp().footer.emit_by_name::<()>("activated", &[]);
    assert_eq!(brightness_footer.get(), 1);

    let display_popover = DisplayPopover::new();
    assert!(
        !display_popover.imp().displays_section.get_visible(),
        "AC-12: no outputs renders empty, without panicking"
    );

    display_popover.set_output_power(true);
    display_popover.set_displays(&[built_in.clone(), external.clone()]);
    assert!(
        display_popover.imp().displays_section.get_visible(),
        "AC-9: two outputs render the section"
    );
    let popover_rows: Vec<ExpanderRow> = children_of(&*display_popover.imp().displays);
    assert_eq!(popover_rows.len(), 2, "AC-9: both outputs list");
    assert!(
        display_popover.imp().blank.get_visible(),
        "AC-9: the blank row is present when output power is supported"
    );
    assert_eq!(
        display_popover
            .imp()
            .blank
            .upcast_ref::<Row>()
            .subtitle()
            .as_deref(),
        None,
        "the blank row has no subtitle"
    );
    assert_eq!(
        display_popover.imp().blank.type_(),
        ActionRow::static_type(),
        "the blank row is an ActionRow, never a ToggleRow — DPMS has no state to sit in, so \
             a knob that moved without the screen following it would be lying"
    );

    let devices_renders_before = display_popover.imp().displays.imp().renders.get();
    display_popover.set_output_power(false);
    assert!(
        !display_popover.imp().blank.get_visible(),
        "AC-10: the blank row is absent entirely, not merely insensitive, when output power \
             is unsupported"
    );
    assert_eq!(
        children_of::<ExpanderRow>(&*display_popover.imp().displays).len(),
        2,
        "gating output power keeps both outputs listed"
    );
    assert!(
        display_popover.imp().displays.imp().renders.get() > devices_renders_before,
        "AC-10: DisplayPopover forwards output_power to DisplayList, which gates every \
             per-output switch on its own render (proven directly on DisplayList, above)"
    );

    display_popover.set_displays(&[]);
    assert!(
        !display_popover.imp().displays_section.get_visible(),
        "AC-12: emptying the outputs renders empty again"
    );

    display_popover.set_output_power(true);
    display_popover.set_displays(std::slice::from_ref(&built_in));
    assert!(display_popover.imp().blank.get_visible());

    let popover_enable_requests = Rc::new(RefCell::new(Vec::<(String, bool)>::new()));
    display_popover.connect_enable_requested({
        let popover_enable_requests = Rc::clone(&popover_enable_requests);
        move |_, connector, enabled| {
            popover_enable_requests
                .borrow_mut()
                .push((connector.to_owned(), enabled))
        }
    });
    display_popover
        .imp()
        .displays
        .emit_by_name::<()>("enable-requested", &[&"eDP-1".to_owned(), &false]);
    assert_eq!(
        *popover_enable_requests.borrow(),
        [("eDP-1".to_owned(), false)],
        "a DisplayList request is forwarded as the popover's own signal"
    );

    let blanked = Rc::new(Cell::new(0u32));
    display_popover.connect_blanked({
        let blanked = Rc::clone(&blanked);
        move |_| blanked.set(blanked.get() + 1)
    });
    display_popover
        .imp()
        .blank
        .emit_by_name::<()>("activated", &[]);
    assert_eq!(blanked.get(), 1);

    display_popover.set_footer(Some("Display settings"));
    assert!(display_popover.imp().footer.get_visible());

    let open_head = head_of(&children_of::<ExpanderRow>(&*display_popover.imp().displays)[0]);
    open_head.emit_by_name::<()>("activated", &[]);
    assert!(
        display_popover
            .imp()
            .shell
            .imp()
            .header
            .has_css_class(crate::drawer::RECEDED)
            && display_popover
                .imp()
                .shell
                .imp()
                .footer_box
                .has_css_class(crate::drawer::RECEDED)
            && display_popover
                .imp()
                .blank
                .has_css_class(crate::drawer::RECEDED)
            && !open_head.has_css_class(crate::drawer::RECEDED),
        "opening display details dims the header, the blank row and the footer, preserving \
             the open row"
    );
    open_head.emit_by_name::<()>("activated", &[]);
    assert!(
        !display_popover
            .imp()
            .shell
            .imp()
            .header
            .has_css_class(crate::drawer::RECEDED)
            && !display_popover
                .imp()
                .shell
                .imp()
                .footer_box
                .has_css_class(crate::drawer::RECEDED)
            && !display_popover
                .imp()
                .blank
                .has_css_class(crate::drawer::RECEDED),
        "closing details restores the popover chrome"
    );

    let popover_footer = Rc::new(Cell::new(0u32));
    display_popover.connect_footer_activated({
        let popover_footer = Rc::clone(&popover_footer);
        move |_| popover_footer.set(popover_footer.get() + 1)
    });
    display_popover
        .imp()
        .footer
        .emit_by_name::<()>("activated", &[]);
    assert_eq!(popover_footer.get(), 1);

    laptop_fixture();
    desktop_fixture();
    signals_fire_once_per_action();
}

fn laptop_fixture() {
    let popover = DisplayPopover::new();
    popover.set_sources(&[
        Source {
            key: "built-in".to_owned(),
            name: "Built-in".to_owned(),
            icon: String::new(),
            value: 50.0,
            maximum: 100.0,
            floor: 5.0,
        },
        Source {
            key: "kbd".to_owned(),
            name: "Keyboard".to_owned(),
            icon: "keyboard-brightness-symbolic".to_owned(),
            value: 30.0,
            maximum: 100.0,
            floor: 0.0,
        },
    ]);
    popover.set_night_light(Some(&NightLight {
        enabled: true,
        temperature: 4000,
        schedule: "automatic".to_owned(),
        fixed_hours: true,
        serving: true,
    }));
    popover.set_output_power(true);
    popover.set_displays(std::slice::from_ref(&Display {
        connector: "eDP-1".to_owned(),
        label: "Built-in display".to_owned(),
        enabled: true,
        built_in: true,
        ..Display::default()
    }));
    popover.set_footer(Some("Display settings"));

    let imp = popover.imp();
    assert!(imp.readout.get_visible());
    assert!(imp.brightness.get_visible());
    assert!(imp.primary.get_visible());
    assert!(imp.primary_name.get_visible());
    assert!(imp.devices.get_visible());
    assert!(imp.night_light.get_visible());
    assert!(imp.displays_section.get_visible());
    assert!(imp.blank.get_visible());
    assert!(imp.footer.get_visible());
    assert_eq!(
        imp.shell.icon_name().as_deref(),
        Some("display-brightness-symbolic")
    );

    let brightness_box = child_named::<gtk4::Box>(&*imp.brightness, "section__content")
        .first_child()
        .expect("the brightness column");
    assert_eq!(imp.primary.parent().as_ref(), Some(&brightness_box));
    assert_eq!(imp.primary_name.parent().as_ref(), Some(&brightness_box));
    assert_eq!(imp.devices.parent().as_ref(), Some(&brightness_box));
    assert_eq!(imp.night_light.parent().as_ref(), Some(&brightness_box));
    assert_eq!(
        brightness_box.last_child().as_ref(),
        Some(imp.night_light.upcast_ref()),
        "night light is the last row of Brightness"
    );
    let displays_box = child_named::<gtk4::Box>(&*imp.displays_section, "section__content")
        .first_child()
        .expect("the displays column");
    assert_eq!(imp.displays.parent().as_ref(), Some(&displays_box));
    assert_eq!(imp.blank.parent().as_ref(), Some(&displays_box));
    assert_eq!(
        imp.blank.prev_sibling().as_ref(),
        Some(imp.displays.upcast_ref()),
        "blank follows the display list"
    );
    assert_ne!(imp.footer.parent(), imp.brightness.parent());
}

fn desktop_fixture() {
    let popover = DisplayPopover::new();
    popover.set_output_power(true);
    popover.set_displays(std::slice::from_ref(&Display {
        connector: "DP-1".to_owned(),
        label: "Dell".to_owned(),
        enabled: true,
        ..Display::default()
    }));
    let imp = popover.imp();
    assert!(!imp.readout.get_visible());
    assert_eq!(
        imp.shell.icon_name().as_deref(),
        Some("video-display-symbolic")
    );
    assert!(
        !imp.brightness.get_visible(),
        "a desktop with no night light hides Brightness"
    );
    assert!(imp.displays_section.get_visible());

    popover.set_night_light(Some(&NightLight {
        enabled: false,
        temperature: 6500,
        ..Default::default()
    }));
    assert!(imp.brightness.get_visible());
    assert!(imp.night_light.get_visible());
    assert!(!imp.readout.get_visible());
    assert!(!imp.primary.get_visible());
    assert!(!imp.devices.get_visible());
    assert_eq!(
        imp.shell.icon_name().as_deref(),
        Some("video-display-symbolic")
    );
}

fn signals_fire_once_per_action() {
    let popover = DisplayPopover::new();
    let changed = Rc::new(Cell::new(0u32));
    let toggled = Rc::new(Cell::new(0u32));
    let night_changed = Rc::new(Cell::new(0u32));
    let moved = Rc::new(Cell::new(0u32));
    let scheduled = Rc::new(Cell::new(0u32));
    let enabled = Rc::new(Cell::new(0u32));
    let blanked = Rc::new(Cell::new(0u32));
    let footer = Rc::new(Cell::new(0u32));
    popover.connect_changed({
        let changed = Rc::clone(&changed);
        move |_, _, _| changed.set(changed.get() + 1)
    });
    popover.connect_night_light_toggled({
        let toggled = Rc::clone(&toggled);
        move |_, _| toggled.set(toggled.get() + 1)
    });
    popover.connect_night_light_changed({
        let night_changed = Rc::clone(&night_changed);
        move |_, _| night_changed.set(night_changed.get() + 1)
    });
    popover.connect_night_light_moved({
        let moved = Rc::clone(&moved);
        move |_, _| moved.set(moved.get() + 1)
    });
    popover.connect_night_light_scheduled({
        let scheduled = Rc::clone(&scheduled);
        move |_, _| scheduled.set(scheduled.get() + 1)
    });
    popover.connect_enable_requested({
        let enabled = Rc::clone(&enabled);
        move |_, _, _| enabled.set(enabled.get() + 1)
    });
    popover.connect_blanked({
        let blanked = Rc::clone(&blanked);
        move |_| blanked.set(blanked.get() + 1)
    });
    popover.connect_footer_activated({
        let footer = Rc::clone(&footer);
        move |_| footer.set(footer.get() + 1)
    });

    popover.set_sources(&[
        Source {
            key: "built-in".to_owned(),
            name: "Built-in".to_owned(),
            icon: String::new(),
            value: 40.0,
            maximum: 100.0,
            floor: 0.0,
        },
        Source {
            key: "kbd".to_owned(),
            name: "Keyboard".to_owned(),
            icon: String::new(),
            value: 20.0,
            maximum: 100.0,
            floor: 0.0,
        },
    ]);
    popover.imp().primary.set_value(12.0);
    popover.set_night_light(Some(&NightLight {
        enabled: false,
        temperature: 4000,
        schedule: "automatic".to_owned(),
        fixed_hours: true,
        serving: true,
    }));
    popover.imp().enabled.set_active(true);
    popover.imp().temperature.set_value(2500.0);
    popover.set_night_light(Some(&NightLight {
        enabled: true,
        temperature: 2500,
        schedule: "schedule".to_owned(),
        fixed_hours: true,
        serving: true,
    }));
    popover.set_output_power(true);
    popover.set_displays(&[
        Display {
            connector: "eDP-1".to_owned(),
            enabled: true,
            built_in: true,
            ..Display::default()
        },
        Display {
            connector: "DP-1".to_owned(),
            enabled: true,
            ..Display::default()
        },
    ]);
    popover.set_footer(Some("Display settings"));
    let rows: Vec<ExpanderRow> = children_of(&*popover.imp().displays);
    let toggle = rows[0]
        .details()
        .last_child()
        .and_downcast::<ToggleRow>()
        .expect("an enable toggle");
    toggle.set_active(true);
    assert_eq!(
        (
            changed.get(),
            toggled.get(),
            night_changed.get(),
            moved.get(),
            scheduled.get(),
            enabled.get(),
            blanked.get(),
            footer.get(),
        ),
        (0, 0, 0, 0, 0, 0, 0, 0),
        "setters emit nothing"
    );

    popover
        .imp()
        .primary
        .emit_by_name::<()>("changed", &[&33.0f64]);
    assert_eq!(changed.get(), 1);
    children_of::<SliderRow>(&*popover.imp().devices)[0].emit_by_name::<()>("changed", &[&22.0f64]);
    assert_eq!(changed.get(), 2);

    popover
        .imp()
        .enabled
        .emit_by_name::<()>("toggled", &[&false]);
    assert_eq!(toggled.get(), 1);

    popover
        .imp()
        .temperature
        .emit_by_name::<()>("changed", &[&3000.0f64]);
    assert_eq!(night_changed.get(), 1);
    assert_eq!(moved.get(), 0);
    popover
        .imp()
        .temperature
        .emit_by_name::<()>("moved", &[&3100.0f64]);
    assert_eq!(moved.get(), 1);
    assert_eq!(night_changed.get(), 1);

    popover
        .imp()
        .schedules
        .emit_by_name::<()>("activated", &[&0u32]);
    assert_eq!(scheduled.get(), 1);

    toggle.emit_by_name::<()>("toggled", &[&false]);
    assert_eq!(enabled.get(), 1);
    toggle.set_active(true);
    assert_eq!(enabled.get(), 1, "set_active does not request again");

    popover.imp().blank.emit_by_name::<()>("activated", &[]);
    assert_eq!(blanked.get(), 1);
    popover.imp().footer.emit_by_name::<()>("activated", &[]);
    assert_eq!(footer.get(), 1);
}
