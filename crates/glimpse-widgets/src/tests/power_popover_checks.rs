use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::{prelude::*, subclass::prelude::*};

use super::{
    ActionRow, Choice, Fact, PowerChargeLimit, PowerDevice, PowerHistory, PowerHoldNotice,
    PowerPopover, Row, RowTone, Severity, TEXT_MAX_CHARS, Tick, ToggleControl, ToggleRow,
    child_named, children_of,
};

pub(super) fn check() {
    let power = PowerPopover::new();
    let window = gtk4::Window::new();
    window.set_child(Some(&power));

    let profiles = Rc::new(RefCell::new(Vec::<u32>::new()));
    let limits = Rc::new(RefCell::new(Vec::<bool>::new()));
    let holds = Rc::new(RefCell::new(Vec::<bool>::new()));
    let requested = Rc::new(RefCell::new(Vec::<u32>::new()));
    let opens = Rc::new(Cell::new(0u32));

    power.connect_profile_activated({
        let profiles = Rc::clone(&profiles);
        move |_, index| profiles.borrow_mut().push(index)
    });
    power.connect_charge_limit_toggled({
        let limits = Rc::clone(&limits);
        move |_, on| limits.borrow_mut().push(on)
    });
    power.connect_hold_toggled({
        let holds = Rc::clone(&holds);
        move |_, on| holds.borrow_mut().push(on)
    });
    power.connect_hold_requested({
        let requested = Rc::clone(&requested);
        move |_, seconds| requested.borrow_mut().push(seconds)
    });
    power.connect_history_opened({
        let opens = Rc::clone(&opens);
        move |_| opens.set(opens.get() + 1)
    });

    let history = sample_history();
    fill_laptop(&power, &history);
    assert!(profiles.borrow().is_empty());
    assert!(limits.borrow().is_empty());
    assert!(holds.borrow().is_empty());
    assert!(requested.borrow().is_empty());
    assert_eq!(opens.get(), 0);

    assert_laptop(&power, &history);

    let rows = children_of::<ToggleRow>(&*power.imp().profiles);
    rows[0].activate();
    assert_eq!(profiles.borrow().as_slice(), &[0]);
    assert!(rows[0].active());
    assert!(!rows[1].active() && !rows[2].active());
    rows[0].activate();
    assert_eq!(profiles.borrow().as_slice(), &[0]);
    assert!(rows[0].active());
    assert!(!rows[1].active() && !rows[2].active());
    rows[2].activate();
    assert_eq!(profiles.borrow().as_slice(), &[0, 2]);
    assert!(rows[2].active() && !rows[0].active());

    let hold_head = child_named::<ActionRow>(&*power.imp().hold_card, "expander-row__head");
    hold_head.activate();
    assert!(power.imp().hold_card.open());
    for (row, seconds) in power.imp().presets() {
        row.activate();
        assert_eq!(*requested.borrow().last().unwrap(), seconds);
        let checked = power
            .imp()
            .presets()
            .into_iter()
            .filter(|(preset, _)| preset.active())
            .count();
        assert_eq!(checked, 1);
    }
    power.imp().preset_4h.activate();
    assert!(power.imp().preset_4h.active());
    assert_eq!(
        requested.borrow().as_slice(),
        &[900, 1_800, 3_600, 7_200, 14_400, 14_400]
    );
    power.collapse_hold();
    assert!(!power.imp().hold_card.open());

    child_named::<ActionRow>(&*power.imp().battery, "expander-row__head").activate();
    power.imp().charge_limit.activate();
    assert_eq!(limits.borrow().as_slice(), &[false]);
    assert!(!power.imp().charge_limit.active());

    power.imp().hold.activate();
    assert_eq!(holds.borrow().as_slice(), &[false]);
    assert!(!power.imp().hold.active());

    let history_head = child_named::<ActionRow>(&*power.imp().history, "expander-row__head");
    history_head.activate();
    assert_eq!(opens.get(), 1);
    assert!(power.imp().history.open());
    history_head.activate();
    assert_eq!(opens.get(), 1);
    assert!(!power.imp().history.open());
    history_head.activate();
    power.set_history(None);
    assert!(!power.imp().history.get_visible());
    assert!(!power.imp().history.open());
    assert_eq!(opens.get(), 2);

    assert_desktop();
    assert_edges();
    power.set_keep_awake_visible(false);
    assert!(!power.imp().keep.get_visible());
    power.set_keep_awake_visible(true);
    assert!(power.imp().keep.get_visible());
    assert_eq!(holds.borrow().as_slice(), &[false]);
}

