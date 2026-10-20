use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{Answer, BluetoothError, BluetoothHandle, BluetoothState, DeviceId, Hold};

use glimpse_widgets::{BluetoothPopover, IndicatorSpec};
use gtk4::glib;
use gtk4::prelude::*;
use render::Asked;

use crate::applet::popover::Seat;
use crate::applet::themed;
use crate::applet::{Applet, Ctx, Input, Opener, Report, report_failure, spawn_command};

use super::render;

pub struct Bluetooth {
    state: BluetoothState,
    bluetooth: BluetoothHandle,
    notifications: NotificationsProviderHandle,
    tooltip_format: Option<String>,
    spec: Vec<IndicatorSpec>,
    devices: usize,
    nearby: usize,
    nearby_toggled: Rc<Cell<bool>>,
    expanded: Rc<Cell<(bool, bool)>>,
    asked: Rc<RefCell<Option<Asked>>>,
    raised: Option<Asked>,
    shown: glib::WeakRef<BluetoothPopover>,
}

impl Applet for Bluetooth {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Bluetooth(cfg) = &config.kind else {
            return;
        };
        self.devices = cfg.devices;
        self.nearby = cfg.nearby;
        self.tooltip_format = config.kind.tooltip_format().map(str::to_owned);

        self.refresh();
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => {
                let was = render::powered(&self.state);
                self.state = self.bluetooth.snapshot();
                if !was
                    && render::powered(&self.state)
                    && self.shown.upgrade().is_some_and(|shown| shown.is_mapped())
                {
                    look(self.bluetooth.clone(), self.raised.is_some());
                }
                self.raise(ctx);
            }
            Input::Tick | Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<gtk4::Widget> {
        let shown = BluetoothPopover::new();

        shown.connect_powered({
            let bluetooth = self.bluetooth.clone();
            let notifications = self.notifications.clone();
            let opener = seat.opener();
            move |_, on| {
                let bluetooth = bluetooth.clone();
                tell(
                    &notifications,
                    &opener,
                    "bluetooth.set_powered",
                    gettext("Could not switch Bluetooth"),
                    async move { bluetooth.set_powered(on).await },
                );
            }
        });

        shown.connect_acted({
            let bluetooth = self.bluetooth.clone();
            let opener = seat.opener();
            let shown = shown.downgrade();
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
                            &shown,
                            &opener,
                            key,
                            async move { bluetooth.connect(id).await },
                        );
                    }
                    "pair" => {
                        let key = id.clone();
                        act(
                            &notifications,
                            "bluetooth.pair_device",
                            gettext("Could not pair"),
                            &shown,
                            &opener,
                            key,
                            async move { bluetooth.pair(id).await },
                        );
                    }
                    "disconnect" => {
                        let key = id.clone();
                        act(
                            &notifications,
                            "bluetooth.disconnect_device",
                            gettext("Could not disconnect"),
                            &shown,
                            &opener,
                            key,
                            async move { bluetooth.disconnect(id).await },
                        );
                    }
                    "forget" => tell(
                        &notifications,
                        &opener,
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
            let opener = seat.opener();
            move |_, id, action, on| {
                if action != "trust" {
                    return;
                }
                let id = DeviceId::new(id);
                let bluetooth = bluetooth.clone();
                tell(
                    &notifications,
                    &opener,
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
            let opener = seat.opener();
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
                            &opener,
                            "bluetooth.answer_pairing",
                            gettext("Could not pair"),
                            async move { bluetooth.answer_pairing(answer).await },
                        );
                    }
                    Asked::Forget(id) if accepted => tell(
                        &notifications,
                        &opener,
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

        shown.connect_nearby_toggled({
            let toggled = Rc::clone(&self.nearby_toggled);
            let opener = seat.opener();
            move |_| {
                toggled.set(!toggled.get());
                opener.wake();
            }
        });

        shown.connect_expanded({
            let expanded = Rc::clone(&self.expanded);
            let opener = seat.opener();
            move |_, place| {
                let (paired, nearby) = expanded.get();
                match place {
                    "paired" => expanded.set((!paired, nearby)),
                    "nearby" => expanded.set((paired, !nearby)),
                    _ => return,
                }
                opener.wake();
            }
        });

        let prompting = self.raised.is_some();
        shown.connect_map({
            let bluetooth = self.bluetooth.clone();
            move |_| {
                if render::powered(&bluetooth.snapshot()) {
                    look(bluetooth.clone(), prompting);
                }
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
                let bluetooth = bluetooth.clone();
                spawn_command("bluetooth.stop_scan", async move {
                    let stopped = bluetooth.stop_scan().await;
                    if let Err(error) = bluetooth.set_discoverable(false).await {
                        tracing::warn!(%error, "could not hide the adapter again");
                    }
                    stopped
                });
            }
        });

        self.expanded.set((false, false));
        self.nearby_toggled.set(false);
        self.shown.set(Some(&shown));
        self.refresh();
        Some(shown.upcast())
    }
}

fn look(bluetooth: BluetoothHandle, prompting: bool) {
    spawn_command("bluetooth.start_scan", async move {
        let scanned = match prompting {
            true => Ok(()),
            false => bluetooth.start_scan(Hold::Timed).await,
        };
        if let Err(error) = bluetooth.set_discoverable(true).await {
            tracing::warn!(%error, "could not make the adapter visible");
        }
        scanned
    });
}

fn tell<T: 'static>(
    notifications: &NotificationsProviderHandle,
    opener: &Opener,
    operation: &'static str,
    summary: String,
    future: impl std::future::Future<Output = Result<T, BluetoothError>> + 'static,
) {
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Bluetooth"),
        icon: render::IDLE.to_owned(),
        summary,
    };
    let opener = opener.clone();
    relm4::spawn_local(async move {
        if let Err(error) = future.await {
            report_failure(operation, report, wording(&error), error).await;
        }
        opener.wake();
    });
}

