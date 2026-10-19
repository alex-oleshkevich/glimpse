use super::*;
use std::cell::{Cell, RefCell};

thread_local! {
    static SEEN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static EXPLODE: Cell<bool> = const { Cell::new(false) };
    static CONFIGURED: Cell<u32> = const { Cell::new(0) };
    static SHOWN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

struct Probe;

impl Applet for Probe {
    fn orient(&mut self, _orientation: gtk4::Orientation) {
        SEEN.with(|seen| seen.borrow_mut().push("Oriented".to_owned()));
    }

    fn configure(&mut self, _ctx: &Ctx, _config: &AppletConfig) {
        if EXPLODE.with(Cell::get) {
            panic!("the probe exploded while configuring");
        }
        CONFIGURED.set(CONFIGURED.get() + 1);
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        if EXPLODE.with(Cell::get) {
            panic!("the probe exploded");
        }
        if let Input::Pointer(pointer) = input {
            SEEN.with(|seen| seen.borrow_mut().push(format!("{pointer:?}")));
        }
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        SHOWN.with(|shown| shown.borrow().iter().map(|label| spec(label)).collect())
    }
}

struct Raiser;

impl Applet for Raiser {
    fn handle(&mut self, _ctx: &Ctx, _input: &Input) {}

    fn indicators(&self) -> Vec<IndicatorSpec> {
        vec![spec("raiser")]
    }

    fn popover(&mut self, _seat: &Seat) -> Option<gtk4::Widget> {
        Some(gtk4::Label::new(Some("popover")).upcast())
    }
}

struct Strip;

impl Applet for Strip {
    fn configure(&mut self, _ctx: &Ctx, _config: &AppletConfig) {
        if EXPLODE.with(Cell::get) {
            panic!("the strip exploded while configuring");
        }
    }

    fn handle(&mut self, _ctx: &Ctx, _input: &Input) {}

    fn view(&mut self, _ctx: &Ctx) -> Option<gtk4::Widget> {
        let own = gtk4::Label::new(Some("strip"));
        own.add_css_class("own-view");
        Some(own.upcast())
    }
}

fn group(handle: &AppletHandle) -> IndicatorGroup {
    handle
        .widget
        .first_child()
        .and_downcast::<IndicatorGroup>()
        .expect("an applet with no view of its own gets the group")
}

fn settle() {
    let context = glib::MainContext::default();
    for _ in 0..64 {
        while context.iteration(false) {}
    }
}

fn seen() -> Vec<String> {
    SEEN.with(|seen| std::mem::take(&mut *seen.borrow_mut()))
}

fn press(group: &IndicatorGroup, button: u32) {
    group.emit_by_name::<()>("pressed", &[&button]);
    settle();
}

fn scroll(group: &IndicatorGroup, dy: f64) {
    group.emit_by_name::<()>("scrolled", &[&0.0f64, &dy]);
    settle();
}

fn config(format: &str) -> AppletConfig {
    glimpse_config::AppletKind::Clock(glimpse_config::ClockConfig {
        label_format: format.to_owned(),
        ..Default::default()
    })
    .into()
}

fn shown(labels: &[&str]) {
    SHOWN.with(|s| *s.borrow_mut() = labels.iter().map(|label| (*label).to_owned()).collect());
}

fn drained(deltas: &[f64]) -> Vec<Direction> {
    let mut accumulated = 0.0;
    let mut out = Vec::new();
    for delta in deltas {
        drain(
            &mut accumulated,
            *delta,
            Direction::Up,
            Direction::Down,
            &mut out,
        );
    }
    out
}

fn spec(label: &str) -> IndicatorSpec {
    IndicatorSpec {
        label: Some(label.to_owned()),
        ..Default::default()
    }
}

fn slot(name: &str) -> SlotInfo {
    SlotInfo {
        connector: "DP-1".to_owned(),
        position: Position::Left,
        zone: Zone::End,
        index: 2,
        name: name.to_owned(),
    }
}

fn lookup<T: glib::variant::FromVariant>(dict: &glib::Variant, key: &str) -> Option<T> {
    glib::VariantDict::new(Some(dict))
        .lookup::<T>(key)
        .ok()
        .flatten()
}

#[test]
fn an_indicator_status_is_the_spec_flattened_and_capped() {
    let status = IndicatorStatus::from_spec(&IndicatorSpec {
        icon: Some(gio::ThemedIcon::new("audio-volume-high").upcast()),
        dot: Some(gtk4::gdk::RGBA::new(1.0, 0.0, 0.5, 0.25)),
        label: Some("x".repeat(300)),
        tooltip: Some("y".repeat(300)),
        badge: Some("3".to_owned()),
        attention: true,
        severity: Some(Severity::Warning),
        class: Some("cast".to_owned()),
        ..Default::default()
    });

    assert_eq!(status.icon.as_deref(), Some("audio-volume-high"));
    assert_eq!(status.overlay, None);
    assert_eq!(status.dot.as_deref(), Some("#ff008040"));
    assert_eq!(status.label, Some(format!("{}…", "x".repeat(64))));
    assert_eq!(status.tooltip, Some(format!("{}…", "y".repeat(256))));
    assert_eq!(status.badge.as_deref(), Some("3"));
    assert!(status.attention && !status.notice);
    assert_eq!(status.severity, Some("warning"));
    assert_eq!(status.class.as_deref(), Some("cast"));

    let texture = gtk4::gdk::MemoryTexture::new(
        1,
        1,
        gtk4::gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from_static(&[0; 4]),
        4,
    );
    let spec = IndicatorSpec {
        icon: Some(texture.upcast()),
        ..Default::default()
    };
    assert_eq!(
        IndicatorStatus::from_spec(&spec).icon,
        None,
        "a texture has no name"
    );

    for (severity, name) in [
        (Severity::Info, "info"),
        (Severity::Warning, "warning"),
        (Severity::Error, "error"),
    ] {
        let spec = IndicatorSpec {
            severity: Some(severity),
            ..Default::default()
        };
        assert_eq!(IndicatorStatus::from_spec(&spec).severity, Some(name));
    }

    assert_eq!(
        IndicatorStatus::from_spec(&IndicatorSpec::default()),
        IndicatorStatus::default(),
        "an empty spec leaves every optional field out"
    );
}

#[test]
fn an_applet_status_omits_what_it_does_not_have() {
    let mut status = AppletStatus {
        slot: slot("clock-utc"),
        kind: Some("clock".to_owned()),
        alive: true,
        own_view: false,
        popover: true,
        indicators: vec![IndicatorStatus {
            label: Some("12:00".to_owned()),
            ..Default::default()
        }],
    };
    let variant = status.to_variant();
    assert_eq!(variant.type_(), glib::VariantTy::VARDICT);
    assert_eq!(
        lookup::<String>(&variant, "connector").as_deref(),
        Some("DP-1")
    );
    assert_eq!(
        lookup::<String>(&variant, "position").as_deref(),
        Some("left")
    );
    assert_eq!(lookup::<String>(&variant, "zone").as_deref(), Some("right"));
    assert_eq!(lookup::<u32>(&variant, "index"), Some(2));
    assert_eq!(
        lookup::<String>(&variant, "name").as_deref(),
        Some("clock-utc")
    );
    assert_eq!(lookup::<String>(&variant, "kind").as_deref(), Some("clock"));
    assert_eq!(lookup::<bool>(&variant, "alive"), Some(true));
    assert_eq!(lookup::<bool>(&variant, "own-view"), Some(false));
    assert_eq!(lookup::<bool>(&variant, "popover"), Some(true));
    let indicators = glib::VariantDict::new(Some(&variant))
        .lookup_value(
            "indicators",
            Some(glib::VariantTy::new("aa{sv}").expect("type")),
        )
        .expect("indicators");
    assert_eq!(indicators.n_children(), 1);
    let chip = indicators.child_value(0);
    assert_eq!(lookup::<String>(&chip, "label").as_deref(), Some("12:00"));
    assert_eq!(lookup::<bool>(&chip, "attention"), Some(false));
    for key in [
        "icon", "overlay", "dot", "tooltip", "badge", "severity", "class",
    ] {
        assert!(lookup::<String>(&chip, key).is_none(), "{key} is omitted");
    }

    status.kind = None;
    status.alive = false;
    status.indicators.clear();
    let variant = status.to_variant();
    assert!(lookup::<String>(&variant, "kind").is_none());
    assert_eq!(lookup::<bool>(&variant, "alive"), Some(false));
    let indicators = glib::VariantDict::new(Some(&variant))
        .lookup_value("indicators", None)
        .expect("indicators");
    assert_eq!(indicators.n_children(), 0);
}

#[test]
fn a_command_maps_only_well_typed_verbs() {
    let command = |verb: &str, argument: glib::Variant| {
        super::command(Some(&(verb.to_owned(), argument).to_variant()))
            .map(|input| format!("{input:?}"))
    };
    assert_eq!(
        command("click", 3u32.to_variant()).as_deref(),
        Some("Pressed { button: 3 }")
    );
    assert_eq!(
        command("scroll", (0.0f64, 1.0f64).to_variant()).as_deref(),
        Some("Scrolled { dx: 0.0, dy: 1.0 }")
    );
    assert_eq!(
        command("open", ().to_variant()).as_deref(),
        Some("PopoverRequested")
    );
    assert_eq!(
        command("close", ().to_variant()).as_deref(),
        Some("PopoverLowered")
    );
    assert_eq!(
        command("toggle", ().to_variant()).as_deref(),
        Some("PopoverToggled")
    );
    assert!(command("click", "3".to_variant()).is_none());
    assert!(command("scroll", 1.0f64.to_variant()).is_none());
    assert!(command("forget", ().to_variant()).is_none());
    assert!(command("scroll", (0.0f64, f64::INFINITY).to_variant()).is_none());
    assert!(command("scroll", (f64::NAN, 0.0f64).to_variant()).is_none());
    assert_eq!(
        command("scroll", (-1e9f64, 1e9f64).to_variant()).as_deref(),
        Some("Scrolled { dx: -16.0, dy: 16.0 }")
    );
    assert!(super::command(None).is_none());
}

#[test]
fn a_partial_gesture_is_carried_across_events_for_the_whole_group() {
    let mut scroll = Scroll::default();

    assert!(
        scroll.notches(0.0, 0.6).is_empty(),
        "0.6 is not a notch yet"
    );
    assert_eq!(
        scroll.notches(0.0, 0.6),
        [Direction::Down],
        "the carried remainder completes the notch"
    );
    assert!(scroll.notches(0.0, 0.3).is_empty());
}

#[test]
fn the_two_axes_accumulate_independently() {
    let mut scroll = Scroll::default();

    assert!(scroll.notches(0.6, 0.6).is_empty());
    assert_eq!(
        scroll.notches(0.6, 0.0),
        [Direction::Right],
        "a horizontal notch must not be completed by vertical travel"
    );
}

#[test]
fn a_wheel_detent_is_one_notch() {
    assert_eq!(drained(&[1.0]), [Direction::Down]);
    assert_eq!(drained(&[-1.0]), [Direction::Up]);
}

#[test]
fn a_touchpad_coalesces_into_whole_notches() {
    assert!(
        drained(&[0.4, 0.4]).is_empty(),
        "a partial gesture moves nothing"
    );
    assert_eq!(drained(&[0.4, 0.4, 0.4]), [Direction::Down]);
    assert_eq!(
        drained(&[0.4; 10]).len(),
        4,
        "four whole notches out of 4.0, not ten events"
    );
}

#[test]
fn a_reversal_cancels_rather_than_firing_both_ways() {
    assert!(drained(&[0.6, -0.6]).is_empty());
    assert_eq!(drained(&[0.6, -1.6]), [Direction::Up]);
}

#[test]
fn one_burst_larger_than_a_notch_fires_every_notch_it_contains() {
    assert_eq!(
        drained(&[2.5]),
        [Direction::Down, Direction::Down],
        "the remainder is carried, not discarded"
    );
    assert_eq!(drained(&[2.5, 0.5]), [Direction::Down; 3]);
}

fn every_weather_icon_the_applet_can_ask_for_exists() {
    use glimpse_dbus::weather::Condition;

    const EVERY: [Condition; 21] = [
        Condition::ClearSky,
        Condition::MainlyClear,
        Condition::PartlyCloudy,
        Condition::Overcast,
        Condition::Fog,
        Condition::Drizzle,
        Condition::FreezingDrizzle,
        Condition::LightRain,
        Condition::Rain,
        Condition::HeavyRain,
        Condition::FreezingRain,
        Condition::LightSnow,
        Condition::Snow,
        Condition::HeavySnow,
        Condition::SnowGrains,
        Condition::Sleet,
        Condition::RainShowers,
        Condition::SnowShowers,
        Condition::Thunderstorm,
        Condition::ThunderstormWithHail,
        Condition::Unknown,
    ];

    let theme = gtk4::IconTheme::for_display(&gtk4::gdk::Display::default().expect("display"));
    let mut names = vec![crate::applets::weather::render::ALERT_ICON];
    for condition in EVERY {
        for is_day in [true, false] {
            names.push(condition.icon_name(is_day));
        }
    }
    names.sort_unstable();
    names.dedup();

    for name in names {
        assert!(theme.has_icon(name), "the icon theme has no `{name}`");
    }
}

#[test]
#[ignore = "needs a display"]
fn an_applet_reaches_its_group() {
    gtk4::init().expect("display");
    glimpse_widgets::register_resources().expect("resources");

    every_weather_icon_the_applet_can_ask_for_exists();

    shown(&["a", "b"]);
    let handle = AppletHandle::launch(
        "probe".to_owned(),
        Some("DP-1".to_owned()),
        Box::new(|_| Box::new(Probe)),
        config("%H:%M"),
        Catcher::new(None, glimpse_config::Position::Top),
        slot("probe"),
    );
    settle();

    assert_eq!(CONFIGURED.get(), 1, "settings reach the applet at launch");
    handle.configure(config("%H:%M"));
    settle();
    assert_eq!(
        CONFIGURED.get(),
        1,
        "an unchanged configuration is not handed to the applet again"
    );
    handle.configure(config("%H:%M:%S"));
    settle();
    assert_eq!(CONFIGURED.get(), 2, "a changed one is");

    assert!(
        group(&handle).first_child().is_some(),
        "the group renders what indicators() returned"
    );

    handle.set_orientation(gtk4::Orientation::Vertical);
    settle();
    assert_eq!(
        group(&handle)
            .layout_manager()
            .and_downcast::<gtk4::BoxLayout>()
            .expect("group layout")
            .orientation(),
        gtk4::Orientation::Vertical
    );
    assert!(seen().is_empty());

    press(&group(&handle), 3);
    assert_eq!(
        seen(),
        ["Press(Right)"],
        "a press carries the decoded button and nothing about which chip it landed on"
    );

    scroll(&group(&handle), 0.6);
    assert!(seen().is_empty(), "a partial gesture reaches no applet");

    scroll(&group(&handle), 0.6);
    assert_eq!(seen(), ["Scroll(Down)"], "the notch arrives once, whole");

    scroll(&group(&handle), 0.7);
    assert!(seen().is_empty(), "0.9 of a notch is still no notch");

    shown(&["a"]);
    press(&group(&handle), 1);
    assert_eq!(seen(), ["Press(Left)"]);

    scroll(&group(&handle), 0.3);
    assert_eq!(
        seen(),
        ["Scroll(Down)"],
        "the group keeps accumulating across a re-render, since no chip owns the gesture"
    );

    EXPLODE.set(true);
    press(&group(&handle), 1);
    assert!(
        group(&handle).first_child().is_none(),
        "a panicking applet empties its group"
    );

    press(&group(&handle), 1);
    assert!(
        seen().is_empty(),
        "a stopped applet receives no further input"
    );

    let exploding = AppletHandle::launch(
        "exploding".to_owned(),
        None,
        Box::new(|_| Box::new(Probe)),
        config("%H"),
        Catcher::new(None, glimpse_config::Position::Top),
        slot("exploding"),
    );
    settle();
    assert!(
        group(&exploding).first_child().is_none(),
        "an applet that panics while configuring is stopped too, not only one that panics \
             while handling"
    );
    EXPLODE.set(false);

    let strip = AppletHandle::launch(
        "strip".to_owned(),
        None,
        Box::new(|_| Box::new(Strip)),
        config("%H"),
        Catcher::new(None, glimpse_config::Position::Top),
        slot("strip"),
    );
    settle();

    let own = strip.widget.first_child().expect("the view is parented");
    assert!(
        own.has_css_class("own-view"),
        "an applet that supplies a view gets that widget, not an IndicatorGroup wrapping it"
    );
    assert!(
        own.downcast_ref::<IndicatorGroup>().is_none(),
        "and the group is never built, so indicators() is never asked for"
    );

    strip.set_orientation(gtk4::Orientation::Vertical);
    settle();
    assert_eq!(
        strip.widget.orientation(),
        gtk4::Orientation::Vertical,
        "a vertical bar has to reach an applet that renders its own strip"
    );

    EXPLODE.set(true);
    strip.configure(config("%M"));
    settle();
    EXPLODE.set(false);
    assert!(
        strip.widget.first_child().is_none(),
        "a panicking applet gives its space back whether the group or the applet owned the \
             widget; only the group empties itself"
    );

    a_raise_opens_a_popover_and_never_closes_one();
    a_bluetooth_popover_signal_reaches_the_service();
    an_applet_action_injects_what_a_pointer_would();
    a_clock_interval_repaints_its_group();
    crate::applet::catcher::tests::every_arrow_paints_over_the_body_on_its_panel_side();
    crate::applet::catcher::tests::a_typing_popover_takes_the_keyboard_without_a_click();
    crate::applet::catcher::tests::only_the_body_sized_surface_is_blurred();
    crate::applet::catcher::tests::the_popover_surface_follows_its_body();
}

fn an_applet_action_injects_what_a_pointer_would() {
    let app = relm4::main_application();
    if !app.is_registered() {
        app.register(gio::Cancellable::NONE).expect("register");
    }
    let _handle = AppletHandle::launch(
        "actioned".to_owned(),
        None,
        Box::new(|_| Box::new(Probe)),
        config("%H"),
        Catcher::new(None, glimpse_config::Position::Top),
        slot("actioned"),
    );
    settle();
    seen();

    let actions: Vec<String> = app
        .list_actions()
        .iter()
        .map(|name| name.to_string())
        .filter(|name| name.starts_with("applet."))
        .collect();
    assert!(
        !actions.iter().any(|name| {
            app.action_state(name)
                .is_some_and(|state| lookup::<String>(&state, "name").as_deref() == Some("raiser"))
        }),
        "a dropped runtime removes its action"
    );
    let name = actions
        .into_iter()
        .find(|name| {
            app.action_state(name).is_some_and(|state| {
                lookup::<String>(&state, "name").as_deref() == Some("actioned")
            })
        })
        .expect("the runtime exports applet.<serial>");
    let state = app.action_state(&name).expect("state");
    assert_eq!(lookup::<bool>(&state, "alive"), Some(true));
    assert_eq!(lookup::<bool>(&state, "own-view"), Some(false));
    assert_eq!(lookup::<String>(&state, "kind").as_deref(), Some("clock"));

    app.activate_action(&name, Some(&("click", 3u32.to_variant()).to_variant()));
    settle();
    assert_eq!(seen(), ["Press(Right)"], "a click takes the pointer's path");

    app.activate_action(
        &name,
        Some(&("scroll", (0.0f64, 1.0f64).to_variant()).to_variant()),
    );
    settle();
    assert_eq!(seen(), ["Scroll(Down)"]);

    app.activate_action(&name, Some(&("forget", 3u32.to_variant()).to_variant()));
    settle();
    assert!(seen().is_empty(), "an unknown verb does nothing");

    let forged = glib::VariantDict::new(None);
    forged.insert_value("alive", &false.to_variant());
    app.change_action_state(&name, &forged.end());
    settle();
    assert_eq!(
        app.action_state(&name).as_ref(),
        Some(&state),
        "SetState cannot overwrite the status"
    );

    EXPLODE.set(true);
    app.activate_action(&name, Some(&("click", 1u32.to_variant()).to_variant()));
    settle();
    EXPLODE.set(false);
    let dead = app.action_state(&name).expect("state");
    assert_eq!(lookup::<bool>(&dead, "alive"), Some(false));
    assert_eq!(
        glib::VariantDict::new(Some(&dead))
            .lookup_value("indicators", None)
            .expect("indicators")
            .n_children(),
        0,
        "a stopped applet publishes no indicators"
    );
    app.activate_action(&name, Some(&("click", 1u32.to_variant()).to_variant()));
    settle();
    assert!(seen().is_empty(), "a stopped applet takes no action input");
}

fn a_raise_opens_a_popover_and_never_closes_one() {
    let catcher = Catcher::new(None, glimpse_config::Position::Top);
    let handle = AppletHandle::launch(
        "raiser".to_owned(),
        None,
        Box::new(|_| Box::new(Raiser)),
        config("%H"),
        Rc::clone(&catcher),
        slot("raiser"),
    );
    settle();

    let send = |input| {
        let _ = handle._controller.sender().send(input);
        settle();
    };
    let raise = || send(HostInput::PopoverRequested);

    raise();
    assert!(!catcher.closing(), "a raise opens the popover");

    raise();
    assert!(
        !catcher.closing(),
        "a second raise leaves it open rather than toggling it shut"
    );

    catcher.close();
    assert!(
        catcher.closing(),
        "the fade holds the child until it finishes"
    );
    raise();
    assert!(
        !catcher.closing(),
        "a raise arriving during the close fade reopens rather than being swallowed"
    );

    send(HostInput::PopoverToggled);
    assert!(
        catcher.closing(),
        "an applet owning its view has no press to ride on and asks for the toggle by name"
    );

    raise();
    press(&group(&handle), 1);
    assert!(catcher.closing(), "a press still toggles");
}

fn a_bluetooth_popover_signal_reaches_the_service() {
    use crate::applet::popover::Seat;
    use crate::applets::bluetooth::Bluetooth as BluetoothApplet;
    use glimpse_dbus::Buses;
    use glimpse_services::{BluetoothDependencies, Confirmation, DeviceId, Running};
    use glimpse_widgets::BluetoothPopover;

    const DEVICE: &str = "/org/bluez/hci0/dev_F8_4E_17_BC_EE_D5";

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let guard = runtime.enter();
    let (mut service, bluetooth) = Running::<glimpse_services::Bluetooth>::spawn(
        &glimpse_config::Config::default(),
        Buses::unavailable("no bus in tests"),
        BluetoothDependencies { agent: true },
    );

    let notifications =
        glimpse_dbus::notifications::NotificationsProvider::unavailable("no bus in tests").handle();
    let mut applet = BluetoothApplet::start(bluetooth.clone(), notifications);
    let (host, _receiver) = relm4::channel();
    let shown = applet
        .popover(&Seat::new(host))
        .expect("the applet opens a popover");
    let popover = shown
        .downcast::<BluetoothPopover>()
        .expect("the bluetooth applet's popover is its own widget");

    let settled = |wanted: Confirmation| {
        for _ in 0..200 {
            if bluetooth.snapshot().confirm.as_ref() == Some(&wanted) {
                return true;
            }
            settle();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        false
    };

    popover.emit_by_name::<()>("acted", &[&DEVICE.to_owned(), &"forget".to_owned()]);
    assert!(
        settled(Confirmation::Forget {
            device: DeviceId::new(DEVICE),
            connected: false,
        }),
        "a forget row must reach the service as an unconfirmed forget, not as a forget"
    );

    drop(guard);
    runtime.block_on(service.stop());
}

fn a_clock_interval_repaints_its_group() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let services = {
        let _guard = runtime.enter();
        crate::services::PanelServices::start_with_buses(
            &glimpse_config::Config::default(),
            glimpse_dbus::Buses::unavailable("no bus in tests"),
        )
    };
    let settings = config("%H:%M:%S");
    let build = crate::applets::build(
        &settings,
        &services.handles,
        glimpse_services::Placement {
            output: None,
            position: glimpse_services::Edge::Top,
            orientation: glimpse_services::Orientation::Horizontal,
            zone: glimpse_services::Zone::Center,
            size: 32,
        },
        None,
    )
    .expect("the clock builder");
    struct Ticking {
        applet: Box<dyn Applet>,
        ticks: Rc<Cell<u32>>,
    }

    impl Applet for Ticking {
        fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
            self.applet.configure(ctx, config);
        }

        fn handle(&mut self, ctx: &Ctx, input: &Input) {
            if matches!(input, Input::Tick) {
                self.ticks.set(self.ticks.get() + 1);
            }
            self.applet.handle(ctx, input);
        }

        fn indicators(&self) -> Vec<IndicatorSpec> {
            self.applet.indicators()
        }
    }

    let ticks = Rc::new(Cell::new(0));
    let received = Rc::clone(&ticks);
    let handle = AppletHandle::launch(
        "ticking-clock".to_owned(),
        None,
        Box::new(move |ctx| {
            Box::new(Ticking {
                applet: build(ctx),
                ticks: received,
            })
        }),
        settings,
        Catcher::new(None, Position::Top),
        slot("ticking-clock"),
    );
    let window = gtk4::Window::new();
    window.set_child(Some(&handle.widget));
    window.present();
    settle();

    let mut pending = vec![group(&handle).upcast::<gtk4::Widget>()];
    let mut label = None;
    while let Some(widget) = pending.pop() {
        if let Some(found) = widget.downcast_ref::<gtk4::Label>()
            && !found.text().is_empty()
        {
            label = Some(found.clone());
            break;
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            pending.push(widget);
        }
    }
    let label = label.expect("the clock renders a label");
    let before = label.text();
    assert!(!before.is_empty());
    glib::MainContext::default().block_on(async {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while (label.text() == before || ticks.get() == 0) && std::time::Instant::now() < deadline {
            glib::timeout_future(std::time::Duration::from_millis(10)).await;
        }
    });
    assert_ne!(
        label.text(),
        before,
        "the configured interval repaints the clock group"
    );
    assert!(
        ticks.get() > 0,
        "the interval delivers Input::Tick to the clock"
    );
    window.close();
    drop(handle);
    settle();
    runtime.block_on(services.shutdown());
}