fn assert_laptop(power: &PowerPopover, history: &PowerHistory) {
    assert!(power.has_css_class("power-popover"));
    assert!(
        power
            .layout_manager()
            .unwrap()
            .downcast::<gtk4::BinLayout>()
            .is_ok()
    );
    assert_eq!(power.imp().shell.title().as_deref(), Some("Power"));
    assert_eq!(
        power.imp().shell.icon_name().as_deref(),
        Some("battery-level-70-symbolic")
    );
    assert!(power.imp().readout.get_visible());
    assert_eq!(power.imp().readout.value().as_deref(), Some("76"));
    assert_eq!(power.imp().readout.unit().as_deref(), Some("%"));

    let notice = &power.imp().notice;
    assert!(notice.get_visible());
    assert_eq!(notice.severity(), Severity::Info);
    assert_eq!(notice.icon_name().as_deref(), Some("alarm-symbolic"));
    assert_eq!(notice.title().as_deref(), Some("Awake until 15:30"));
    assert_eq!(notice.subtitle().as_deref(), Some("Held by Firefox"));

    assert!(power.imp().remaining.get_visible());
    assert_eq!(power.imp().remaining.title().as_deref(), Some("Remaining"));
    assert_eq!(power.imp().remaining.value().as_deref(), Some("3 h 10 min"));

    assert!(power.imp().battery.get_visible());
    assert_eq!(power.imp().battery.title().as_deref(), Some("Battery"));
    let details = power.imp().battery.details();
    assert_eq!(
        details.first_child().unwrap(),
        power.imp().facts.clone().upcast::<gtk4::Widget>()
    );
    assert_eq!(
        details.last_child().unwrap(),
        power.imp().charge_limit.clone().upcast::<gtk4::Widget>()
    );
    let facts = children_of::<Row>(&*power.imp().facts);
    assert_eq!(facts.len(), 7);
    assert_eq!(facts[0].title().as_deref(), Some("Since unplugged"));
    assert_eq!(facts[0].value().as_deref(), Some("2 h 40 min"));
    assert_eq!(facts[0].tone(), RowTone::Normal);
    assert_eq!(facts[3].title().as_deref(), Some("Energy"));
    assert_eq!(facts[3].value().as_deref(), Some("41.2 / 47.5 Wh"));
    assert!(power.imp().charge_limit.get_visible());
    assert!(power.imp().charge_limit.active());
    assert_eq!(
        face(&*power.imp().charge_limit).title().as_deref(),
        Some("Limit charge to 80%")
    );

    assert!(power.imp().history.get_visible());
    assert_eq!(power.imp().history.title().as_deref(), Some("History"));
    let spark = power.imp().sparkline.imp();
    assert_eq!(spark.domain.get(), Some((0.0, 100.0)));
    assert!(spark.fill.get());
    assert_eq!(spark.values.borrow().as_slice(), history.values.as_slice());
    assert_eq!(spark.marked.borrow().as_slice(), history.marked.as_slice());
    assert_eq!(spark.ticks.borrow().as_slice(), history.ticks.as_slice());
    assert_eq!(
        spark.tooltips.borrow().as_slice(),
        history.tooltips.as_slice()
    );

    assert!(power.imp().profiles_section.get_visible());
    assert_eq!(
        power.imp().profiles_section.title().as_deref(),
        Some("Power mode")
    );
    let rows = children_of::<ToggleRow>(&*power.imp().profiles);
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| row.control() == ToggleControl::Check));
    assert!(rows[1].active() && !rows[0].active() && !rows[2].active());
    assert!(face(&rows[0]).subtitle().is_none());
    assert_eq!(
        face(&rows[2]).subtitle().as_deref(),
        Some("Unavailable: laptop on lap")
    );
    assert_eq!(face(&rows[2]).tone(), RowTone::Warning);
    assert_eq!(face(&rows[1]).tone(), RowTone::Normal);
    assert_eq!(
        face(&rows[1]).icon_name().as_deref(),
        Some("power-profile-balanced-symbolic")
    );

    assert!(power.imp().keep.get_visible());
    assert_eq!(power.imp().keep.title().as_deref(), Some("Keep awake"));
    assert!(power.imp().hold.active());
    assert_eq!(
        face(&*power.imp().hold).title().as_deref(),
        Some("Keep awake")
    );
    assert!(power.imp().preset_1h.active());
    assert!(!power.imp().preset_15m.active());
    assert!(!power.imp().preset_4h.active());

    assert!(power.imp().devices_section.get_visible());
    let devices = children_of::<Row>(&*power.imp().devices_box);
    assert_eq!(devices.len(), 3);
    assert_eq!(devices[0].title().as_deref(), Some("MX Keys S"));
    assert_eq!(devices[0].value().as_deref(), Some("54%"));
    assert_eq!(devices[0].tone(), RowTone::Normal);
    assert_eq!(devices[2].title().as_deref(), Some("Pixel 8"));
    assert_eq!(devices[2].tone(), RowTone::Warning);
    assert!(children_of::<ToggleRow>(&*power.imp().devices_box).is_empty());

}

