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
use crate::applet::{Applet, Ctx, Input, Opener, Report, report_failure, wording};

pub struct Battery {
    battery: BatteryHandle,
    notifications: NotificationsProviderHandle,
    state: BatteryState,
    settings: BatteryAppletConfig,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    twelve: bool,
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
            twelve: false,
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
            let name = charge.icon_name();
            spec.icon = Some(self.themed(&name));
        }
        Some(spec)
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

    fn full_at(&self) -> Option<String> {
        let seconds = self.state.display.as_ref()?.time_to_full?;
        let at = chrono::Local::now() + chrono::Duration::seconds(i64::from(seconds));
        Some(at.format(glimpse_config::clock(self.twelve)).to_string())
    }

    fn dress(&self, shown: &BatteryPopover) {
        match render::heading(&self.state, self.full_at().as_deref()) {
            Some(heading) => shown.set_heading(
                Some(&heading.icon),
                heading.subtitle.as_deref(),
                Some(heading.percentage),
                heading.severity,
            ),
            None => shown.set_heading(Some("battery-missing-symbolic"), None, None, None),
        }
        let (choices, selected) = render::profiles(self.state.profile.as_ref());
        shown.set_profiles(&choices, selected);
        shown.set_devices(&render::devices(&self.state));
        let supply = self.state.internals.first();
        let health = supply.and_then(render::health);
        shown.set_health(
            health.as_ref().map(|(value, _)| value.as_str()),
            health.as_ref().is_some_and(|(_, warning)| *warning),
            &supply.map(render::facts).unwrap_or_default(),
        );
        shown.set_charge_limit(supply.and_then(render::charge_limit).as_ref());
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
        self.twelve = config.regional.twelve_hour();
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

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = BatteryPopover::new();

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
                &notifications,
                opener.clone(),
                "battery.set_profile",
                gettext("Could not change the power mode"),
                async move { battery.set_profile(name).await },
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
                &notifications,
                opener.clone(),
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
        app_name: gettext("Battery"),
        icon: "battery-symbolic".to_owned(),
        summary,
    };
    let unavailable = gettext("The battery service is unavailable.");
    relm4::spawn_local(async move {
        let Err(error) = future.await else {
            return;
        };
        opener.wake();
        report_failure(operation, report, wording(&error, &unavailable), error).await;
    });
}
