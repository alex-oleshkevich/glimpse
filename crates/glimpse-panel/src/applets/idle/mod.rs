pub mod keep_awake;
pub(crate) mod render;

use std::rc::Rc;
use std::time::Duration;

use chrono::{Local, Utc};
use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::idle::{IdleProviderHandle, IdleProviderState};
use glimpse_widgets::{IdlePopover, IndicatorSpec, Severity};
use gtk4::{gio, glib, prelude::*};

use crate::applet::popover::Seat;
use crate::applet::{Applet, Ctx, Input};
use keep_awake::{deadline, manual_holds, settling};

const MINUTE: Duration = Duration::from_secs(60);

pub struct Idle {
    idle: IdleProviderHandle,
    tooltip_format: Option<String>,
    state: IdleProviderState,
    twelve: bool,
    icon: Option<(&'static str, gio::Icon)>,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<IdlePopover>,
}

impl Applet for Idle {
    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Idle { .. } = &config.kind else {
            return;
        };
        self.tooltip_format = config.kind.tooltip_format().map(str::to_owned);
        self.twelve = config.regional.twelve_hour();

        ctx.interval(MINUTE);
        self.sync();
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => self.sync(),
            Input::Tick => {}
            _ => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<gtk4::Widget> {
        let shown = IdlePopover::new();

        shown.connect_hold_requested({
            let idle = self.idle.clone();
            let opener = seat.opener();
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
                    Rc::new(keep_awake::log_failure),
                );
            }
        });

        shown.connect_hold_toggled({
            let idle = self.idle.clone();
            let opener = seat.opener();
            move |_, on| {
                keep_awake::toggle(&idle, opener.clone(), on, Rc::new(keep_awake::log_failure));
            }
        });

        shown.connect_release_requested({
            let idle = self.idle.clone();
            let opener = seat.opener();
            move |_, id| {
                let idle = idle.clone();
                settling(
                    "idle.release",
                    opener.clone(),
                    async move { idle.release(id).await },
                    Rc::new(keep_awake::log_failure),
                );
            }
        });

        self.shown.set(Some(&shown));
        self.refresh();
        Some(shown.upcast())
    }
}

impl Idle {
    pub fn start(idle: IdleProviderHandle) -> Self {
        let state = idle.snapshot();
        Self {
            idle,
            tooltip_format: None,
            state,
            twelve: false,
            icon: None,
            spec: Vec::new(),
            shown: glib::WeakRef::new(),
        }
    }

    fn sync(&mut self) {
        self.state = self.idle.snapshot();
        self.state.inhibitors.retain(render::holds_idle_or_sleep);
    }

    fn left(&self) -> Option<String> {
        let seconds = (deadline(&self.state.inhibitors, &self.state.hold_expiries)?.0
            - Local::now())
        .num_seconds();
        (seconds > 0).then(|| render::time_left((seconds + 59) / 60))
    }

    fn ends(&self) -> Option<String> {
        deadline(&self.state.inhibitors, &self.state.hold_expiries)
            .map(|(at, _)| at.format(glimpse_config::clock(self.twelve)).to_string())
    }

    fn refresh(&mut self) {
        self.spec = self.indicator().into_iter().collect();

        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn indicator(&mut self) -> Option<IndicatorSpec> {
        if let Some(reason) = render::unusable(&self.state) {
            return Some(IndicatorSpec {
                icon: Some(self.themed(render::ICON_IDLE)),
                tooltip: Some(reason),
                severity: Some(Severity::Warning),
                ..Default::default()
            });
        }
        let active = !manual_holds(&self.state.inhibitors).is_empty();
        let icon = self.themed(render::icon(active));
        Some(IndicatorSpec {
            icon: Some(icon),
            tooltip: render::tooltip(
                &self.state,
                self.ends().as_deref(),
                self.tooltip_format.as_deref(),
            ),
            severity: active.then_some(Severity::Warning),
            ..Default::default()
        })
    }

    fn themed(&mut self, name: &'static str) -> gio::Icon {
        match &self.icon {
            Some((held, icon)) if *held == name => icon.clone(),
            _ => {
                let icon: gio::Icon = gio::ThemedIcon::new(name).upcast();
                self.icon = Some((name, icon.clone()));
                icon
            }
        }
    }

    fn dress(&self, shown: &IdlePopover) {
        let held = !manual_holds(&self.state.inhibitors).is_empty();
        let ends = self.ends();
        shown.set_heading(
            render::icon(held),
            &gettext("Idle"),
            Some(&render::hero_subtitle(
                &self.state,
                ends.as_deref(),
                self.left().as_deref(),
            )),
        );
        shown.set_hold_active(held);
        shown.set_hold_preset(
            deadline(&self.state.inhibitors, &self.state.hold_expiries).map(|(_, seconds)| seconds),
        );
        shown.set_hold_label(ends.map(|at| render::awake_until(&at)).as_deref());
        shown.set_inhibitors(&render::to_inhibitor_entries(
            &self.state.inhibitors,
            Utc::now(),
        ));
    }
}

#[cfg(test)]
mod tests;
