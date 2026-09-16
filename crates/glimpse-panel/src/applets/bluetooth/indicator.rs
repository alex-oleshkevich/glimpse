use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{Answer, BluetoothError, BluetoothHandle, BluetoothState, DeviceId, Hold};

use glimpse_widgets::{BluetoothPopover, IndicatorSpec};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use render::Asked;

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{
    Applet, Ctx, Input, Opener, Report, report_failure, spawn_command, spawn_reported,
};

use super::render;

pub struct Bluetooth {
    state: BluetoothState,
    bluetooth: BluetoothHandle,
    notifications: NotificationsProviderHandle,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    spec: Vec<IndicatorSpec>,
    devices: usize,
    nearby: usize,
    selected: Rc<RefCell<Option<DeviceId>>>,
    expanded: Rc<Cell<(bool, bool)>>,
    asked: Rc<RefCell<Option<Asked>>>,
    raised: Option<Asked>,
    shown: glib::WeakRef<BluetoothPopover>,
}

fn themed(name: &str) -> gio::Icon {
    gio::ThemedIcon::new(name).upcast()
}

impl Applet for Bluetooth {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Bluetooth(cfg) = &config.kind else {
            return;
        };
        self.devices = cfg.devices;
        self.nearby = cfg.nearby;
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()));
        self.refresh();
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => {
                self.state = self.bluetooth.snapshot();
                self.raise(ctx);
            }
            Input::Tick | Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = BluetoothPopover::new();

        shown.connect_powered({
            let bluetooth = self.bluetooth.clone();
            let notifications = self.notifications.clone();
            move |_, on| {
                let bluetooth = bluetooth.clone();
                tell(
                    &notifications,
                    "bluetooth.set_powered",
                    gettext("Could not switch Bluetooth"),
                    async move { bluetooth.set_powered(on).await },
                );
            }
        });

        shown.connect_activated({
            let bluetooth = self.bluetooth.clone();
            let notifications = self.notifications.clone();
            let selected = Rc::clone(&self.selected);
            let opener = seat.opener();
            move |_, id, connected| {
                let id = DeviceId::new(id);
                let key = id.clone();
                let bluetooth = bluetooth.clone();
                match connected {
                    true => act(
                        &notifications,
                        "bluetooth.disconnect_device",
                        gettext("Could not disconnect"),
                        &selected,
                        &opener,
                        key,
                        async move { bluetooth.disconnect(id).await },
                    ),
                    false => act(
                        &notifications,
                        "bluetooth.connect_device",
                        gettext("Could not connect"),
                        &selected,
                        &opener,
                        key,
                        async move { bluetooth.connect(id).await },
                    ),
                }
            }
        });

        shown.connect_selected({
            let selected = Rc::clone(&self.selected);
            let opener = seat.opener();
            move |_, id| {
                let id = DeviceId::new(id);
                let mut held = selected.borrow_mut();
                *held = (held.as_ref() != Some(&id)).then_some(id);
                drop(held);
                opener.wake();
            }
        });

        shown.connect_acted({
            let bluetooth = self.bluetooth.clone();
            let opener = seat.opener();
            let selected = Rc::clone(&self.selected);
            let notifications = self.notifications.clone();
            move |_, id, action| {
                let id = DeviceId::new(id);
                let bluetooth = bluetooth.clone();
                match action {
                    "connect" => {
                        let key = id.clone();
                        act(
                            &notifications,
                            "bluetooth.connect_device",
                            gettext("Could not connect"),
                            &selected,
                            &opener,
                            key,
                            async move { bluetooth.connect(id).await },
                        );
                    }
                    "disconnect" => {
                        let key = id.clone();
                        act(
                            &notifications,
                            "bluetooth.disconnect_device",
                            gettext("Could not disconnect"),
                            &selected,
                            &opener,
                            key,
                            async move { bluetooth.disconnect(id).await },
                        );
                    }
                    "pair" => {
                        let key = id.clone();
                        act(
                            &notifications,
                            "bluetooth.pair_device",
                            gettext("Could not pair"),
                            &selected,
                            &opener,
                            key,
                            async move { bluetooth.pair(id).await },
                        );
                    }
                    "forget" => tell(
                        &notifications,
                        "bluetooth.forget_device",
                        gettext("Could not remove the device"),
                        async move { bluetooth.forget(id, false).await },
                    ),
                    _ => {}
                }
            }
        });

        shown.connect_toggled({
            let bluetooth = self.bluetooth.clone();
            let notifications = self.notifications.clone();
            move |_, id, action, on| {
                if action != "trust" {
                    return;
                }
                let id = DeviceId::new(id);
                let bluetooth = bluetooth.clone();
                tell(
                    &notifications,
                    "bluetooth.set_trusted",
                    gettext("Could not change that setting"),
                    async move { bluetooth.set_trusted(id, on).await },
                );
            }
        });

        shown.connect_answered({
            let bluetooth = self.bluetooth.clone();
            let notifications = self.notifications.clone();
            let asked = Rc::clone(&self.asked);
            move |_, accepted| {
                let bluetooth = bluetooth.clone();
                let Some(asked) = asked.borrow().clone() else {
                    return;
                };
                match asked {
                    Asked::Pairing(_) => {
                        let answer = match accepted {
                            true => Answer::Confirm,
                            false => Answer::Deny,
                        };
                        tell(
                            &notifications,
                            "bluetooth.answer_pairing",
                            gettext("Could not pair"),
                            async move { bluetooth.answer_pairing(answer).await },
                        );
                    }
                    Asked::Forget(id) if accepted => tell(
                        &notifications,
                        "bluetooth.forget_device",
                        gettext("Could not remove the device"),
                        async move { bluetooth.forget(id, true).await },
                    ),
                    Asked::Forget(_) => {
                        spawn_command("bluetooth.dismiss_confirmation", async move {
                            bluetooth.dismiss_confirmation().await
                        })
                    }
                }
            }
        });

        shown.connect_scanning({
            let bluetooth = self.bluetooth.clone();
            let notifications = self.notifications.clone();
            move |_, wanted| {
                let bluetooth = bluetooth.clone();
                let operation = match wanted {
                    true => "bluetooth.start_scan",
                    false => "bluetooth.stop_scan",
                };
                tell(
                    &notifications,
                    operation,
                    gettext("Could not look for devices"),
                    async move {
                        match wanted {
                            true => bluetooth.start_scan(Hold::Held).await,
                            false => bluetooth.stop_scan().await,
                        }
                    },
                );
            }
        });

        shown.connect_discoverable({
            let bluetooth = self.bluetooth.clone();
            let notifications = self.notifications.clone();
            move |_, wanted| {
                let bluetooth = bluetooth.clone();
                tell(
                    &notifications,
                    "bluetooth.set_discoverable",
                    gettext("Could not change visibility"),
                    async move { bluetooth.set_discoverable(wanted).await },
                );
            }
        });

        shown.connect_expanded({
            let expanded = Rc::clone(&self.expanded);
            let opener = seat.opener();
            move |_, place| {
                let (paired, nearby) = expanded.get();
                match place {
                    "paired" => expanded.set((true, nearby)),
                    "nearby" => expanded.set((paired, true)),
                    _ => return,
                }
                opener.wake();
            }
        });

        let powered = render::powered(&self.state);
        let prompting = self.raised.is_some();
        shown.connect_map({
            let bluetooth = self.bluetooth.clone();
            move |_| {
                if !powered {
                    return;
                }
                let visible = bluetooth.clone();
                let scan = bluetooth.clone();
                spawn_command("bluetooth.set_discoverable", async move {
                    visible.set_discoverable(true).await
                });
                if prompting {
                    return;
                }
                spawn_command("bluetooth.start_scan", async move {
                    scan.start_scan(Hold::Timed).await
                });
            }
        });

        shown.connect_unmap({
            let bluetooth = self.bluetooth.clone();
            let asked = Rc::clone(&self.asked);
            move |_| {
                if matches!(*asked.borrow(), Some(Asked::Forget(_))) {
                    let bluetooth = bluetooth.clone();
                    spawn_command("bluetooth.dismiss_confirmation", async move {
                        bluetooth.dismiss_confirmation().await
                    });
                }
                let stop = bluetooth.clone();
                spawn_command("bluetooth.set_discoverable", async move {
                    stop.set_discoverable(false).await
                });
                let bluetooth = bluetooth.clone();
                spawn_command(
                    "bluetooth.stop_scan",
                    async move { bluetooth.stop_scan().await },
                );
            }
        });

        if let Some((_, command)) = &self.footer {
            let command = command.clone();
            shown.connect_footer_activated(move |_| run(&command));
        }

        self.expanded.set((false, false));
        self.selected.replace(None);
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
    F: std::future::Future<Output = Result<T, BluetoothError>> + Send + 'static,
    T: Send + 'static,
{
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Bluetooth"),
        icon: render::IDLE.to_owned(),
        summary,
    };
    spawn_reported(operation, report, wording, future);
}

