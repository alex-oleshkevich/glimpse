use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind, DisplayAppletConfig};
use glimpse_dbus::night_light::{
    NightLightProviderError, NightLightProviderHandle, NightLightProviderState,
};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{
    BrightnessHandle, BrightnessState, Capability, CommandError, CompositorHandle, OutputInfo,
    OutputLogical, OutputMode,
};
use glimpse_widgets::{
    Display as WidgetDisplay, DisplayLogical as WidgetDisplayLogical,
    DisplayMode as WidgetDisplayMode, DisplayPopover, IndicatorSpec, NightLight,
    Source as WidgetSource,
};
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::Seat;
use crate::applet::themed;
use crate::applet::{
    Applet, Ctx, Direction, Input, Opener, Pointer, Report, report_failure, tell, wording,
};

use super::render::{self, Coalescer};

const NIGHT_LIGHT_ICON: &str = "night-light-symbolic";

type LevelCoalescers = Rc<RefCell<HashMap<String, Rc<RefCell<Coalescer<u32>>>>>>;
type TemperatureCoalescer = Rc<RefCell<Coalescer<u32>>>;

pub struct Display {
    state: BrightnessState,
    night_light_state: NightLightProviderState,
    night_light_seen: bool,
    outputs: Vec<OutputInfo>,
    brightness: BrightnessHandle,
    night_light: NightLightProviderHandle,
    compositor: CompositorHandle,
    notifications: NotificationsProviderHandle,
    settings: DisplayAppletConfig,
    tooltip_format: Option<String>,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<DisplayPopover>,
    pinned: RefCell<Option<String>>,
    level_coalescers: LevelCoalescers,
    temperature_coalescer: TemperatureCoalescer,
}

