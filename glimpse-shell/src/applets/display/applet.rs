use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    panels::applets::AppletConfig,
    services::{
        compositor::{Command as CompositorCommand, CompositorHandle, State as CompositorState},
        framework::ServiceCommand,
    },
    widgets::panel_indicator::PanelIndicator,
};

use super::{
    format,
    popover::{Popover, PopoverInit, PopoverInput, PopoverOutput},
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(alias = "tooltip")]
    pub tooltip_format: String,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };
        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid display applet config, using defaults");
                Self::default()
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tooltip_format: format::DEFAULT_TOOLTIP_FORMAT.into(),
        }
    }
}

pub struct Applet {
    config: Config,
    compositor_state: CompositorState,
    tooltip: String,
    compositor: CompositorHandle,
    popover: Controller<Popover>,
    cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub compositor: CompositorHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    CompositorStateChanged(CompositorState),
    Reconfigure(Config),
    TogglePopover,
    PopoverOutput(PopoverOutput),
}

#[relm4::component(pub)]
impl SimpleComponent for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = PanelIndicator {
            #[watch]
            set_visible: !model.compositor_state.monitors.is_empty(),
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },
            set_icon: Some("video-display-symbolic"),
            connect_activated[sender] => move |_| {
                sender.input(Input::TogglePopover);
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
                parent: root.clone().upcast::<gtk::Box>(),
            })
            .forward(sender.input_sender(), Input::PopoverOutput);

        let compositor_state = init.compositor.snapshot();
        let tooltip = format::tooltip(&init.config.tooltip_format, &compositor_state.monitors);

        let model = Applet {
            tooltip,
            config: init.config,
            compositor_state,
            compositor: init.compositor,
            popover,
            cancel: CancellationToken::new(),
        };

        let compositor = model.compositor.clone();
        let cancel = model.cancel.clone();
        let compositor_sender = sender.input_sender().clone();
        relm4::spawn(async move {
            let mut sub = compositor.subscribe();
            if compositor_sender
                .send(Input::CompositorStateChanged(sub.borrow().clone()))
                .is_err()
            {
                return;
            }
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    changed = sub.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        if compositor_sender
                            .send(Input::CompositorStateChanged(sub.borrow().clone()))
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::CompositorStateChanged(state) => {
                self.compositor_state = state;
                self.apply_state();
            }
            Input::Reconfigure(config) => {
                self.config = config;
                self.apply_state();
            }
            Input::TogglePopover => {
                self.popover.emit(PopoverInput::UpdateMonitors(
                    self.compositor_state.monitors.clone(),
                ));
                self.popover.emit(PopoverInput::Toggle);
            }
            Input::PopoverOutput(output) => match output {
                PopoverOutput::SetMonitorEnabled { name, on } => {
                    self.send_compositor_command(CompositorCommand::SetMonitorEnabled {
                        name,
                        on,
                    });
                }
            },
        }
    }
}

impl Applet {
    fn apply_state(&mut self) {
        self.tooltip =
            format::tooltip(&self.config.tooltip_format, &self.compositor_state.monitors);
        self.popover.emit(PopoverInput::UpdateMonitors(
            self.compositor_state.monitors.clone(),
        ));
    }

    fn send_compositor_command(&self, command: CompositorCommand) {
        let compositor = self.compositor.clone();
        relm4::spawn(async move {
            if let Err(error) = compositor.send(ServiceCommand::Command(command)).await {
                tracing::warn!(%error, "failed to send compositor command");
            }
        });
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_tooltip_format() {
        let config = Config::default();
        assert_eq!(config.tooltip_format, format::DEFAULT_TOOLTIP_FORMAT);
    }
}
