use std::cell::RefCell;
use std::rc::Rc;

use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind, KdeconnectAppletConfig};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{KdeconnectDeviceId, KdeconnectHandle, KdeconnectState};
use glimpse_widgets::{IndicatorSpec, KdeconnectPopover, Severity};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Input, Opener, Report, report_failure, wording};

use super::render;

const SETTINGS_COMMAND: &str = "kdeconnect-app";

pub struct Kdeconnect {
    kdeconnect: KdeconnectHandle,
    notifications: NotificationsProviderHandle,
    state: Rc<RefCell<KdeconnectState>>,
    config: KdeconnectAppletConfig,
    tooltip_format: Option<String>,
    footer: (String, Vec<String>),
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<KdeconnectPopover>,
    opener: Option<Opener>,
}

fn themed(name: &str) -> gio::Icon {
    gio::ThemedIcon::new(name).upcast()
}

fn default_footer() -> (String, Vec<String>) {
    (
        gettext("KDE Connect settings"),
        vec![SETTINGS_COMMAND.to_owned()],
    )
}

impl Applet for Kdeconnect {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Kdeconnect(cfg) = &config.kind else {
            return;
        };
        self.config = cfg.clone();
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()))
            .unwrap_or_else(default_footer);
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => {
                self.state.replace(self.kdeconnect.snapshot());
            }
            Input::Tick | Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = KdeconnectPopover::new();
        shown.set_nearby_open(!render::any_paired(&self.state.borrow()));

        shown.connect_map({
            let kdeconnect = self.kdeconnect.clone();
            move |_| {
                let kdeconnect = kdeconnect.clone();
                relm4::spawn(async move {
                    if let Err(error) = kdeconnect.discover().await {
                        tracing::debug!(%error, "kdeconnect discovery was refused");
                    }
                });
            }
        });

        shown.connect_action({
            let kdeconnect = self.kdeconnect.clone();
            let notifications = self.notifications.clone();
            let opener = seat.opener();
            move |popover, id, key| {
                let id = KdeconnectDeviceId::new(id);
                if key == render::SHARE {
                    opener.close_popover();
                    send_files(kdeconnect.clone(), notifications.clone(), id);
                    return;
                }
                let Some(action) = render::action(key) else {
                    return;
                };
                if matches!(
                    action,
                    glimpse_services::KdeconnectAction::Browse
                        | glimpse_services::KdeconnectAction::OpenMessages
                ) {
                    opener.close_popover();
                }
                let kdeconnect = kdeconnect.clone();
                let collapse = popover.downgrade();
                let key = id.as_str().to_owned();
                act(
                    &notifications,
                    "kdeconnect.act",
                    async move { kdeconnect.act(id, action).await },
                    move || {
                        if let Some(popover) = collapse.upgrade() {
                            popover.collapse(&key);
                        }
                    },
                );
            }
        });

        shown.connect_pair({
            let kdeconnect = self.kdeconnect.clone();
            let notifications = self.notifications.clone();
            let opener = seat.opener();
            move |_, id| {
                let kdeconnect = kdeconnect.clone();
                let opener = opener.clone();
                let id = KdeconnectDeviceId::new(id);
                act(
                    &notifications,
                    "kdeconnect.pair",
                    async move {
                        let asked = kdeconnect
                            .act(id, glimpse_services::KdeconnectAction::Pair)
                            .await;
                        opener.wake();
                        asked
                    },
                    || {},
                );
            }
        });

        let command = self.footer.1.clone();
        shown.connect_footer_activated(move |_| run(&command));

        self.opener = Some(seat.opener());
        self.shown.set(Some(&shown));
        self.refresh();
        Some(Box::new(shown))
    }
}

fn report(notifications: &NotificationsProviderHandle) -> Report {
    Report {
        notifications: notifications.clone(),
        app_name: gettext("KDE Connect"),
        icon: render::ICON.to_owned(),
        summary: gettext("The device did not respond"),
    }
}

fn act<F>(
    notifications: &NotificationsProviderHandle,
    operation: &'static str,
    future: F,
    succeeded: impl FnOnce() + 'static,
) where
    F: std::future::Future<Output = Result<(), glimpse_services::CommandError>> + Send + 'static,
{
    let report = report(notifications);
    relm4::spawn_local(async move {
        match future.await {
            Ok(()) => succeeded(),
            Err(error) => {
                let body = wording(&error, &gettext("KDE Connect is not running."));
                report_failure(operation, report, body, error).await;
            }
        }
    });
}

fn send_files(
    kdeconnect: KdeconnectHandle,
    notifications: NotificationsProviderHandle,
    id: KdeconnectDeviceId,
) {
    relm4::spawn_local(async move {
        let dialog = gtk4::FileDialog::builder()
            .title(gettext("Send files"))
            .modal(false)
            .build();
        let Ok(files) = dialog.open_multiple_future(None::<&gtk4::Window>).await else {
            return;
        };
        let urls: Vec<String> = files
            .iter::<gio::File>()
            .filter_map(Result::ok)
            .map(|file| file.uri().to_string())
            .collect();
        if urls.is_empty() {
            return;
        }
        act(
            &notifications,
            "kdeconnect.share",
            async move { kdeconnect.share(id, urls).await },
            || {},
        );
    });
}

impl Kdeconnect {
    pub fn start(kdeconnect: KdeconnectHandle, notifications: NotificationsProviderHandle) -> Self {
        let state = kdeconnect.snapshot();
        Self {
            kdeconnect,
            notifications,
            state: Rc::new(RefCell::new(state)),
            config: KdeconnectAppletConfig::default(),
            tooltip_format: None,
            footer: default_footer(),
            spec: Vec::new(),
            shown: glib::WeakRef::new(),
            opener: None,
        }
    }

    fn refresh(&mut self) {
        self.spec = self.indicator().into_iter().collect();
        if let Some(shown) = self.shown.upgrade() {
            if !self.state.borrow().running {
                if let Some(opener) = &self.opener {
                    opener.close_popover();
                }
                return;
            }
            self.dress(&shown);
        }
    }

    fn dress(&self, shown: &KdeconnectPopover) {
        let state = self.state.borrow();
        shown.set_summary(&render::summary(&state));
        shown.set_devices(&render::devices(&state));
        let (nearby, more) = render::nearby(&state);
        shown.set_nearby(&nearby, more.as_deref());
        shown.set_footer(Some(self.footer.0.as_str()));
    }

    fn indicator(&self) -> Option<IndicatorSpec> {
        let state = self.state.borrow();
        let chip = render::chip(&state, &self.config)?;
        let tooltip = match &self.tooltip_format {
            Some(format) => render::tooltip(&state, &self.config, format),
            None => chip.tooltip,
        };
        Some(IndicatorSpec {
            icon: chip.icon.map(themed),
            overlay: chip.overlay.map(themed),
            label: chip.label,
            tooltip: Some(tooltip),
            notice: chip.notice,
            severity: chip.low.then_some(Severity::Warning),
            ..Default::default()
        })
    }
}
