pub mod render;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use chrono::{DateTime, Local};
use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind, PowerAppletConfig};
use glimpse_dbus::idle::{IdleProviderError, IdleProviderHandle, IdleProviderState};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_dbus::upower::ChargeState;
use glimpse_services::{BatteryHandle, BatteryHistoryPoint, BatteryState};
use glimpse_widgets::{IndicatorSpec, PowerPopover, Severity};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::Seat;
use crate::applet::{Applet, Ctx, Input, Report, report_failure, tell, wording};
use crate::applets::idle::keep_awake;

pub struct Power {
    battery: BatteryHandle,
    idle: IdleProviderHandle,
    notifications: NotificationsProviderHandle,
    state: BatteryState,
    idle_state: IdleProviderState,
    settings: PowerAppletConfig,
    tooltip_format: Option<String>,
    twelve: bool,
    spec: Vec<IndicatorSpec>,
    icon: Option<(String, gio::Icon)>,
    hold_icon: gio::Icon,
    since_unplugged: Option<DateTime<Local>>,
    seen_plugged: bool,
    history: Rc<RefCell<Vec<BatteryHistoryPoint>>>,
    shown: glib::WeakRef<PowerPopover>,
}

impl Power {
    pub fn start(
        battery: BatteryHandle,
        idle: IdleProviderHandle,
        notifications: NotificationsProviderHandle,
    ) -> Self {
        let idle_state = idle.snapshot();
        let state = battery.snapshot();
        let mut seen_plugged = false;
        let _ = render::unplugged_at(
            state.display.as_ref().map(|charge| charge.state),
            &mut seen_plugged,
            None,
            Local::now(),
        );
        Self {
            state,
            idle_state,
            battery,
            idle,
            notifications,
            settings: PowerAppletConfig::default(),
            tooltip_format: None,
            twelve: false,
            spec: Vec::new(),
            icon: None,
            hold_icon: gio::ThemedIcon::new("dialog-warning-symbolic").upcast(),
            since_unplugged: None,
            seen_plugged,
            history: Rc::new(RefCell::new(Vec::new())),
            shown: glib::WeakRef::new(),
        }
    }

    fn refresh(&mut self) {
        self.state = self.battery.snapshot();
        self.idle_state = self.idle.snapshot();
        self.since_unplugged = render::unplugged_at(
            self.state.display.as_ref().map(|charge| charge.state),
            &mut self.seen_plugged,
            self.since_unplugged,
            Local::now(),
        );
        self.idle_state
            .inhibitors
            .retain(crate::applets::idle::render::holds_idle_or_sleep);
        self.spec = vec![self.indicator()];
        if let Some(notice) = render::hold_notice(&self.idle_state, self.ends().as_deref()) {
            self.spec.push(IndicatorSpec {
                icon: Some(self.hold_icon.clone()),
                tooltip: Some(format!("{} · {}", notice.title, notice.subtitle)),
                severity: Some(Severity::Warning),
                ..Default::default()
            });
        }
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn indicator(&mut self) -> IndicatorSpec {
        let mut spec = render::chip(
            &self.state,
            self.settings.indicator_style,
            &self.settings.label_format,
            self.tooltip_format.as_deref(),
        );
        if self.state.display.is_none() || render::shows_icon(self.settings.indicator_style) {
            let name = render::header_icon(&self.state);
            spec.icon = Some(self.themed(&name));
        }
        spec
    }

    fn themed(&mut self, name: &str) -> gio::Icon {
        match &self.icon {
            Some((held, icon)) if held == name => icon.clone(),
            _ => {
                let icon: gio::Icon = gio::ThemedIcon::new(name).upcast();
                self.icon = Some((name.to_owned(), icon.clone()));
                icon
            }
        }
    }

    fn ends(&self) -> Option<String> {
        keep_awake::deadline(&self.idle_state.inhibitors, &self.idle_state.hold_expiries)
            .map(|(at, _)| at.format(glimpse_config::clock(self.twelve)).to_string())
    }

    fn dress(&self, shown: &PowerPopover) {
        shown.set_header(
            &render::header_icon(&self.state),
            self.state.display.as_ref().map(|charge| charge.percentage),
        );
        shown.set_notice(render::hold_notice(&self.idle_state, self.ends().as_deref()).as_ref());
        let remaining = self.state.display.as_ref().and_then(render::remaining);
        shown.set_remaining(
            remaining
                .as_ref()
                .map(|(title, value)| (title.as_str(), value.as_str())),
        );
        let (choices, selected) = render::profiles(self.state.profile.as_ref());
        shown.set_profiles(&choices, selected);
        shown.set_devices(&render::devices(&self.state));
        let supply = self
            .state
            .internals
            .first()
            .filter(|_| self.state.display.is_some());
        let mut facts = supply
            .map(|supply| {
                render::battery_facts(
                    supply,
                    self.state.display.as_ref().unwrap_or(&supply.charge),
                )
            })
            .unwrap_or_default();
        if let Some(charge) = &self.state.display {
            let elapsed = self
                .since_unplugged
                .and_then(|at| u32::try_from((Local::now() - at).num_seconds().max(0)).ok());
            if let Some(fact) = render::since_unplugged(charge, elapsed) {
                facts.insert(0, fact);
            }
        }
        shown.set_battery(&facts);
        shown.set_charge_limit(supply.and_then(render::charge_limit).as_ref());
        if let Some(display) = &self.state.display {
            let now = render::history_minute(Local::now());
            let history = {
                let mut points = self.history.borrow().clone();
                points.push(BatteryHistoryPoint {
                    at: u32::try_from(now.timestamp()).unwrap_or(u32::MAX),
                    percent: f64::from(display.percentage),
                    charging: display.state == ChargeState::Charging,
                });
                render::history_view(&points, now, self.twelve)
            };
            shown.set_history(Some(&history));
        } else {
            shown.set_history(None);
        }
        shown.set_keep_awake_visible(render::keep_awake_available(&self.idle_state));
        shown.set_hold_active(!keep_awake::manual_holds(&self.idle_state.inhibitors).is_empty());
        shown.set_hold_preset(
            keep_awake::deadline(&self.idle_state.inhibitors, &self.idle_state.hold_expiries)
                .map(|(_, seconds)| seconds),
        );
    }
}

impl Applet for Power {
    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Power(settings) = &config.kind else {
            return;
        };
        self.settings = settings.clone();
        self.tooltip_format = config.common.tooltip_format.clone();
        self.twelve = config.regional.twelve_hour();

