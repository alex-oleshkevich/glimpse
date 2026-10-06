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
    BrightnessHandle, BrightnessState, CommandError, CompositorCapabilities, CompositorHandle,
    OutputInfo, OutputLogical, OutputMode,
};
use glimpse_widgets::{
    Display as WidgetDisplay, DisplayLogical as WidgetDisplayLogical,
    DisplayMode as WidgetDisplayMode, DisplayPopover, IndicatorSpec, NightLight,
    Source as WidgetSource,
};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::footer::Footer;
use crate::applet::popover::{PopoverHandle, Seat};
use crate::applet::{
    Applet, Ctx, Direction, Input, Opener, Pointer, Report, report_failure, wording,
};

use super::render::{self, Coalescer};

const NIGHT_LIGHT_ICON: &str = "night-light-symbolic";

type LevelCoalescers = Rc<RefCell<HashMap<String, Rc<RefCell<Coalescer<u32>>>>>>;
type TemperatureCoalescer = Rc<RefCell<Coalescer<u32>>>;

fn themed(name: &str) -> gio::Icon {
    gio::ThemedIcon::new(name).upcast()
}

pub struct Display {
    state: BrightnessState,
    night_light_state: NightLightProviderState,
    night_light_seen: bool,
    outputs: Vec<OutputInfo>,
    capabilities: Option<CompositorCapabilities>,
    brightness: BrightnessHandle,
    night_light: NightLightProviderHandle,
    compositor: CompositorHandle,
    notifications: NotificationsProviderHandle,
    settings: DisplayAppletConfig,
    tooltip_format: Option<String>,
    footer: Footer,
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
        self.settings = *settings;
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer.configure(&config.common);
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
                self.capabilities = state.status.map(|status| status.capabilities);
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

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let focused = self.focused_connector();
        let pinned = render::current_display(&self.powered_sources(), focused)
            .map(|source| source.id.clone());
        self.pinned.replace(pinned);

        let shown = DisplayPopover::new();

        shown.connect_changed({
            let brightness = self.brightness.clone();
            let notifications = self.notifications.clone();
            let coalescers = Rc::clone(&self.level_coalescers);
            move |_, key, value| {
                let value = value.round().max(0.0) as u32;
                dispatch_level(
                    &coalescers,
                    brightness.clone(),
                    notifications.clone(),
                    key.to_owned(),
                    value,
                );
            }
        });

        shown.connect_night_light_toggled({
            let night_light = self.night_light.clone();
            let notifications = self.notifications.clone();
            move |_, on| {
                let night_light = night_light.clone();
                let notifications = notifications.clone();
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
            move |_, schedule| {
                let night_light = night_light.clone();
                let notifications = notifications.clone();
                let schedule = schedule.to_owned();
                relm4::spawn_local(async move {
                    if let Err(error) = night_light.set_schedule(&schedule).await {
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

        shown.connect_night_light_changed({
            let night_light = self.night_light.clone();
            let notifications = self.notifications.clone();
            let coalescer = Rc::clone(&self.temperature_coalescer);
            move |_, value| {
                let value = value.round().max(0.0) as u32;
                dispatch_temperature(
                    &coalescer,
                    night_light.clone(),
                    notifications.clone(),
                    value,
                );
            }
        });

        shown.connect_night_light_moved({
            let night_light = self.night_light.clone();
            let notifications = self.notifications.clone();
            let coalescer = Rc::clone(&self.temperature_coalescer);
            move |_, value| {
                let value = value.round().max(0.0) as u32;
                dispatch_temperature(
                    &coalescer,
                    night_light.clone(),
                    notifications.clone(),
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
                    &notifications,
                    opener.clone(),
                    "compositor.set_output_enabled",
                    gettext("Could not change that display"),
                    async move { compositor.set_output_enabled(connector, enabled).await },
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
                    &notifications,
                    opener.clone(),
                    "compositor.power_off_monitors",
                    gettext("Could not blank the screens"),
                    async move { compositor.power_off_monitors().await },
                );
            }
        });

        let footer = self.footer.clone();
        shown.connect_footer_activated(move |_| footer.run());

        self.shown.set(Some(&shown));
        self.refresh();
        Some(Box::new(shown))
    }
}

fn dispatch_level(
    coalescers: &LevelCoalescers,
    brightness: BrightnessHandle,
    notifications: NotificationsProviderHandle,
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
    value: u32,
) {
    let coalescer = Rc::clone(coalescer);
    let Some(mut value) = coalescer.borrow_mut().request(value) else {
        return;
    };
    relm4::spawn_local(async move {
        loop {
            if let Err(error) = night_light.set_temperature(value).await {
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
        NightLightProviderError::Unavailable(_) => gettext("The night light is not running."),
        NightLightProviderError::TimedOut => gettext("That took too long."),
        NightLightProviderError::Call(_) => gettext("That did not work."),
    })
}

fn tell<F, T>(
    notifications: &NotificationsProviderHandle,
    opener: Opener,
    operation: &'static str,
    summary: String,
    future: F,
) where
    F: std::future::Future<Output = Result<T, CommandError>> + 'static,
    T: 'static,
{
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Displays"),
        icon: render::ICON.to_owned(),
        summary,
    };
    let unavailable = gettext("The compositor is unavailable.");
    relm4::spawn_local(async move {
        let Err(error) = future.await else {
            return;
        };
        opener.wake();
        report_failure(operation, report, wording(&error, &unavailable), error).await;
    });
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
        x: logical.x,
        y: logical.y,
        scale: logical.scale,
    }
}

fn to_widget_display(output: &OutputInfo) -> WidgetDisplay {
    WidgetDisplay {
        connector: output.connector.clone(),
        label: output.label.clone().unwrap_or_default(),
        make: output.make.clone(),
        model: output.model.clone(),
        serial: output.serial.clone(),
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
        let capabilities = compositor_state.status.map(|status| status.capabilities);
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
            capabilities,
            brightness,
            night_light,
            compositor,
            notifications,
            settings: DisplayAppletConfig::default(),
            tooltip_format: None,
            footer: Footer::default(),
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
        let ordered = render::ordered_sources(
            &sources,
            focused,
            self.settings.show_keyboard,
            pinned.as_deref(),
        );
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
            });
        shown.set_night_light(night_light.as_ref());

        let output_power = self.capabilities.is_some_and(|caps| caps.output_power);
        shown.set_output_power(output_power);
        let displays: Vec<WidgetDisplay> = self.outputs.iter().map(to_widget_display).collect();
        shown.set_displays(&displays);

        shown.set_footer(self.footer.label().as_deref());
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
