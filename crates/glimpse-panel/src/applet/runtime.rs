use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};

use glimpse_config::Applet as AppletConfig;
use glimpse_widgets::IndicatorGroup;
use std::rc::Rc;

use super::catcher::Catcher;
use gtk4::prelude::*;

use super::popover::{PopoverHandle, Seat};
use relm4::{Component, ComponentController, ComponentParts, ComponentSender, Controller};

use super::{Applet, Button, Ctx, Direction, Input, Pointer};

const NOTCH: f64 = 1.0;

pub type Builder = Box<dyn FnOnce(&Ctx) -> Box<dyn Applet>>;

pub struct AppletInit {
    pub name: String,
    pub output: Option<String>,
    pub build: Builder,
    pub config: AppletConfig,
    pub catcher: Rc<Catcher>,
}

#[derive(Debug)]
pub enum HostInput {
    Configured(AppletConfig),
    PopoverRequested,
    PopoverClosed,
    PopoverDismissed,
    Oriented(gtk4::Orientation),
    Pressed { button: u32 },
    Woken,
    Scrolled { dx: f64, dy: f64 },
    Ticked,
}

pub struct AppletRuntime {
    applet: Option<Box<dyn Applet>>,
    ctx: Ctx,
    seat: Seat,
    root: gtk4::Box,
    group: Option<IndicatorGroup>,
    scroll: Scroll,
    config: Option<AppletConfig>,
    catcher: Rc<Catcher>,
    shown: Option<Box<dyn PopoverHandle>>,
    anchored: Option<i32>,
}

impl Component for AppletRuntime {
    type Init = AppletInit;
    type Input = HostInput;
    type Output = ();
    type CommandOutput = ();
    type Root = gtk4::Box;
    type Widgets = ();

    fn init_root() -> Self::Root {
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        root.add_css_class("applet");
        root
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let build = init.build;
        let seat = Seat::new(sender.input_sender().clone());
        let ctx = Ctx::new(init.name, init.output, sender.input_sender().clone());
        let mut applet = build(&ctx);

        let group = match applet.view(&ctx) {
            Some(view) => {
                root.append(&view);
                None
            }
            None => {
                let group = IndicatorGroup::new();
                group.connect_pressed({
                    let sender = sender.clone();
                    move |_, button| sender.input(HostInput::Pressed { button })
                });
                group.connect_scrolled({
                    let sender = sender.clone();
                    move |_, dx, dy| sender.input(HostInput::Scrolled { dx, dy })
                });
                root.append(&group);
                Some(group)
            }
        };

        tracing::debug!(applet = ctx.name(), own_view = group.is_none(), "started");

        let mut model = AppletRuntime {
            applet: Some(applet),
            ctx,
            seat,
            root,
            group,
            scroll: Scroll::default(),
            config: None,
            catcher: init.catcher,
            shown: None,
            anchored: None,
        };
        model.configure(init.config);
        model.deliver(None);

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            HostInput::PopoverRequested => self.show_popover(&sender),
            HostInput::PopoverClosed => {
                if self.owns() {
                    self.catcher.close();
                }
            }
            HostInput::PopoverDismissed => {
                if !self.owns() {
                    self.shown = None;
                    self.anchored = None;
                }
            }
            HostInput::Configured(settings) => self.configure(settings),
            HostInput::Oriented(orientation) => self.orient(orientation),
            HostInput::Ticked => self.deliver(Some(&Input::Tick)),
            HostInput::Woken => self.deliver(Some(&Input::Woken)),
            HostInput::Pressed { button } => {
                let button = Button::from_code(button);
                self.deliver(Some(&Input::Pointer(Pointer::Press(button))));
                if button == Button::Left {
                    self.show_popover(&sender);
                }
            }
            HostInput::Scrolled { dx, dy } => {
                for direction in self.scroll.notches(dx, dy) {
                    self.deliver(Some(&Input::Pointer(Pointer::Scroll(direction))));
                }
            }
        }
    }
}

impl AppletRuntime {
    fn configure(&mut self, config: AppletConfig) {
        if self.config.as_ref() == Some(&config) {
            return;
        }

        let outcome = self.applet.as_mut().map(|applet| {
            let ctx = &self.ctx;
            catch_unwind(AssertUnwindSafe(|| applet.configure(ctx, &config)))
        });
        if let Some(Err(panic)) = outcome {
            self.stop(panic.as_ref());
        }

        self.config = Some(config);
        tracing::debug!(applet = self.ctx.name(), "configured");
        self.deliver(None);
    }