#[allow(clippy::too_many_arguments)]
fn act(
    notifications: &NotificationsProviderHandle,
    operation: &'static str,
    summary: String,
    selected: &Rc<RefCell<Option<DeviceId>>>,
    opener: &Opener,
    id: DeviceId,
    future: impl std::future::Future<Output = Result<(), BluetoothError>> + 'static,
) {
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Bluetooth"),
        icon: render::IDLE.to_owned(),
        summary,
    };
    let selected = Rc::clone(selected);
    let opener = opener.clone();
    relm4::spawn_local(async move {
        match future.await {
            Ok(()) => {
                let mine = selected.borrow().as_ref() == Some(&id);
                if mine {
                    selected.replace(None);
                }
            }
            Err(error) => report_failure(operation, report, wording(&error), error).await,
        }
        opener.wake();
    });
}

fn wording(error: &BluetoothError) -> Option<String> {
    error.failure().map(render::wording)
}

impl Bluetooth {
    pub fn start(bluetooth: BluetoothHandle, notifications: NotificationsProviderHandle) -> Self {
        let state = bluetooth.snapshot();
        Self {
            state,
            bluetooth,
            notifications,
            tooltip_format: None,
            footer: None,
            spec: Vec::new(),
            devices: 6,
            nearby: 8,
            selected: Rc::new(RefCell::new(None)),
            expanded: Rc::new(Cell::new((false, false))),
            asked: Rc::new(RefCell::new(None)),
            raised: None,
            shown: glib::WeakRef::new(),
        }
    }

