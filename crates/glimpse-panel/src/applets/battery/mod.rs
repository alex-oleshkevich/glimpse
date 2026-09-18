pub mod render;

use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind, BatteryAppletConfig};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{BatteryHandle, BatteryState, CommandError};
use glimpse_widgets::{BatteryPopover, IndicatorSpec};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Input, Report, spawn_reported};

pub struct Battery {
    battery: BatteryHandle,
    notifications: NotificationsProviderHandle,
    state: BatteryState,
    settings: BatteryAppletConfig,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    spec: Vec<IndicatorSpec>,
    icon: Option<(String, gio::Icon)>,
    shown: glib::WeakRef<BatteryPopover>,
}

impl Battery {
    pub fn start(battery: BatteryHandle, notifications: NotificationsProviderHandle) -> Self {
        Self {
            state: battery.snapshot(),
            battery,
            notifications,
            settings: BatteryAppletConfig::default(),
            tooltip_format: None,
            footer: None,
            spec: Vec::new(),
            icon: None,
            shown: glib::WeakRef::new(),
        }
    }

    fn refresh(&mut self) {
        self.state = self.battery.snapshot();
        self.spec = self.indicator().into_iter().collect();
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn indicator(&mut self) -> Option<IndicatorSpec> {
        let mut spec = render::chip(
            &self.state,
            self.settings.indicator_style,
            &self.settings.label_format,
            self.tooltip_format.as_deref(),
        )?;
        if render::shows_icon(self.settings.indicator_style)
            && let Some(charge) = self.state.display.as_ref()
        {
            let name = render::icon(charge).to_owned();
            spec.icon = Some(self.themed(&name));
        }
        Some(spec)
    }

    fn themed(&mut self, name: &str) -> gio::Icon {
        if self.icon.as_ref().is_none_or(|(held, _)| held != name) {
            let icon = gio::ThemedIcon::new(name).upcast();
            self.icon = Some((name.to_owned(), icon));
        }
        match self.icon.as_ref() {
            Some((_, icon)) => icon.clone(),
            None => gio::ThemedIcon::new(name).upcast(),
        }
    }

    fn dress(&self, shown: &BatteryPopover) {
        match self.state.display.as_ref() {
            Some(charge) => {
                let (icon, subtitle, percentage) = render::heading(charge);
                shown.set_heading(Some(&icon), subtitle.as_deref(), Some(percentage));
            }
            None => shown.set_heading(Some("battery-missing-symbolic"), None, None),
        }
        let (choices, selected) = render::profiles(self.state.profile.as_ref());
        shown.set_profiles(&choices, selected);
        shown.set_devices(&render::devices(&self.state));
        match self.state.internals.first() {
            Some(supply) => shown.set_details(
                &render::facts(supply),
                render::charge_limit(supply).as_ref(),
            ),
            None => shown.set_details(&[], None),
        }
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
    }
}

impl Applet for Battery {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Battery(settings) = &config.kind else {
            return;
        };
        self.settings = settings.clone();
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()));
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        if matches!(input, Input::Woken) {
            self.refresh();
        }
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, _seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = BatteryPopover::new();
        shown.close_details();

        let battery = self.battery.clone();
        let notifications = self.notifications.clone();
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
                &notifications,
                "battery.set_profile",
                gettext("Could not change the power mode"),
                async move { battery.set_profile(name).await },
            );
        });

        let battery = self.battery.clone();
        let notifications = self.notifications.clone();
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
                &notifications,
                "battery.enable_charge_threshold",
                gettext("Could not change the charge limit"),
                async move { battery.enable_charge_threshold(path, enabled).await },
            );
        });

        if let Some((_, command)) = &self.footer {
            let command = command.clone();
            shown.connect_footer_activated(move |_| run(&command));
        }

        self.shown.set(Some(&shown));
        self.refresh();
        Some(Box::new(shown))
    }
}

fn tell<F, T>(
    notifications: &NotificationsProviderHandle,
    operation: &'static str,
    summary: String,
    future: F,
) where
    F: std::future::Future<Output = Result<T, CommandError>> + Send + 'static,
    T: Send + 'static,
{
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Battery"),
        icon: "battery-symbolic".to_owned(),
        summary,
    };
    spawn_reported(operation, report, wording, future);
}

fn wording(error: &CommandError) -> Option<String> {
    Some(match error {
        CommandError::InvalidArgument(_) => gettext("That was not a valid value."),
        CommandError::Unavailable(_) => gettext("The battery service is unavailable."),
        CommandError::Unsupported(_) => gettext("That is not supported."),
        CommandError::LimitExceeded(_) => gettext("That could not be completed."),
        CommandError::Internal(_) => gettext("That did not work."),
    })
}