    fn owns(&self) -> bool {
        self.shown
            .as_ref()
            .is_some_and(|shown| self.catcher.holds(&shown.widget()))
    }

    fn show_popover(&mut self, sender: &ComponentSender<Self>) {
        if self.owns() {
            return self.catcher.close();
        }

        let seat = self.seat.clone();
        let outcome = self
            .applet
            .as_mut()
            .map(|applet| catch_unwind(AssertUnwindSafe(|| applet.popover(&seat))));
        let shown = match outcome {
            Some(Ok(Some(shown))) => shown,
            Some(Err(panic)) => return self.stop(panic.as_ref()),
            _ => return,
        };

        let at = self.anchor().unwrap_or(0);
        self.catcher.open(&shown.widget(), at, {
            let sender = sender.clone();
            move || sender.input(HostInput::PopoverDismissed)
        });
        self.anchored = Some(at);
        self.shown = Some(shown);
        tracing::debug!(applet = self.ctx.name(), at, "popover opened");
    }

    fn follow_anchor(&mut self) {
        if !self.owns() {
            return;
        }
        let at = self.anchor();
        if at == self.anchored || at.is_none() {
            return;
        }
        self.anchored = at;
        if let Some(at) = at {
            self.catcher.place(at);
        }
    }

    fn anchor(&self) -> Option<i32> {
        let target = self.root.root()?;
        let widget = match self.applet.as_ref()?.anchor() {
            Some(widget) => widget,
            None => self.root.clone().upcast(),
        };
        let bounds = widget.compute_bounds(target.upcast_ref::<gtk4::Widget>())?;
        Some(match self.catcher.horizontal() {
            true => (bounds.x() + bounds.width() / 2.0).round() as i32,
            false => (bounds.y() + bounds.height() / 2.0).round() as i32,
        })
    }

    fn orient(&mut self, orientation: gtk4::Orientation) {
        self.root.set_orientation(orientation);
        if let Some(group) = self.group.as_ref() {
            group.set_orientation(orientation);
            return;
        }

        let outcome = self
            .applet
            .as_mut()
            .map(|applet| catch_unwind(AssertUnwindSafe(|| applet.orient(orientation))));
        if let Some(Err(panic)) = outcome {
            self.stop(panic.as_ref());
        }
    }

    fn stop(&mut self, panic: &(dyn Any + Send)) {
        tracing::error!(
            applet = self.ctx.name(),
            reason = panic_reason(panic),
            "applet panicked, stopping it"
        );
        self.applet = None;
        self.ctx.shutdown();

        if self.group.is_none()
            && let Some(view) = self.root.first_child()
        {
            self.root.remove(&view);
        }
    }

    fn deliver(&mut self, input: Option<&Input>) {
        match input {
            Some(Input::Pointer(pointer)) => {
                tracing::debug!(applet = self.ctx.name(), ?pointer, "pointer")
            }
            Some(Input::Tick | Input::Woken) | None => {}
        }

        let outcome = {
            let Some(applet) = self.applet.as_mut() else {
                return;
            };
            let ctx = &self.ctx;
            catch_unwind(AssertUnwindSafe(|| {
                if let Some(input) = input {
                    applet.handle(ctx, input);
                }
                applet.indicators()
            }))
        };

        let specs = match outcome {
            Ok(specs) => specs,
            Err(panic) => {
                self.stop(panic.as_ref());
                Vec::new()
            }
        };

        self.follow_anchor();

        let Some(group) = self.group.as_ref() else {
            return;
        };
        tracing::debug!(
            applet = self.ctx.name(),
            indicators = specs.len(),
            "rendered"
        );
        group.set_items(&specs);
    }
}

#[derive(Default)]
struct Scroll {
    horizontal: f64,
    vertical: f64,
}

impl Scroll {
    fn notches(&mut self, dx: f64, dy: f64) -> Vec<Direction> {
        let mut out = Vec::new();
        drain(
            &mut self.horizontal,
            dx,
            Direction::Left,
            Direction::Right,
            &mut out,
        );
        drain(
            &mut self.vertical,
            dy,
            Direction::Up,
            Direction::Down,
            &mut out,
        );
        out
    }
}