fn face(row: &impl IsA<Row>) -> &Row {
    row.upcast_ref()
}

fn assert_desktop() {
    let power = PowerPopover::new();
    power.set_header("power-profile-balanced-symbolic", None);
    power.set_notice(None);
    power.set_remaining(None);
    power.set_battery(&[]);
    power.set_charge_limit(None);
    power.set_history(None);
    power.set_profiles(&profiles(), Some(1));
    power.set_devices(&devices());
    power.set_hold_active(false);

    assert!(!power.imp().readout.get_visible());
    assert!(!power.imp().notice.get_visible());
    assert!(!power.imp().remaining.get_visible());
    assert!(!power.imp().battery.get_visible());
    assert!(!power.imp().history.get_visible());
    assert!(power.imp().profiles_section.get_visible());
    assert!(power.imp().keep.get_visible());
    assert!(power.imp().devices_section.get_visible());

    assert_eq!(
        power.imp().shell.icon_name().as_deref(),
        Some("power-profile-balanced-symbolic")
    );
}

fn assert_edges() {
    let power = PowerPopover::new();
    let writes = Rc::new(Cell::new(0u32));
    power.imp().readout.connect_notify_local(Some("value"), {
        let writes = Rc::clone(&writes);
        move |_, _| writes.set(writes.get() + 1)
    });
    power.set_header("battery-symbolic", Some(76));
    assert_eq!(writes.get(), 1);
    power.set_header("battery-symbolic", Some(76));
    assert_eq!(writes.get(), 1);
    power.set_header("battery-symbolic", Some(70));
    assert_eq!(writes.get(), 2);
    power.set_header("battery-symbolic", None);
    assert!(!power.imp().readout.get_visible());

    power.set_remaining(Some(("Remaining", "3 h 10 min")));
    assert!(power.imp().remaining.get_visible());
    power.set_remaining(None);
    assert!(!power.imp().remaining.get_visible());

    power.set_notice(Some(&PowerHoldNotice {
        title: "Awake until 15:30".into(),
        subtitle: "Held by Firefox".into(),
    }));
    assert!(power.imp().notice.get_visible());
    power.set_notice(None);
    assert!(!power.imp().notice.get_visible());

    power.set_battery(&[Fact::new("Health", "87%")]);
    assert!(power.imp().battery.get_visible());
    assert!(!power.imp().charge_limit.get_visible());
    child_named::<ActionRow>(&*power.imp().battery, "expander-row__head").activate();
    assert!(power.imp().battery.open());
    power.set_battery(&[]);
    assert!(!power.imp().battery.get_visible());
    assert!(!power.imp().battery.open());

    power.set_charge_limit(Some(&PowerChargeLimit {
        enabled: false,
        title: "Limit charge to 80%".into(),
    }));
    assert!(power.imp().battery.get_visible());
    assert!(power.imp().charge_limit.get_visible());
    assert!(!power.imp().charge_limit.active());
    power.set_charge_limit(None);
    power.set_battery(&[]);
    assert!(!power.imp().battery.get_visible());

    let emitted = Rc::new(Cell::new(0u32));
    power.connect_profile_activated({
        let emitted = Rc::clone(&emitted);
        move |_, _| emitted.set(emitted.get() + 1)
    });
    power.set_profiles(&profiles(), Some(0));
    assert_eq!(emitted.get(), 0);
    assert!(power.imp().profiles_section.get_visible());
    power.set_profiles(&[], None);
    assert_eq!(emitted.get(), 0);
    assert!(!power.imp().profiles_section.get_visible());

    let listed = devices();
    power.set_devices(&listed);
    let first = children_of::<Row>(&*power.imp().devices_box);
    power.set_devices(&listed);
    let again = children_of::<Row>(&*power.imp().devices_box);
    assert_eq!(first, again);
    let long = "я".repeat(TEXT_MAX_CHARS + 40);
    power.set_devices(&[PowerDevice {
        name: long,
        icon_name: "phone-symbolic".into(),
        value: "12%".into(),
        warning: true,
    }]);
    let truncated = children_of::<Row>(&*power.imp().devices_box);
    assert_eq!(
        truncated[0].title().unwrap().chars().count(),
        TEXT_MAX_CHARS
    );
    assert_eq!(truncated[0].tone(), RowTone::Warning);
    power.set_devices(&[]);
    assert!(!power.imp().devices_section.get_visible());
    assert!(children_of::<Row>(&*power.imp().devices_box).is_empty());
}