    fn raise(&mut self, ctx: &Ctx) {
        let asking = render::asked(&self.state);
        if asking == self.raised {
            return;
        }
        self.raised = asking;
        if self.raised.is_some() {
            ctx.opener().open_popover();
        }
    }

    fn refresh(&mut self) {
        self.spec = self.indicator().into_iter().collect();
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn dress(&self, shown: &BluetoothPopover) {
        let hero = render::hero(&self.state);
        shown.set_adapter(
            &hero.title,
            &hero.subtitle,
            &hero.icon,
            hero.on,
            hero.settable,
        );
        shown.set_controls_sensitive(hero.controls);
        shown.set_discoverable(hero.discoverable);
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
        shown.set_scanning(self.state.scanning());

        let selected = self.selected.borrow();
        let selected = selected
            .as_ref()
            .filter(|id| self.state.device(id).is_some());
        let (paired_all, nearby_all) = self.expanded.get();
        let listing = render::entries(
            &self.state,
            selected,
            if paired_all { usize::MAX } else { self.devices },
            if nearby_all { usize::MAX } else { self.nearby },
        );
        shown.set_entries(&listing.entries);
        shown.set_overflow(
            listing.more_paired.as_deref(),
            listing.more_nearby.as_deref(),
        );
        shown.set_details(
            selected
                .and_then(|id| render::details(&self.state, id))
                .as_ref(),
        );
        shown.set_prompt(render::prompt(&self.state).as_ref());
        self.asked.replace(render::asked(&self.state));
    }

    fn indicator(&self) -> Option<IndicatorSpec> {
        let waiting = render::waiting(&self.state);
        Some(IndicatorSpec {
            icon: Some(themed(render::chip(&self.state)?)),
            attention: waiting.is_some(),
            tooltip: waiting
                .or_else(|| render::tooltip(&self.state, self.tooltip_format.as_deref())),
            ..Default::default()
        })
    }
}
