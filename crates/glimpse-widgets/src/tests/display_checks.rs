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

    let detail_of = |holder: &Expandable| -> (FactList, SwitchRow) {
        let body = holder.details::<gtk4::Box>().expect("a detail body");
        let facts = body
            .first_child()
            .and_downcast::<FactList>()
            .expect("the facts lead the detail");
        let switch = facts
            .next_sibling()
            .and_downcast::<SwitchRow>()
            .expect("the enable switch follows the facts");
        (facts, switch)
    };
    let heads_of = |holders: &[Expandable]| -> Vec<LegacyRow> {
        holders.iter().filter_map(Expandable::head).collect()
    };

    displays.set_displays(&[built_in.clone(), external.clone()]);
    let heads: Vec<LegacyRow> = heads_of(&children_of(&displays));
    let holders: Vec<Expandable> = children_of(&displays);
    assert_eq!(heads.len(), 2);
    assert_eq!(holders.len(), 2);
    for head in &heads {
        let _chevron = child_named::<gtk4::Image>(head, "drawer-chevron");
    }
    assert_eq!(heads[0].lead_icon().as_deref(), Some("computer-symbolic"));
    assert_eq!(
        heads[1].lead_icon().as_deref(),
        Some("video-display-symbolic")
    );
    assert!(
        heads.iter().all(|head| head.subtitle().is_none()),
        "a display's head is one line"
    );

    let (built_in_facts, built_in_switch) = detail_of(&holders[0]);
    let (external_facts, external_switch) = detail_of(&holders[1]);
    let built_in_fact_rows: Vec<LegacyRow> = children_of(&built_in_facts);
    let external_fact_rows: Vec<LegacyRow> = children_of(&external_facts);
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

    assert!(!built_in_switch.locked());
    assert!(
        !external_switch.locked(),
        "two enabled outputs: the guard bites on neither"
    );

    let solo = built_in.clone();
    let mut off = external.clone();
    off.enabled = false;
    displays.set_displays(&[solo.clone(), off.clone()]);
    let holders: Vec<Expandable> = children_of(&displays);
    let (_, solo_switch) = detail_of(&holders[0]);
    let (_, off_switch) = detail_of(&holders[1]);
    assert!(
        solo_switch.locked(),
        "the only enabled output's switch cannot be turned off"
    );
    assert!(
        solo_switch.is_sensitive(),
        "the row itself stays sensitive so its explanation is not dimmed along with the knob"
    );
    let solo_head: &LegacyRow = solo_switch.upcast_ref();
    assert_eq!(
        solo_head.subtitle().as_deref(),
        Some("The last enabled display can't be turned off")
    );
    assert!(
        !off_switch.locked(),
        "a disabled output is never locked, or its user would be stranded"
    );
    assert!(!off_switch.active());
    let (off_facts, _) = detail_of(&holders[1]);
    assert_eq!(
        children_of::<LegacyRow>(&off_facts)[0].value().as_deref(),
        Some("Off"),
        "a disabled display's card says so instead of a mode"
    );

    let enable_requests = Rc::new(RefCell::new(Vec::<(String, bool)>::new()));
    displays.connect_enable_requested({
        let enable_requests = Rc::clone(&enable_requests);
        move |_, connector, enabled| enable_requests.borrow_mut().push((connector, enabled))
    });
    off_switch.emit_by_name::<()>("toggled", &[&true]);
    assert_eq!(
        *enable_requests.borrow(),
        vec![("DP-1".to_owned(), true)],
        "toggling a disabled output's switch requests it be enabled, naming its own connector"
    );

    displays.set_displays(&[built_in.clone(), external.clone()]);
    let holders: Vec<Expandable> = children_of(&displays);
    let heads: Vec<LegacyRow> = heads_of(&holders);
    assert!(!holders[0].expanded());
    assert!(!holders[0].has_css_class(crate::drawer::OPEN));

    heads[0].emit_clicked();
    assert!(
        holders[0].expanded() && holders[0].has_css_class(crate::drawer::OPEN),
        "activating a head opens its own detail"
    );

    heads[0].emit_clicked();
    assert!(
        !holders[0].expanded() && !holders[0].has_css_class(crate::drawer::OPEN),
        "activating the open head again closes it"
    );

    heads[0].emit_clicked();
    assert!(holders[0].expanded());
    let kept_head = heads[0].clone();
    let renders_before = displays.imp().renders.get();
    displays.set_displays(&[built_in.clone(), external.clone()]);
    assert_eq!(
        displays.imp().renders.get(),
        renders_before,
        "an identical slice returns early and never re-renders"
    );
    let heads_after: Vec<LegacyRow> = heads_of(&children_of(&displays));
    assert_eq!(heads_after.len(), 2);
    assert_eq!(heads_after[0], kept_head, "rows are reused, not rebuilt");
    assert!(
        holders[0].expanded(),
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
    let holders: Vec<Expandable> = children_of(&displays);
    let (hostile_facts, _) = detail_of(&holders[0]);
    assert_eq!(
        children_of::<LegacyRow>(&hostile_facts)[0]
            .title()
            .as_deref(),
        Some("Connector"),
        "an enabled display with no current mode has no mode line"
    );
    assert!(
        children_of::<LegacyRow>(&hostile_facts).iter().all(|row| {
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
    let gate_holders: Vec<Expandable> = children_of(&gate);
    assert_eq!(gate_holders.len(), 2);

    let facts_of = |holder: &Expandable| -> FactList {
        holder
            .details::<gtk4::Box>()
            .and_then(|body| body.first_child())
            .and_downcast::<FactList>()
            .expect("a fact list")
    };

    let (_, gate_built_in_switch) = detail_of(&gate_holders[0]);
    assert!(
        gate_built_in_switch.locked(),
        "AC-1: output_power true, the default, keeps today's behaviour unchanged"
    );

    let gate_heads: Vec<LegacyRow> = heads_of(&gate_holders);
    gate_heads[0].emit_clicked();
    let gate_panel = gate_holders[0].clone();
    assert!(
        gate_panel.expanded(),
        "the detail opens before the gate changes"
    );

    gate.set_output_power(false);
    let gate_holders_off: Vec<Expandable> = children_of(&gate);
    assert_eq!(
        gate_holders_off, gate_holders,
        "gating the switches off reuses the existing rows rather than rebuilding them"
    );
    assert!(
        gate_panel.expanded(),
        "AC-6: closing the gate on a display that is not changing must not shut an open \
             detail under the user's hand"
    );
    for holder in &gate_holders_off {
        assert!(
            facts_of(holder)
                .next_sibling()
                .is_none_or(|switch| !switch.get_visible()),
            "AC-2/AC-5: output_power false omits the enable switch entirely, even on the \
                 sole enabled output, rather than leaving it present and locked"
        );
    }
    assert_eq!(
        children_of::<LegacyRow>(&facts_of(&gate_holders_off[0])).len(),
        4,
        "AC-3: the built-in display's own facts survive the gate"
    );
    assert_eq!(
        children_of::<LegacyRow>(&facts_of(&gate_holders_off[1])).len(),
        7,
        "AC-3: so do the external display's, mode through position"
    );

    gate.set_output_power(true);
    assert!(
        gate_panel.expanded(),
        "AC-6: reopening the gate must not shut the detail either"
    );
    let (_, gate_built_in_switch_again) = detail_of(&gate_holders[0]);
    let (_, gate_off_switch_again) = detail_of(&gate_holders[1]);
    assert_eq!(
        gate_built_in_switch_again, gate_built_in_switch,
        "AC-4: the switch that comes back is the same row, not a rebuilt one"
    );
    assert!(
        gate_built_in_switch_again.locked(),
        "AC-4: the sole enabled output relocks once the gate reopens"
    );
    assert!(
        !gate_off_switch_again.locked(),
        "AC-4: a disabled output's switch is never locked"
    );

    let brightness = BrightnessPopover::new();
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
    let non_primary_faders: Vec<Fader> = children_of(&*brightness.imp().devices);
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
    let keyboard: Vec<Fader> = children_of(&*brightness.imp().devices);
    assert_eq!(
        keyboard[0].icon_name().as_deref(),
        Some("keyboard-brightness-symbolic"),
        "a keyboard backlight carries its own glyph rather than a display's"
    );
    assert_eq!(
        brightness.imp().primary.icon_name().as_deref(),
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
    let night_head = brightness.imp().night_head.get();
    let night_facts = brightness.imp().night_facts.get();
    let night_values = || -> Vec<(Option<String>, Option<String>)> {
        children_of::<LegacyRow>(&night_facts)
            .iter()
            .map(|row| (row.title(), row.value()))
            .collect()
    };
    assert!(
        night_head.subtitle().is_none(),
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
        brightness.imp().temperature.has_css_class("fader--warm"),
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
        night_head.subtitle().as_deref(),
        Some("Not reaching any display"),
        "a provider that applies nothing says so on its row rather than looking broken"
    );
    assert!(night_head.has_css_class("row--warning"));

    let brightness_changed = Rc::new(RefCell::new(Vec::<(String, f64)>::new()));
    brightness.connect_changed({
        let brightness_changed = Rc::clone(&brightness_changed);
        move |_, key, value| {
            brightness_changed
                .borrow_mut()
                .push((key.to_owned(), value))
        }
    });
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
    night_head.emit_clicked();
    assert!(
        brightness.imp().night_light.expanded(),
        "the row opens the night light card"
    );
    brightness
        .imp()
        .enabled
        .emit_by_name::<()>("toggled", &[&false]);
    assert!(
        brightness.imp().night_light.expanded() && !brightness.imp().temperature.is_sensitive(),
        "switching the light off disables the rail at once and leaves the card open"
    );
    brightness
        .imp()
        .enabled
        .emit_by_name::<()>("toggled", &[&true]);
    assert!(brightness.imp().temperature.is_sensitive());
    night_head.emit_clicked();
    assert!(
        !brightness.imp().night_light.expanded(),
        "and closes it again"
    );
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
    brightness.imp().footer.emit_clicked();
    assert_eq!(brightness_footer.get(), 1);

    let display_popover = DisplayPopover::new();
    assert!(
        !display_popover.imp().section.get_visible(),
        "AC-12: no outputs renders empty, without panicking"
    );

    display_popover.set_output_power(true);
    display_popover.set_displays(&[built_in.clone(), external.clone()]);
    assert!(
        display_popover.imp().section.get_visible(),
        "AC-9: two outputs render the section"
    );
    let popover_holders: Vec<Expandable> = children_of(&*display_popover.imp().devices);
    assert_eq!(
        heads_of(&popover_holders).len(),
        2,
        "AC-9: both outputs list"
    );
    assert!(
        display_popover.imp().blank.get_visible(),
        "AC-9: the blank row is present when output power is supported"
    );
    assert_eq!(
        display_popover.imp().blank.subtitle().as_deref(),
        Some("Any input wakes them again"),
        "AC-11: the blank row's subtitle says input wakes the screens"
    );
    assert_eq!(
        display_popover.imp().blank.type_(),
        LegacyRow::static_type(),
        "AC-11: the blank row is a LegacyRow, never a SwitchRow — DPMS has no state to sit in, so \
             a knob that moved without the screen following it would be lying"
    );

    let devices_renders_before = display_popover.imp().devices.imp().renders.get();
    display_popover.set_output_power(false);
    assert!(
        !display_popover.imp().blank.get_visible(),
        "AC-10: the blank row is absent entirely, not merely insensitive, when output power \
             is unsupported"
    );
    assert_eq!(
        children_of::<Expandable>(&*display_popover.imp().devices).len(),
        2,
        "gating output power keeps both outputs listed"
    );
    assert!(
        display_popover.imp().devices.imp().renders.get() > devices_renders_before,
        "AC-10: DisplayPopover forwards output_power to DisplayList, which gates every \
             per-output switch on its own render (proven directly on DisplayList, above)"
    );

    display_popover.set_displays(&[]);
    assert!(
        !display_popover.imp().section.get_visible(),
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
        .devices
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
    display_popover.imp().blank.emit_clicked();
    assert_eq!(blanked.get(), 1);

    display_popover.set_footer(Some("Display settings"));
    assert!(display_popover.imp().footer.get_visible());

    let popover_heads: Vec<LegacyRow> = heads_of(&children_of(&*display_popover.imp().devices));
    let dimmed = |widget: &gtk4::Widget| {
        std::iter::successors(Some(widget.clone()), |widget| widget.parent())
            .any(|widget| widget.has_css_class(crate::drawer::RECEDED))
    };
    let chrome: [gtk4::Widget; 3] = [
        display_popover.imp().hero.clone().upcast(),
        display_popover.imp().blank.clone().upcast(),
        display_popover.imp().footer.clone().upcast(),
    ];
    popover_heads[0].emit_clicked();
    assert!(
        chrome.iter().all(dimmed) && !dimmed(popover_heads[0].upcast_ref()),
        "opening display details dims the popover chrome while preserving the open row"
    );
    popover_heads[0].emit_clicked();
    assert!(
        !chrome.iter().any(dimmed),
        "closing details restores the popover chrome"
    );

    let popover_footer = Rc::new(Cell::new(0u32));
    display_popover.connect_footer_activated({
        let popover_footer = Rc::clone(&popover_footer);
        move |_| popover_footer.set(popover_footer.get() + 1)
    });
    display_popover.imp().footer.emit_clicked();
    assert_eq!(popover_footer.get(), 1);
}
