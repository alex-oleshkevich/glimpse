use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::services::{
    framework::ServiceCommand,
    idle_inhibitor::{Command, IdleInhibitorHandle, State},
    wayland_idle_inhibit::WaylandHealth,
};

use super::popover::{
    Init as PopoverInit, Input as PopoverInput, Output as PopoverOutput, Popover,
};

pub struct Applet {
    icon_name: &'static str,
    state: State,
    wayland_health: WaylandHealth,
    daemon_offline: bool,
    own_unique_name: String,
    service: IdleInhibitorHandle,
    popover: Controller<Popover>,
    subscription_cancel: CancellationToken,
}

pub struct Init {
    pub service: IdleInhibitorHandle,
    pub wayland_health: watch::Receiver<WaylandHealth>,
    pub own_unique_name: String,
}

#[derive(Debug)]
pub enum Input {
    ServiceStateChanged(State),
    WaylandHealthChanged(WaylandHealth),
    TogglePopover,
    PopoverCommand(Command),
}

#[relm4::component(pub)]
impl SimpleComponent for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = gtk::Box {
            add_css_class: "applet",
            add_css_class: "idle-applet",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            #[watch]
            set_tooltip_text: Some("Idle Inhibitor"),

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(Input::TogglePopover);
                },
            },

            gtk::Image {
                set_pixel_size: 16,
                set_valign: gtk::Align::Center,
                #[watch]
                set_icon_name: Some(model.icon_name),
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let own_unique_name = init.own_unique_name.clone();
        let popover = Popover::builder()
            .launch(PopoverInit {
                parent: root.clone(),
                own_unique_name: init.own_unique_name,
            })
            .forward(sender.input_sender(), |output| match output {
                PopoverOutput::Command(command) => Input::PopoverCommand(command),
            });

        let state = init.service.snapshot();
        let wayland_health = init.wayland_health.borrow().clone();

        let model = Applet {
            icon_name: icon_name_for_state(&state, &own_unique_name),
            state,
            wayland_health,
            daemon_offline: false,
            own_unique_name,
            service: init.service,
            popover,
            subscription_cancel: CancellationToken::new(),
        };

        // State subscription.
        let service = model.service.clone();
        let cancel = model.subscription_cancel.clone();
        let state_sender = sender.input_sender().clone();
        relm4::spawn(async move {
            let mut sub = service.subscribe();
            if state_sender
                .send(Input::ServiceStateChanged(sub.borrow().clone()))
                .is_err()
            {
                return;
            }
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    changed = sub.changed() => {
                        if changed.is_err() { break; }
                        if state_sender
                            .send(Input::ServiceStateChanged(sub.borrow().clone()))
                            .is_err()
                        { break; }
                    }
                }
            }
        });

        // Wayland health subscription.
        let mut wayland_rx = init.wayland_health.clone();
        let wayland_cancel = model.subscription_cancel.clone();
        let wayland_sender = sender.input_sender().clone();
        relm4::spawn(async move {
            loop {
                tokio::select! {
                    _ = wayland_cancel.cancelled() => break,
                    changed = wayland_rx.changed() => {
                        if changed.is_err() { break; }
                        let h = wayland_rx.borrow().clone();
                        if wayland_sender.send(Input::WaylandHealthChanged(h)).is_err() { break; }
                    }
                }
            }
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::ServiceStateChanged(state) => {
                self.icon_name = icon_name_for_state(&state, &self.own_unique_name);
                self.state = state;
                self.send_popover_state();
            }
            Input::WaylandHealthChanged(h) => {
                self.wayland_health = h;
                self.send_popover_state();
            }
            Input::TogglePopover => {
                self.popover.emit(PopoverInput::Toggle);
            }
            Input::PopoverCommand(command) => {
                self.send_command(command);
            }
        }
    }
}

impl Applet {
    fn send_popover_state(&self) {
        self.popover.emit(PopoverInput::UpdateState {
            state: self.state.clone(),
            wayland: self.wayland_health.clone(),
            daemon_offline: self.daemon_offline,
        });
    }

    fn send_command(&self, command: Command) {
        let service = self.service.clone();
        relm4::spawn(async move {
            if let Err(error) = service.send(ServiceCommand::Command(command)).await {
                tracing::warn!(%error, "failed to send idle inhibitor command");
            }
        });
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

fn icon_name_for_state(state: &State, own_unique_name: &str) -> &'static str {
    // The panel icon reflects the user's MANUAL HOLD state — flips when they
    // toggle the popover switch. External inhibitors (niri's power-key,
    // Firefox playing video, systemd-inhibit) live in the popover but don't
    // affect the panel icon: otherwise on a system with any persistent
    // external inhibitor the icon would be stuck "active" forever.
    let manual_hold_on = state
        .inhibitors
        .iter()
        .any(|r| r.bus_name == own_unique_name);
    if manual_hold_on {
        "view-reveal-symbolic"
    } else {
        "view-conceal-symbolic"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::idle_inhibitor::{
        IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets,
    };

    fn rec() -> IdleInhibitorRecord {
        IdleInhibitorRecord {
            id: 1,
            who: "x".into(),
            why: "y".into(),
            bus_name: ":1.1".into(),
            process_name: String::new(),
            source: IdleInhibitorSource::screen_saver(1),
            targets: InhibitionTargets::idle_only(),
            can_release: true,
            added_at_unix: 0,
        }
    }

    #[test]
    fn icon_reflects_manual_hold_not_total_inhibitors() {
        let mut state = State::default();
        let own = ":1.7";
        // Empty state -> conceal.
        assert_eq!(icon_name_for_state(&state, own), "view-conceal-symbolic");

        // An EXTERNAL inhibitor (different bus_name) does NOT flip the icon —
        // this is the key behavior for systems with persistent inhibitors
        // like niri's power-key handler.
        let mut external = rec();
        external.bus_name = ":1.99".into();
        state.inhibitors.push(external);
        assert_eq!(icon_name_for_state(&state, own), "view-conceal-symbolic");

        // Our OWN record (matching bus_name) flips it.
        let mut ours = rec();
        ours.id = 2;
        ours.bus_name = own.into();
        state.inhibitors.push(ours);
        assert_eq!(icon_name_for_state(&state, own), "view-reveal-symbolic");
    }
}
