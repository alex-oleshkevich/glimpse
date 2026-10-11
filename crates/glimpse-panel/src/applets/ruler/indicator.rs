use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{CommandError, RulerHandle, RulerState};
use glimpse_widgets::{IndicatorSpec, RulerPopover};
use gtk4::glib;
use gtk4::prelude::*;

use super::render;
use crate::applet::footer::Footer;
use crate::applet::popover::Seat;
use crate::applet::{Applet, Button, Ctx, Input, Pointer, Report, spawn_reported};

pub struct Ruler {
    service: RulerHandle,
    notifications: NotificationsProviderHandle,
    state: RulerState,
    tooltip_format: Option<String>,
    footer: Footer,
    shown: glib::WeakRef<RulerPopover>,
    spec: Vec<IndicatorSpec>,
    icon: gtk4::gio::Icon,
}

impl Ruler {
    pub fn start(service: RulerHandle, notifications: NotificationsProviderHandle) -> Self {
        let state = service.snapshot();
        let mut applet = Self {
            state,
            service,
            notifications,
            tooltip_format: None,
            footer: Footer::default(),
            shown: glib::WeakRef::new(),
            spec: Vec::new(),
            icon: gtk4::gio::ThemedIcon::new(render::ICON).upcast(),
        };
        applet.refresh();
        applet
    }

    fn refresh(&mut self) {
        self.spec = vec![IndicatorSpec {
            icon: Some(self.icon.clone()),
            tooltip: Some(render::tooltip(
                self.state.history.first(),
                self.tooltip_format.as_deref(),
            )),
            ..Default::default()
        }];
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn dress(&self, shown: &RulerPopover) {
        shown.set_history(&render::history(&self.state.history));
        shown.set_footer(self.footer.label().as_deref());
    }

    fn report(&self, summary: String) -> Report {
        Report {
            notifications: self.notifications.clone(),
            app_name: gettext("Ruler"),
            icon: render::ICON.to_owned(),
            summary,
        }
    }
}

fn measure(service: &RulerHandle, report: Report) {
    let service = service.clone();
    spawn_reported("ruler.measure", report, wording, async move {
        service.measure().await
    });
}

fn wording(error: &CommandError) -> Option<String> {
    match error {
        CommandError::LimitExceeded(_) => None,
        CommandError::Unavailable(_) => Some(gettext(
            "The ruler could not start, capture the screen or reach the clipboard.",
        )),
        CommandError::InvalidArgument(_)
        | CommandError::Unsupported(_)
        | CommandError::Internal(_) => Some(gettext("That did not work.")),
    }
}

fn copy(service: &RulerHandle, report: Report, id: u64) {
    let Ok(id) = u32::try_from(id) else {
        return;
    };
    let service = service.clone();
    spawn_reported("ruler.copy", report, wording, async move {
        service.copy(id).await
    });
}

impl Applet for Ruler {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Ruler {} = &config.kind else {
            return;
        };
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer.configure(&config.common);
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => {
                self.state = self.service.snapshot();
            }
            Input::Pointer(Pointer::Press(Button::Right)) => {
                if !self.state.measuring {
                    measure(
                        &self.service,
                        self.report(gettext("Could not measure the screen")),
                    );
                }
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
        let shown = RulerPopover::new();
        let opener = seat.opener();

        shown.connect_activated({
            let service = self.service.clone();
            let report = self.report(gettext("Could not copy that measurement"));
            let opener = opener.clone();
            move |_, id| {
                copy(&service, report.clone(), id);
                opener.close_popover();
            }
        });
        shown.connect_measure_requested({
            let service = self.service.clone();
            let report = self.report(gettext("Could not measure the screen"));
            let opener = opener.clone();
            move |_| {
                opener.close_popover();
                if !service.snapshot().measuring {
                    measure(&service, report.clone());
                }
            }
        });
        let footer = self.footer.clone();
        shown.connect_footer_activated(move |_| footer.run());

        self.shown.set(Some(&shown));
        self.dress(&shown);
        Some(shown.upcast())
    }
}
