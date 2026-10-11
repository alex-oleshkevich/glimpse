use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};

use glimpse_config::{Applet as AppletConfig, Position};
use glimpse_widgets::{IndicatorGroup, IndicatorSpec, Severity};
use gtk4::{gio, glib};
use std::rc::Rc;

use crate::components::panel::Zone;

use super::catcher::Catcher;
use gtk4::prelude::*;

use super::popover::Seat;
use relm4::{Component, ComponentController, ComponentParts, ComponentSender, Controller};

use super::{Applet, Button, Ctx, Direction, Input, Pointer};

const NOTCH: f64 = 1.0;
const SCROLL_CAP: f64 = 16.0 * NOTCH;
const TEXT_CAP: usize = 64;
const TOOLTIP_CAP: usize = 256;

static SERIAL: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SlotInfo {
    pub connector: String,
    pub position: Position,
    pub zone: Zone,
    pub index: u32,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct IndicatorStatus {
    pub icon: Option<String>,
    pub overlay: Option<String>,
    pub dot: Option<String>,
    pub label: Option<String>,
    pub tooltip: Option<String>,
    pub badge: Option<String>,
    pub attention: bool,
    pub notice: bool,
    pub severity: Option<&'static str>,
    pub class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppletStatus {
    pub slot: SlotInfo,
    pub kind: Option<String>,
    pub alive: bool,
    pub own_view: bool,
    pub popover: bool,
    pub indicators: Vec<IndicatorStatus>,
}

impl IndicatorStatus {
    pub fn from_spec(spec: &IndicatorSpec) -> Self {
        let icon = |icon: &Option<gio::Icon>| {
            icon.as_ref()
                .and_then(gio::prelude::IconExt::to_string)
                .map(|name| glimpse_utils::clean(&name, TEXT_CAP))
        };
        let text = |text: &Option<String>, cap| {
            text.as_deref().map(|text| glimpse_utils::clean(text, cap))
        };
        Self {
            icon: icon(&spec.icon),
            overlay: icon(&spec.overlay),
            dot: spec.dot.map(|rgba| {
                let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
                format!(
                    "#{:02x}{:02x}{:02x}{:02x}",
                    byte(rgba.red()),
                    byte(rgba.green()),
                    byte(rgba.blue()),
                    byte(rgba.alpha())
                )
            }),
            label: text(&spec.label, TEXT_CAP),
            tooltip: text(&spec.tooltip, TOOLTIP_CAP),
            badge: text(&spec.badge, TEXT_CAP),
            attention: spec.attention,
            notice: spec.notice,
            severity: spec.severity.map(|severity| match severity {
                Severity::Info => "info",
                Severity::Warning => "warning",
                Severity::Error => "error",
            }),
            class: text(&spec.class, TEXT_CAP),
        }
    }

    fn to_variant(&self) -> glib::Variant {
        let dict = glib::VariantDict::new(None);
        for (key, value) in [
            ("icon", &self.icon),
            ("overlay", &self.overlay),
            ("dot", &self.dot),
            ("label", &self.label),
            ("tooltip", &self.tooltip),
            ("badge", &self.badge),
            ("class", &self.class),
        ] {
            if let Some(value) = value {
                dict.insert_value(key, &value.to_variant());
            }
        }
        dict.insert_value("attention", &self.attention.to_variant());
        dict.insert_value("notice", &self.notice.to_variant());
        if let Some(severity) = self.severity {
            dict.insert_value("severity", &severity.to_variant());
        }
        dict.end()
    }
}

impl AppletStatus {
    pub fn to_variant(&self) -> glib::Variant {
        let dict = glib::VariantDict::new(None);
        let position = match self.slot.position {
            Position::Top => "top",
            Position::Bottom => "bottom",
            Position::Left => "left",
            Position::Right => "right",
        };
        let zone = match self.slot.zone {
            Zone::Start => "start",
            Zone::Center => "center",
            Zone::End => "end",
        };
        dict.insert_value("connector", &self.slot.connector.to_variant());
        dict.insert_value("position", &position.to_variant());
        dict.insert_value("zone", &zone.to_variant());
        dict.insert_value("index", &self.slot.index.to_variant());
        dict.insert_value("name", &self.slot.name.to_variant());
        if let Some(kind) = &self.kind {
            dict.insert_value("kind", &kind.to_variant());
        }
        dict.insert_value("alive", &self.alive.to_variant());
        dict.insert_value("own-view", &self.own_view.to_variant());
        dict.insert_value("popover", &self.popover.to_variant());
        dict.insert_value(
            "indicators",
            &glib::Variant::array_from_iter_with_type(
                glib::VariantTy::VARDICT,
                self.indicators.iter().map(IndicatorStatus::to_variant),
            ),
        );
        dict.end()
    }
}

pub type Builder = Box<dyn FnOnce(&Ctx) -> Box<dyn Applet>>;

pub struct AppletInit {
    pub name: String,
    pub output: Option<String>,
    pub build: Builder,
    pub config: AppletConfig,
    pub catcher: Rc<Catcher>,
    pub slot: SlotInfo,
}

#[derive(Debug)]
pub enum HostInput {
    Configured(AppletConfig),
    Slotted(SlotInfo),
    PopoverRequested,
    PopoverToggled,
    PopoverLowered,
    PopoverDismissed,
    Oriented(gtk4::Orientation),
    Placed(glimpse_services::Placement),
    Pressed { button: u32 },
    Woken,
    Acknowledged,
    Typing(bool),
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
    shown: Option<gtk4::Widget>,
    anchored: Option<i32>,
    slot: SlotInfo,
    kind: Option<String>,
    indicators: Vec<IndicatorStatus>,
    action: gio::SimpleAction,
    published: Option<glib::Variant>,
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
        if let Some(child) = root.first_child() {
            child
                .bind_property("visible", &root, "visible")
                .sync_create()
                .build();
        }

        tracing::debug!(applet = ctx.name(), own_view = group.is_none(), "started");

        let action = gio::SimpleAction::new_stateful(
            &format!("applet.{}", SERIAL.fetch_add(1, Ordering::Relaxed)),
            Some(&<(String, glib::Variant)>::static_variant_type()),
            &glib::VariantDict::new(None).end(),
        );
        action.connect_change_state(|_, _| {});
        action.connect_activate({
            let sender = sender.clone();
            move |action, parameter| match command(parameter) {
                Some(input) => sender.input(input),
                None => {
                    tracing::warn!(action = %action.name(), ?parameter, "unknown applet command")
                }
            }
        });

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
            slot: init.slot,
            kind: None,
            indicators: Vec::new(),
            action,
            published: None,
        };
        model.configure(init.config);
        model.deliver(None);
        model.publish();
        relm4::main_application().add_action(&model.action);

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            HostInput::PopoverRequested => self.raise_popover(&sender),
            HostInput::PopoverToggled => self.toggle_popover(&sender),
            HostInput::PopoverLowered => self.lower_popover(),
            HostInput::PopoverDismissed => {
                if !self.owns() {
                    self.shown = None;
                    self.anchored = None;
                }
            }
            HostInput::Configured(settings) => self.configure(settings),
            HostInput::Oriented(orientation) => self.orient(orientation),
            HostInput::Placed(placement) => self.place(placement),
            HostInput::Ticked => self.deliver(Some(&Input::Tick)),
            HostInput::Woken => self.deliver(Some(&Input::Woken)),
            HostInput::Acknowledged => {
                if let Some(group) = &self.group {
                    group.acknowledge();
                }
            }
            HostInput::Typing(typing) => {
                if self.owns() {
                    self.catcher.set_typing(typing);
                }
            }
            HostInput::Pressed { button } => {
                let button = Button::from_code(button);
                self.deliver(Some(&Input::Pointer(Pointer::Press(button))));
                if button == Button::Left {
                    self.toggle_popover(&sender);
                }
            }
            HostInput::Scrolled { dx, dy } => {
                for direction in self.scroll.notches(dx, dy) {
                    self.deliver(Some(&Input::Pointer(Pointer::Scroll(direction))));
                }
            }
            HostInput::Slotted(slot) => self.slot = slot,
        }
        self.publish();
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        relm4::main_application().remove_action(&self.action.name());
    }
}