#[allow(clippy::too_many_arguments)]
fn act(
    notifications: &NotificationsProviderHandle,
    operation: &'static str,
    summary: String,
    shown: &glib::WeakRef<BluetoothPopover>,
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
    let shown = shown.clone();
    let opener = opener.clone();
    relm4::spawn_local(async move {
        match future.await {
            Ok(()) => {
                if let Some(shown) = shown.upgrade() {
                    shown.collapse(id.as_str());
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
            spec: Vec::new(),
            devices: 6,
            nearby: 8,
            expanded: Rc::new(Cell::new((false, false))),
            nearby_toggled: Rc::new(Cell::new(false)),
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
        if let Some(Asked::Pairing(_)) = &self.raised {
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
        shown.set_adapter(&hero.title, hero.on, hero.settable);
        shown.set_controls_sensitive(hero.controls);

        shown.set_prompt(render::prompt(&self.state).as_ref());

        let listing = render::entries(
            &self.state,
            self.devices,
            self.nearby,
            self.expanded.get(),
            self.nearby_toggled.get(),
        );
        shown.set_entries(&listing.entries);
        shown.set_nearby(listing.nearby_count, listing.nearby_open);
        shown.set_overflow(
            listing.more_paired.as_deref(),
            listing.more_nearby.as_deref(),
        );
        let details: Vec<_> = listing
            .entries
            .iter()
            .filter_map(|entry| render::details(&self.state, &DeviceId::new(&entry.id)))
            .collect();
        shown.set_details(&details);
        self.asked.replace(render::asked(&self.state));
    }

    fn indicator(&self) -> Option<IndicatorSpec> {
        let waiting = render::waiting(&self.state);
        Some(IndicatorSpec {
            icon: Some(themed(self.state.icon_name()?)),
            attention: waiting.is_some(),
            tooltip: waiting
                .or_else(|| render::tooltip(&self.state, self.tooltip_format.as_deref())),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests;
