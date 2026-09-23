use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind, BrightnessAppletConfig};
use glimpse_dbus::night_light::{
    NightLightProviderError, NightLightProviderHandle, NightLightProviderState,
};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{
    BrightnessHandle, BrightnessState, CommandError, CompositorHandle, OutputInfo,
};
use glimpse_widgets::{BrightnessPopover, IndicatorSpec, NightLight, Source as WidgetSource};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::{PopoverHandle, Seat};
use crate::applet::{Applet, Ctx, Direction, Input, Pointer, Report, report_failure};

use super::render::{self, Coalescer};

const NIGHT_LIGHT_ICON: &str = "night-light-symbolic";

type LevelCoalescers = Rc<RefCell<HashMap<String, Rc<RefCell<Coalescer<u32>>>>>>;
type TemperatureCoalescer = Rc<RefCell<Coalescer<u32>>>;

fn themed(name: &str) -> gio::Icon {
    gio::ThemedIcon::new(name).upcast()
}

pub struct Brightness {
    state: BrightnessState,
    night_light_state: NightLightProviderState,
    night_light_seen: bool,
    outputs: Vec<OutputInfo>,
    brightness: BrightnessHandle,
    night_light: NightLightProviderHandle,
    compositor: CompositorHandle,
    notifications: NotificationsProviderHandle,
    settings: BrightnessAppletConfig,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<BrightnessPopover>,
    pinned: RefCell<Option<String>>,
    level_coalescers: LevelCoalescers,
    temperature_coalescer: TemperatureCoalescer,
}

impl Applet for Brightness {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Brightness(settings) = &config.kind else {
            return;
        };
        self.settings = *settings;
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()));
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => {
                self.state = self.brightness.snapshot();
                self.night_light_state = self.night_light.snapshot();
                self.night_light_seen |= self.night_light_state.current.is_some();
                self.outputs = self.outputs();
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

        let shown = BrightnessPopover::new();

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
                        .map(|snapshot| snapshot.configured)
                        .filter(|configured| configured != "off")
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

        if let Some((_, command)) = &self.footer {
            let command = command.clone();
            shown.connect_footer_activated(move |_| crate::applet::popover::run(&command));
        }

        let _ = seat;
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

impl Brightness {
    fn powered_sources(&self) -> Vec<glimpse_services::BrightnessSource> {
        self.state
            .sources
            .iter()
            .filter(|source| render::is_powered(source, &self.outputs))
            .cloned()
            .collect()
    }

    fn outputs(&self) -> Vec<OutputInfo> {
        self.compositor
            .snapshot()
            .outputs
            .map(|outputs| outputs.outputs)
            .unwrap_or_default()
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
        let outputs = compositor
            .snapshot()
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
            settings: BrightnessAppletConfig::default(),
            tooltip_format: None,
            footer: None,
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

    fn dress(&self, shown: &BrightnessPopover) {
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
            });
        shown.set_night_light(night_light.as_ref());

        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
    }

    fn indicator(&self) -> Option<IndicatorSpec> {
        let has_sources = !self.state.sources.is_empty();
        let icon = render::chip(has_sources, self.night_light_seen)?;

        let focused = self.focused_connector();
        let sources = self.powered_sources();
        let current = render::current_display(&sources, focused);
        let tooltip = current.map(|source| {
            let name = render::source_name(source, &self.outputs);
            let percent = render::percent_of(source.current, source.max);
            render::tooltip(Some(&name), percent, self.tooltip_format.as_deref())
        });

        Some(IndicatorSpec {
            icon: Some(themed(icon)),
            tooltip,
            ..Default::default()
        })
    }
}