fn command(parameter: Option<&glib::Variant>) -> Option<HostInput> {
    let (verb, argument) = parameter?.get::<(String, glib::Variant)>()?;
    match verb.as_str() {
        "click" => argument
            .get::<u32>()
            .map(|button| HostInput::Pressed { button }),
        "scroll" => argument
            .get::<(f64, f64)>()
            .filter(|(dx, dy)| dx.is_finite() && dy.is_finite())
            .map(|(dx, dy)| HostInput::Scrolled {
                dx: dx.clamp(-SCROLL_CAP, SCROLL_CAP),
                dy: dy.clamp(-SCROLL_CAP, SCROLL_CAP),
            }),
        "open" => Some(HostInput::PopoverRequested),
        "close" => Some(HostInput::PopoverLowered),
        "toggle" => Some(HostInput::PopoverToggled),
        _ => None,
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

        self.kind = serde_json::to_value(&config.kind)
            .ok()
            .and_then(|value| value["extends"].as_str().map(String::from));
        self.config = Some(config);
        tracing::debug!(applet = self.ctx.name(), "configured");
        self.deliver(None);
    }

    fn owns(&self) -> bool {
        self.shown
            .as_ref()
            .is_some_and(|shown| self.catcher.holds(shown))
    }

    fn up(&self) -> bool {
        self.owns() && !self.catcher.closing()
    }

    fn toggle_popover(&mut self, sender: &ComponentSender<Self>) {
        if self.up() {
            return self.catcher.close();
        }
        self.show_popover(sender);
    }

    fn lower_popover(&mut self) {
        if self.up() {
            self.catcher.close();
        }
    }

    fn raise_popover(&mut self, sender: &ComponentSender<Self>) {
        if self.up() {
            return;
        }
        self.show_popover(sender);
    }

    fn show_popover(&mut self, sender: &ComponentSender<Self>) {
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
        self.catcher.open(&shown, at, {
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

    fn publish(&mut self) {
        let status = AppletStatus {
            slot: self.slot.clone(),
            kind: self.kind.clone(),
            alive: self.applet.is_some(),
            own_view: self.group.is_none(),
            popover: self.up(),
            indicators: self.indicators.clone(),
        }
        .to_variant();
        if self.published.as_ref() == Some(&status) {
            return;
        }
        self.action.set_state(&status);
        self.published = Some(status);
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

    fn place(&mut self, placement: glimpse_services::Placement) {
        let outcome = self
            .applet
            .as_mut()
            .map(|applet| catch_unwind(AssertUnwindSafe(|| applet.place(placement))));
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
        self.indicators.clear();
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

        self.indicators = specs.iter().map(IndicatorStatus::from_spec).collect();
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
        slot: SlotInfo,
    ) -> Self {
        let controller = AppletRuntime::builder()
            .launch(AppletInit {
                name,
                output,
                build,
                config,
                catcher,
                slot,
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

    pub fn set_slot(&self, slot: SlotInfo) {
        let _ = self._controller.sender().send(HostInput::Slotted(slot));
    }

    pub fn set_placement(&self, placement: glimpse_services::Placement) {
        let _ = self._controller.sender().send(HostInput::Placed(placement));
    }
}

#[cfg(test)]
mod tests;