impl Applet for Display {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Display(settings) = &config.kind else {
            return;
        };
        self.settings = settings.clone();
        self.tooltip_format = config.kind.tooltip_format().map(str::to_owned);

        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => {
                self.state = self.brightness.snapshot();
                self.night_light_state = self.night_light.snapshot();
                self.night_light_seen |= self.night_light_state.current.is_some();
                let state = self.compositor.snapshot();
                self.outputs = state
                    .outputs
                    .map(|outputs| outputs.outputs)
                    .unwrap_or_default();
            }
            Input::Pointer(Pointer::Scroll(direction)) => {
                self.nudge(*direction);
                return;
            }
            Input::Tick | Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<gtk4::Widget> {
        let focused = self.focused_connector();
        let pinned = render::current_display(&self.powered_sources(), focused)
            .map(|source| source.id.clone());
        self.pinned.replace(pinned);

        let shown = DisplayPopover::new();
        let resync = Resync {
            shown: shown.downgrade(),
            opener: seat.opener(),
        };

        shown.connect_changed({
            let brightness = self.brightness.clone();
            let notifications = self.notifications.clone();
            let coalescers = Rc::clone(&self.level_coalescers);
            let resync = resync.clone();
            move |_, key, value| {
                let value = value.round().max(0.0) as u32;
                dispatch_level(
                    &coalescers,
                    brightness.clone(),
                    notifications.clone(),
                    resync.clone(),
                    key.to_owned(),
                    value,
                );
            }
        });

        shown.connect_night_light_toggled({
            let night_light = self.night_light.clone();
            let notifications = self.notifications.clone();
            let resync = resync.clone();
            move |_, on| {
                let night_light = night_light.clone();
                let notifications = notifications.clone();
                let resync = resync.clone();
                let schedule = match on {
                    true => night_light
                        .snapshot()
                        .current
                        .map(|snapshot| {
                            render::night_mode(&snapshot.schedule, &snapshot.configured).to_owned()
                        })
                        .unwrap_or_else(|| "automatic".to_owned()),
                    false => "off".to_owned(),
                };
                relm4::spawn_local(async move {
                    if let Err(error) = night_light.set_schedule(&schedule).await {
                        resync.run();
                        report_night_light_failure(
                            &notifications,
                            "night_light.set_schedule",
                            error,
                        )
                        .await;
                    }
                });
            }
        });

        shown.connect_night_light_scheduled({
            let night_light = self.night_light.clone();
            let notifications = self.notifications.clone();
            let resync = resync.clone();
            move |_, schedule| {
                let night_light = night_light.clone();
                let notifications = notifications.clone();
                let resync = resync.clone();
                let schedule = schedule.to_owned();
                relm4::spawn_local(async move {
                    if let Err(error) = night_light.set_schedule(&schedule).await {
                        resync.run();
                        report_night_light_failure(
                            &notifications,
                            "night_light.set_schedule",
                            error,
                        )
                        .await;
                    }
                });
            }
        });

        shown.connect_night_light_paused({
            let night_light = self.night_light.clone();
            let notifications = self.notifications.clone();
            let resync = resync.clone();
            move |_, paused| {
                let night_light = night_light.clone();
                let notifications = notifications.clone();
                let resync = resync.clone();
                relm4::spawn_local(async move {
                    let (operation, result) = if paused {
                        ("night_light.pause", night_light.pause().await)
                    } else {
                        ("night_light.resume", night_light.resume().await)
                    };
                    if let Err(error) = result {
                        resync.run();
                        report_night_light_failure(&notifications, operation, error).await;
                    }
                });
            }
        });

        shown.connect_night_light_changed({
            let night_light = self.night_light.clone();
            let notifications = self.notifications.clone();
            let coalescer = Rc::clone(&self.temperature_coalescer);
            let resync = resync.clone();
            move |_, value| {
                let value = value.round().max(0.0) as u32;
                dispatch_temperature(
                    &coalescer,
                    night_light.clone(),
                    notifications.clone(),
                    resync.clone(),
                    value,
                );
            }
        });

        shown.connect_night_light_moved({
            let night_light = self.night_light.clone();
            let notifications = self.notifications.clone();
            let coalescer = Rc::clone(&self.temperature_coalescer);
            let resync = resync.clone();
            move |_, value| {
                let value = value.round().max(0.0) as u32;
                dispatch_temperature(
                    &coalescer,
                    night_light.clone(),
                    notifications.clone(),
                    resync.clone(),
                    value,
                );
            }
        });

        shown.connect_enable_requested({
            let compositor = self.compositor.clone();
            let notifications = self.notifications.clone();
            let opener = seat.opener();
            move |_, connector, enabled| {
                let compositor = compositor.clone();
                let connector = connector.to_owned();
                tell(
                    Report {
                        notifications: notifications.clone(),
                        app_name: gettext("Displays"),
                        icon: render::ICON.to_owned(),
                        summary: gettext("Could not change that display"),
                    },
                    opener.clone(),
                    "compositor.set_output_enabled",
                    async move { compositor.set_output_enabled(connector, enabled).await },
                    gettext("The compositor is unavailable."),
                    wording,
                );
            }
        });

        shown.connect_blanked({
            let compositor = self.compositor.clone();
            let notifications = self.notifications.clone();
            let opener = seat.opener();
            move |_| {
                let compositor = compositor.clone();
                tell(
                    Report {
                        notifications: notifications.clone(),
                        app_name: gettext("Displays"),
                        icon: render::ICON.to_owned(),
                        summary: gettext("Could not blank the screens"),
                    },
                    opener.clone(),
                    "compositor.power_off_monitors",
                    async move { compositor.power_off_monitors().await },
                    gettext("The compositor is unavailable."),
                    wording,
                );
            }
        });

        self.shown.set(Some(&shown));
        self.refresh();
        Some(shown.upcast())
    }
}

#[derive(Clone)]
struct Resync {
    shown: glib::WeakRef<DisplayPopover>,
    opener: Opener,
}

impl Resync {
    fn run(&self) {
        if let Some(shown) = self.shown.upgrade() {
            shown.forget();
        }
        self.opener.wake();
    }
}

fn dispatch_level(
    coalescers: &LevelCoalescers,
    brightness: BrightnessHandle,
    notifications: NotificationsProviderHandle,
    resync: Resync,
    key: String,
    value: u32,
) {
    let coalescer = Rc::clone(
        coalescers
            .borrow_mut()
            .entry(key.clone())
            .or_insert_with(|| Rc::new(RefCell::new(Coalescer::new()))),
    );
    let Some(mut value) = coalescer.borrow_mut().request(value) else {
        return;
    };
    relm4::spawn_local(async move {
        loop {
            if let Err(error) = brightness.set_level(key.clone(), value).await {
                resync.run();
                report_brightness_failure(&notifications, "brightness.set_level", error).await;
            }
            match coalescer.borrow_mut().completed() {
                Some(pending) => value = pending,
                None => break,
            }
        }
    });
}