fn fill_laptop(power: &PowerPopover, history: &PowerHistory) {
    power.set_header("battery-level-70-symbolic", Some(76));
    power.set_notice(Some(&PowerHoldNotice {
        title: "Awake until 15:30".into(),
        subtitle: "Held by Firefox".into(),
    }));
    power.set_remaining(Some(("Remaining", "3 h 10 min")));
    power.set_battery(&[
        Fact::new("Since unplugged", "2 h 40 min"),
        Fact::new("Power draw", "8.4 W"),
        Fact::new("Health", "87%"),
        Fact::new("Energy", "41.2 / 47.5 Wh"),
        Fact::new("Capacity when new", "54.0 Wh"),
        Fact::new("Cycles", "312"),
        Fact::new("Technology", "Lithium-ion"),
    ]);
    power.set_charge_limit(Some(&PowerChargeLimit {
        enabled: true,
        title: "Limit charge to 80%".into(),
    }));
    power.set_history(Some(history));
    power.set_profiles(&profiles(), Some(1));
    power.set_hold_active(true);
    power.set_hold_preset(Some(3_600));
    power.set_devices(&devices());

}

fn profiles() -> Vec<Choice> {
    vec![
        Choice {
            icon_name: "power-profile-power-saver-symbolic".into(),
            label: "Power saver".into(),
            ..Choice::default()
        },
        Choice {
            icon_name: "power-profile-balanced-symbolic".into(),
            label: "Balanced".into(),
            ..Choice::default()
        },
        Choice {
            icon_name: "power-profile-performance-symbolic".into(),
            label: "Performance".into(),
            detail: "Unavailable: laptop on lap".into(),
            warning: true,
        },
    ]
}

fn devices() -> Vec<PowerDevice> {
    vec![
        PowerDevice {
            name: "MX Keys S".into(),
            icon_name: "input-keyboard-symbolic".into(),
            value: "54%".into(),
            warning: false,
        },
        PowerDevice {
            name: "WH-1000XM4".into(),
            icon_name: "audio-headphones-symbolic".into(),
            value: "80%".into(),
            warning: false,
        },
        PowerDevice {
            name: "Pixel 8".into(),
            icon_name: "phone-symbolic".into(),
            value: "12%".into(),
            warning: true,
        },
    ]
}

fn sample_history() -> PowerHistory {
    PowerHistory {
        values: vec![Some(90.0), Some(70.0), Some(55.0), Some(76.0)],
        marked: vec![false, true, true, false],
        ticks: vec![
            Tick {
                at: 0.0,
                label: "16:00".into(),
            },
            Tick {
                at: 0.25,
                label: "22:00".into(),
            },
            Tick {
                at: 0.5,
                label: "04:00".into(),
            },
            Tick {
                at: 0.75,
                label: "10:00".into(),
            },
            Tick {
                at: 1.0,
                label: "Now".into(),
            },
        ],
        tooltips: vec![
            Some("90%".into()),
            Some("70%".into()),
            None,
            Some("76%".into()),
        ],
    }
}