fn drain(
    accumulated: &mut f64,
    delta: f64,
    negative: Direction,
    positive: Direction,
    out: &mut Vec<Direction>,
) {
    *accumulated += delta;
    while *accumulated >= NOTCH {
        *accumulated -= NOTCH;
        out.push(positive);
    }
    while *accumulated <= -NOTCH {
        *accumulated += NOTCH;
        out.push(negative);
    }
}

fn panic_reason(panic: &(dyn Any + Send)) -> &str {
    if let Some(text) = panic.downcast_ref::<&str>() {
        return text;
    }
    if let Some(text) = panic.downcast_ref::<String>() {
        return text;
    }
    "panicked"
}

pub struct AppletHandle {
    pub widget: gtk4::Box,
    _controller: Controller<AppletRuntime>,
}

impl AppletHandle {
    pub fn launch(
        name: String,
        output: Option<String>,
        build: Builder,
        config: AppletConfig,
        catcher: Rc<Catcher>,
    ) -> Self {
        let controller = AppletRuntime::builder()
            .launch(AppletInit {
                name,
                output,
                build,
                config,
                catcher,
            })
            .detach();

        Self {
            widget: controller.widget().clone(),
            _controller: controller,
        }
    }

    pub fn configure(&self, config: AppletConfig) {
        let _ = self
            ._controller
            .sender()
            .send(HostInput::Configured(config));
    }

    pub fn set_orientation(&self, orientation: gtk4::Orientation) {
        let _ = self
            ._controller
            .sender()
            .send(HostInput::Oriented(orientation));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_widgets::IndicatorSpec;
    use std::cell::{Cell, RefCell};

    thread_local! {
        static SEEN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        static EXPLODE: Cell<bool> = const { Cell::new(false) };
        static CONFIGURED: Cell<u32> = const { Cell::new(0) };
        static SHOWN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    struct Probe;

    impl Applet for Probe {
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

    /// `weather-none-available-symbolic` is in the icon-naming spec and is NOT in Adwaita, so the
    /// chip for an unknown condition — and every trouble chip, which routes through the same
    /// mapping — rendered GTK's broken-image placeholder. Nothing caught it because a missing icon
    /// is not an error at any layer: the name is a string until something asks the theme for it.
    fn every_weather_icon_the_applet_can_ask_for_exists() {
        let theme = gtk4::IconTheme::for_display(&gtk4::gdk::Display::default().expect("display"));
        let mut names = vec![crate::applets::weather::render::ALERT_ICON];
        for condition in crate::applets::weather::render::EVERY {
            for is_day in [true, false] {
                names.push(crate::applets::weather::render::icon(condition, is_day));
            }
        }
        names.sort_unstable();
        names.dedup();

        for name in names {
            assert!(theme.has_icon(name), "the icon theme has no `{name}`");
        }
    }

    /// One function, because `gtk4::init()` binds GTK to the calling thread and cargo runs tests
    /// in parallel.
    #[test]
    #[ignore = "needs a display"]
    fn an_applet_reaches_its_group() {
        if gtk4::init().is_err() {
            return;
        }
        glimpse_widgets::register_resources().expect("resources");

        every_weather_icon_the_applet_can_ask_for_exists();

        shown(&["a", "b"]);
        let handle = AppletHandle::launch(
            "probe".to_owned(),
            Some("DP-1".to_owned()),
            Box::new(|_| Box::new(Probe)),
            config("%H:%M"),
            Catcher::new(None, glimpse_config::Position::Top),
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

        a_bluetooth_popover_signal_reaches_the_service();
    }

    /// The one link the widget's own tests and the service's own tests leave between them: the
    /// closures `Bluetooth::popover` connects. Both assertions use a confirmation, because that is
    /// the only command whose whole effect is local — no bus is needed to see it land.
    fn a_bluetooth_popover_signal_reaches_the_service() {
        use crate::applet::popover::Seat;
        use crate::applets::bluetooth::Bluetooth as BluetoothApplet;
        use glimpse_dbus::Buses;
        use glimpse_services::{Confirmation, DeviceId, Running};
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
            (),
        );

        let notifications =
            glimpse_dbus::notifications::NotificationsProvider::unavailable("no bus in tests")
                .handle();
        let mut applet = BluetoothApplet::start(bluetooth.clone(), notifications);
        let (host, _receiver) = relm4::channel();
        let shown = applet
            .popover(&Seat::new(host))
            .expect("the applet opens a popover");
        let popover = shown
            .widget()
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
}