fn dispatch_temperature(
    coalescer: &TemperatureCoalescer,
    night_light: NightLightProviderHandle,
    notifications: NotificationsProviderHandle,
    resync: Resync,
    value: u32,
) {
    let coalescer = Rc::clone(coalescer);
    let Some(mut value) = coalescer.borrow_mut().request(value) else {
        return;
    };
    relm4::spawn_local(async move {
        loop {
            if let Err(error) = night_light.set_temperature(value).await {
                resync.run();
                report_night_light_failure(&notifications, "night_light.set_temperature", error)
                    .await;
            }
            match coalescer.borrow_mut().completed() {
                Some(pending) => value = pending,
                None => break,
            }
        }
    });
}

async fn report_brightness_failure(
    notifications: &NotificationsProviderHandle,
    operation: &'static str,
    error: CommandError,
) {
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Displays"),
        icon: render::CHIP_ICON.to_owned(),
        summary: gettext("Could not change the brightness"),
    };
    report_failure(operation, report, brightness_wording(&error), error).await;
}

async fn report_night_light_failure(
    notifications: &NotificationsProviderHandle,
    operation: &'static str,
    error: NightLightProviderError,
) {
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Night light"),
        icon: NIGHT_LIGHT_ICON.to_owned(),
        summary: gettext("Could not change that setting"),
    };
    report_failure(operation, report, night_light_wording(&error), error).await;
}

fn brightness_wording(error: &CommandError) -> Option<String> {
    Some(match error {
        CommandError::InvalidArgument(_) => gettext("That was not a valid value."),
        CommandError::Unavailable(_) => gettext("The brightness service is unavailable."),
        CommandError::Unsupported(_) => gettext("That is not supported."),
        CommandError::LimitExceeded(_) => gettext("That could not be completed."),
        CommandError::Internal(_) => gettext("That did not work."),
    })
}

fn night_light_wording(error: &NightLightProviderError) -> Option<String> {
    Some(match error {
        NightLightProviderError::InvalidSchedule(_) => gettext("That was not a valid setting."),
        NightLightProviderError::InvalidTemperature(_) => gettext("That was not a valid value."),
        NightLightProviderError::NotActive(_) => gettext("The night light is off."),
        NightLightProviderError::Unavailable(_) => gettext("The night light is not running."),
        NightLightProviderError::TimedOut => gettext("That took too long."),
        NightLightProviderError::Call(_) => gettext("That did not work."),
    })
}

fn to_mode(mode: &OutputMode) -> WidgetDisplayMode {
    WidgetDisplayMode {
        width: mode.width as i32,
        height: mode.height as i32,
        refresh_mhz: mode.refresh_mhz as i32,
    }
}

fn to_logical(logical: &OutputLogical) -> WidgetDisplayLogical {
    WidgetDisplayLogical {
        scale: logical.scale,
    }
}

fn to_widget_display(output: &OutputInfo) -> WidgetDisplay {
    WidgetDisplay {
        connector: output.connector.clone(),
        label: output.label.clone().unwrap_or_default(),
        make: output.make.clone(),
        model: output.model.clone(),
        current_mode: output.current_mode.as_ref().map(to_mode),
        logical: output.logical.as_ref().map(to_logical),
        enabled: output.enabled,
        built_in: output.built_in,
    }
}

impl Display {
    fn powered_sources(&self) -> Vec<glimpse_services::BrightnessSource> {
        self.state
            .sources
            .iter()
            .filter(|source| render::is_powered(source, &self.outputs))
            .cloned()
            .collect()
    }

    fn focused_connector(&self) -> Option<&str> {
        self.outputs
            .iter()
            .find(|output| output.focused)
            .map(|output| output.connector.as_str())
    }

