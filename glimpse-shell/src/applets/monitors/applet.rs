use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};
use tokio_util::sync::CancellationToken;

use crate::{
    panels::applets::AppletConfig,
    prompts::warning::{
        WarningDialog, WarningDialogInit, WarningDialogInput,
    },
    services::{
        compositor::{Command, CompositorHandle, State},
        framework::ServiceCommand,
    },
};

use super::popover::{
    Init as PopoverInit, Input as PopoverInput, Output as PopoverOutput, Popover,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config;

impl Config {
    pub fn from_raw(_raw: &Option<AppletConfig>) -> Self {
        Self
    }
}

pub struct Init {
    pub compositor: CompositorHandle,
    pub config: Config,
}

pub struct Applet {
    compositor: CompositorHandle,
    _config: Config,
    popover: Controller<Popover>,
    warning: Controller<WarningDialog>,
    popover_open: bool,
    subscription_cancel: CancellationToken,
}

#[derive(Debug)]
pub enum Input {
    TogglePopover,
    ServiceStateChanged(State),
    PopoverOutput(PopoverOutput),
}

#[relm4::component(pub)]
impl SimpleComponent for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = gtk::Box {
            add_css_class: "applet",
            add_css_class: "monitors-applet",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            set_tooltip_text: Some("Monitors"),

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(Input::TogglePopover);
                },
            },

            gtk::Image {
                set_pixel_size: 16,
                set_valign: gtk::Align::Center,
                set_icon_name: Some("video-display-symbolic"),
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = Popover::builder()
            .launch(PopoverInit {
                parent: root.clone(),
            })
            .forward(sender.input_sender(), Input::PopoverOutput);

        let warning = WarningDialog::builder()
            .launch(WarningDialogInit {
                parent: root.clone().upcast(),
            })
            .detach();

        let model = Applet {
            compositor: init.compositor,
            _config: init.config,
            popover,
            warning,
            popover_open: false,
            subscription_cancel: CancellationToken::new(),
        };

        let service = model.compositor.clone();
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

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::TogglePopover => {
                self.popover.emit(PopoverInput::Toggle);
            }
            Input::ServiceStateChanged(state) => {
                self.popover
                    .emit(PopoverInput::StateChanged(state.monitors));
            }
            Input::PopoverOutput(PopoverOutput::Opened) => {
                self.popover_open = true;
            }
            Input::PopoverOutput(PopoverOutput::Closed) => {
                self.popover_open = false;
            }
            Input::PopoverOutput(PopoverOutput::SetEnabled { name, on }) => {
                self.send_command(Command::SetMonitorEnabled { name, on });
            }
            Input::PopoverOutput(PopoverOutput::LastMonitorWarning { label }) => {
                self.warning.emit(WarningDialogInput::Show {
                    heading: "Cannot turn off this display".into(),
                    body: format!(
                        "\"{label}\" is your only active monitor. Glimpse keeps at least one display powered on."
                    ),
                });
            }
        }
    }
}

impl Applet {
    fn send_command(&self, command: Command) {
        let service = self.compositor.clone();
        relm4::spawn(async move {
            if let Err(error) = service.send(ServiceCommand::Command(command)).await {
                tracing::warn!(%error, "failed to send compositor command from monitors applet");
            }
        });
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}