        ctx.interval(Duration::from_secs(60));
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        if matches!(input, Input::Woken | Input::Tick) {
            self.refresh();
        }
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<gtk4::Widget> {
        let shown = PowerPopover::new();
        self.history = Rc::new(RefCell::new(Vec::new()));

        shown.connect_hold_requested({
            let idle = self.idle.clone();
            let opener = seat.opener();
            let failed = hold_failure(&self.notifications);
            move |popover, seconds| {
                popover.set_hold_active(true);
                let popover = popover.downgrade();
                keep_awake::request(
                    &idle,
                    opener.clone(),
                    seconds,
                    move || {
                        if let Some(popover) = popover.upgrade() {
                            popover.collapse_hold();
                        }
                    },
                    Rc::clone(&failed),
                );
            }
        });

        shown.connect_hold_toggled({
            let idle = self.idle.clone();
            let opener = seat.opener();
            let failed = hold_failure(&self.notifications);
            move |_, on| keep_awake::toggle(&idle, opener.clone(), on, Rc::clone(&failed))
        });

        shown.connect_history_opened({
            let battery = self.battery.clone();
            let opener = seat.opener();
            let history = Rc::clone(&self.history);
            let generation = Rc::new(Cell::new(0_u64));
            move |popover| {
                let battery = battery.clone();
                let opener = opener.clone();
                let history = Rc::clone(&history);
                let popover = popover.downgrade();
                let generation = Rc::clone(&generation);
                let request = generation.get().wrapping_add(1);
                generation.set(request);
                relm4::spawn_local(async move {
                    let result = battery.history().await;
                    if generation.get() != request {
                        return;
                    }
                    match result {
                        Ok(points) if popover.upgrade().is_some() => {
                            *history.borrow_mut() = points;
                            opener.wake();
                        }
                        Ok(_) => {}
                        Err(error) => {
                            tracing::warn!(operation = "battery.history", %error, "service command failed");
                        }
                    }
                });
            }
        });

        let battery = self.battery.clone();
        let notifications = self.notifications.clone();
        let opener = seat.opener();
        shown.connect_profile_activated(move |_, index| {
            let Some(name) = battery
                .snapshot()
                .profile
                .as_ref()
                .and_then(|profile| profile.available.get(index as usize))
                .cloned()
            else {
                return;
            };
            let battery = battery.clone();
            tell(
                Report {
                    notifications: notifications.clone(),
                    app_name: gettext("Power"),
                    icon: "battery-symbolic".to_owned(),
                    summary: gettext("Could not change the power mode"),
                },
                opener.clone(),
                "battery.set_profile",
                async move { battery.set_profile(name).await },
                gettext("The battery service is unavailable."),
                wording,
            );
        });

        let battery = self.battery.clone();
        let notifications = self.notifications.clone();
        let opener = seat.opener();
        shown.connect_charge_limit_toggled(move |_, enabled| {
            let Some(path) = battery
                .snapshot()
                .internals
                .first()
                .map(|supply| supply.path.clone())
            else {
                return;
            };
            let battery = battery.clone();
            tell(
                Report {
                    notifications: notifications.clone(),
                    app_name: gettext("Power"),
                    icon: "battery-symbolic".to_owned(),
                    summary: gettext("Could not change the charge limit"),
                },
                opener.clone(),
                "battery.enable_charge_threshold",
                async move { battery.enable_charge_threshold(path, enabled).await },
                gettext("The battery service is unavailable."),
                wording,
            );
        });

        self.shown.set(Some(&shown));
        self.refresh();
        Some(shown.upcast())
    }
}

fn hold_failure(notifications: &NotificationsProviderHandle) -> keep_awake::Failure {
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Power"),
        icon: "battery-symbolic".to_owned(),
        summary: gettext("Could not change keep awake"),
    };
    Rc::new(move |operation, error| {
        let body = match &error {
            IdleProviderError::Unavailable(_) => gettext("The idle service is unavailable."),
            IdleProviderError::TimedOut => gettext("That took too long."),
            IdleProviderError::Call(_) | IdleProviderError::UnknownObject => {
                gettext("That did not work.")
            }
        };
        let report = report.clone();
        relm4::spawn_local(async move {
            report_failure(operation, report, Some(body), error).await;
        });
    })
}