    fn nudge(&self, direction: Direction) {
        let focused = self.focused_connector();
        let sources = self.powered_sources();
        let Some(current) = render::current_display(&sources, focused) else {
            return;
        };
        let magnitude = render::native_step(self.settings.scroll_step, current.max);
        if magnitude == 0 {
            return;
        }
        let delta = match direction {
            Direction::Up | Direction::Right => magnitude as i32,
            Direction::Down | Direction::Left => -(magnitude as i32),
        };
        let id = current.id.clone();
        let brightness = self.brightness.clone();
        let notifications = self.notifications.clone();
        relm4::spawn_local(async move {
            if let Err(error) = brightness.adjust_level(id, delta).await {
                report_brightness_failure(&notifications, "brightness.adjust_level", error).await;
            }
        });
    }

    pub fn start(
        brightness: BrightnessHandle,
        night_light: NightLightProviderHandle,
        compositor: CompositorHandle,
        notifications: NotificationsProviderHandle,
    ) -> Self {
        let state = brightness.snapshot();
        let night_light_state = night_light.snapshot();
        let compositor_state = compositor.snapshot();
        let outputs = compositor_state
            .outputs
            .map(|value| value.outputs)
            .unwrap_or_default();
        let night_light_seen = night_light_state.current.is_some();
        Self {
            state,
            night_light_state,
            night_light_seen,
            outputs,
            brightness,
            night_light,
            compositor,
            notifications,
            settings: DisplayAppletConfig::default(),
            tooltip_format: None,
            spec: Vec::new(),
            shown: glib::WeakRef::new(),
            pinned: RefCell::new(None),
            level_coalescers: Rc::new(RefCell::new(HashMap::new())),
            temperature_coalescer: Rc::new(RefCell::new(Coalescer::new())),
        }
    }

    fn refresh(&mut self) {
        self.spec = self.indicator().into_iter().collect();
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn dress(&self, shown: &DisplayPopover) {
        let focused = self.focused_connector();
        let pinned = self.pinned.borrow();
        let sources = self.powered_sources();
        let ordered = render::ordered_sources(&sources, focused, pinned.as_deref());
        let widget_sources: Vec<WidgetSource> = ordered
            .iter()
            .map(|source| WidgetSource {
                key: source.id.clone(),
                name: render::source_name(source, &self.outputs),
                icon: render::source_icon(source.kind).to_owned(),
                value: source.current as f64,
                maximum: source.max as f64,
                floor: source.floor as f64,
            })
            .collect();
        shown.set_sources(&widget_sources);

        let night_light = self
            .night_light_state
            .current
            .as_ref()
            .map(|snapshot| NightLight {
                enabled: render::switch_on(&snapshot.schedule),
                temperature: snapshot.temperature,
                schedule: render::night_mode(&snapshot.schedule, &snapshot.configured).to_owned(),
                fixed_hours: snapshot.schedule == "schedule" || snapshot.configured == "schedule",
                serving: snapshot.serving,
                can_pause: snapshot.pausable && snapshot.serving,
                paused_until: (snapshot.paused_until != 0).then(|| {
                    chrono::DateTime::from_timestamp(snapshot.paused_until, 0)
                        .map(|time| {
                            time.with_timezone(&chrono::Local)
                                .format("%H:%M")
                                .to_string()
                        })
                        .unwrap_or_default()
                }),
            });
        shown.set_night_light(night_light.as_ref());

        shown.set_output_power(self.compositor.supports(Capability::OutputPower));
        shown.set_blank_screens(self.compositor.supports(Capability::BlankScreens));
        let displays: Vec<WidgetDisplay> = self.outputs.iter().map(to_widget_display).collect();
        shown.set_displays(&displays);
    }

    fn indicator(&self) -> Option<IndicatorSpec> {
        let has_sources = !self.state.sources.is_empty();
        let (icon, fallback_tooltip) =
            render::chip(has_sources, self.night_light_seen, &self.outputs)?;

        let tooltip = if icon == render::CHIP_ICON {
            let focused = self.focused_connector();
            let sources = self.powered_sources();
            let current = render::current_display(&sources, focused);
            current
                .map(|source| {
                    let name = render::source_name(source, &self.outputs);
                    let percent = render::percent_of(source.current, source.max);
                    render::brightness_tooltip(Some(&name), percent, self.tooltip_format.as_deref())
                })
                .or_else(|| render::tooltip(&self.outputs))
        } else {
            fallback_tooltip
        };

        Some(IndicatorSpec {
            icon: Some(themed(icon)),
            tooltip,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests;
