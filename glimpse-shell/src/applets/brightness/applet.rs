use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, glib, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    panels::applets::AppletConfig,
    services::{
        brightness::{BrightnessHandle, Command, State},
        compositor::{Command as CompositorCommand, CompositorHandle, State as CompositorState},
        framework::ServiceCommand,
    },
};

use super::{
    format,
    popover::{Popover, PopoverInit, PopoverInput, PopoverOutput},
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(alias = "label")]
    pub label_format: String,
    #[serde(alias = "tooltip")]
    pub tooltip_format: String,
    pub scroll_step: u8,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid brightness applet config, using defaults");
                Self::default()
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            label_format: format::DEFAULT_LABEL_FORMAT.into(),
            tooltip_format: format::DEFAULT_TOOLTIP_FORMAT.into(),
            scroll_step: 10,
        }
    }
}

pub struct Applet {
    config: Config,
    panel_monitor: Option<String>,
    service_state: State,
    compositor_state: CompositorState,
    state: State,
    icon_name: String,
    label: String,
    tooltip: String,
    service: BrightnessHandle,
    compositor: CompositorHandle,
    popover: Controller<Popover>,
    popover_open: bool,
    brightness_cancel: CancellationToken,
    compositor_cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub service: BrightnessHandle,
    pub compositor: CompositorHandle,
    pub config: Config,
    pub panel_monitor: Option<String>,
}

#[derive(Debug)]
pub enum Input {
    ServiceStateChanged(State),
    CompositorStateChanged(CompositorState),
    Reconfigure(Config),
    Scroll(f64),
    TogglePopover,
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
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            #[watch]
            set_visible: model.state.available,
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(Input::TogglePopover);
                },
            },

            add_controller = gtk::EventControllerScroll::new(
                gtk::EventControllerScrollFlags::VERTICAL
            ) {
                connect_scroll[sender] => move |_, _dx, dy| {
                    sender.input(Input::Scroll(dy));
                    glib::Propagation::Stop
                },
            },

            gtk::Image {
                set_pixel_size: 16,
                set_valign: gtk::Align::Center,
                #[watch]
                set_icon_name: Some(&model.icon_name),
            },

            gtk::Label {
                add_css_class: "brightness-label",
                set_valign: gtk::Align::Center,
                #[watch]
                set_label: &model.label,
                #[watch]
                set_visible: !model.label.is_empty(),
            }
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

        let service_state = init.service.snapshot();
        let compositor_state = init.compositor.snapshot();
        let state = visible_state(&service_state, &compositor_state);
        let model = Applet {
            icon_name: format::icon_name(&state).into(),
            label: format::label(&init.config.label_format, &state),
            tooltip: format::tooltip(&init.config.tooltip_format, &state),
            config: init.config,
            panel_monitor: init.panel_monitor,
            service_state,
            compositor_state,
            state,
            service: init.service,
            compositor: init.compositor,
            popover,
            popover_open: false,
            brightness_cancel: CancellationToken::new(),
            compositor_cancel: CancellationToken::new(),
        };

        let service = model.service.clone();
        let cancel = model.brightness_cancel.clone();
        let subscription_sender = sender.input_sender().clone();
        relm4::spawn(async move {
            let mut sub = service.subscribe();
            if subscription_sender
                .send(Input::ServiceStateChanged(sub.borrow().clone()))
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

                        if subscription_sender
                            .send(Input::ServiceStateChanged(sub.borrow().clone()))
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });

        let compositor = model.compositor.clone();
        let cancel = model.compositor_cancel.clone();
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
            Input::ServiceStateChanged(state) => {
                self.service_state = state;
                self.apply_filtered_state();
            }
            Input::CompositorStateChanged(state) => {
                self.compositor_state = state;
                self.apply_filtered_state();
                if self.popover_open {
                    self.sync_popover_monitors();
                }
            }
            Input::Reconfigure(config) => {
                self.config = config;
                self.service_state = self.service.snapshot();
                self.apply_filtered_state();
            }
            Input::Scroll(dy) => {
                let Some(source) = scroll_source(
                    &self.state,
                    self.panel_monitor.as_deref(),
                    &self.compositor_state,
                ) else {
                    return;
                };

                let delta = if dy > 0.0 {
                    -(self.config.scroll_step as i32)
                } else {
                    self.config.scroll_step as i32
                };
                self.send_command(Command::AdjustPercent {
                    id: source.id.clone(),
                    delta,
                });
            }
            Input::TogglePopover => {
                self.popover.emit(PopoverInput::Toggle);
            }
            Input::PopoverOutput(output) => match output {
                PopoverOutput::Opened => {
                    self.popover_open = true;
                    self.sync_popover_state();
                    self.send_command(Command::Refresh);
                }
                PopoverOutput::Closed => {
                    self.popover_open = false;
                }
                PopoverOutput::Command(command) => {
                    self.send_command(command);
                }
                output => {
                    if let Some(command) = compositor_command_for_popover_output(&output) {
                        self.send_compositor_command(command);
                    }
                }
            },
        }
    }
}

impl Applet {
    fn apply_filtered_state(&mut self) {
        let state = visible_state(&self.service_state, &self.compositor_state);
        self.icon_name = format::icon_name(&state).into();
        self.label = format::label(&self.config.label_format, &state);
        self.tooltip = format::tooltip(&self.config.tooltip_format, &state);
        self.state = state.clone();
        if self.popover_open {
            self.popover.emit(PopoverInput::UpdateState(state));
        }
    }

    fn sync_popover_state(&self) {
        self.popover
            .emit(PopoverInput::UpdateState(self.state.clone()));
        self.sync_popover_monitors();
    }

    fn sync_popover_monitors(&self) {
        self.popover.emit(PopoverInput::UpdateMonitors(
            self.compositor_state.monitors.clone(),
        ));
    }

    fn send_command(&self, command: Command) {
        let service = self.service.clone();
        relm4::spawn(async move {
            if let Err(error) = service.send(ServiceCommand::Command(command)).await {
                tracing::warn!(%error, "failed to send brightness command");
            }
        });
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
        self.brightness_cancel.cancel();
        self.compositor_cancel.cancel();
    }
}

fn visible_state(state: &State, compositor: &CompositorState) -> State {
    let mut state = state.clone();
    if should_hide_builtin_display(compositor) {
        state.sources.retain(|source| {
            source.kind != glimpse_core::services::brightness::BrightnessSourceKind::BuiltInDisplay
        });
        normalize_visible_primary(&mut state);
    }
    state.available = state.sources.iter().any(|source| source.is_usable());
    state
}

fn should_hide_builtin_display(compositor: &CompositorState) -> bool {
    !compositor.monitors.is_empty()
        && compositor
            .monitors
            .iter()
            .any(|monitor| internal_monitor_name(&monitor.name))
        && !compositor.monitors.iter().any(|monitor| {
            internal_monitor_name(&monitor.name) && monitor.active_workspace.is_some()
        })
}

fn internal_monitor_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("edp") || name.starts_with("lvds") || name.starts_with("dsi")
}

fn normalize_visible_primary(state: &mut State) {
    let mut primary_seen = false;
    for source in &mut state.sources {
        if source.primary && source.is_usable() && !primary_seen {
            primary_seen = true;
        } else {
            source.primary = false;
        }
    }
    if !primary_seen
        && let Some(source) = state.sources.iter_mut().find(|source| source.is_usable())
    {
        source.primary = true;
    }
}

fn scroll_source<'a>(
    state: &'a State,
    panel_monitor: Option<&str>,
    compositor: &CompositorState,
) -> Option<&'a glimpse_core::services::brightness::BrightnessSource> {
    let monitor = panel_monitor.or_else(|| {
        compositor
            .monitors
            .iter()
            .find(|monitor| monitor.focused)
            .map(|monitor| monitor.name.as_str())
    });

    if let Some(monitor) = monitor {
        if let Some(source) = source_for_connector(state, monitor) {
            return Some(source);
        }
    }

    format::primary_source(state)
}

fn source_for_connector<'a>(
    state: &'a State,
    connector: &str,
) -> Option<&'a glimpse_core::services::brightness::BrightnessSource> {
    state.sources.iter().find(|source| {
        source.is_usable()
            && source
                .connector
                .as_deref()
                .is_some_and(|source_connector| source_connector.eq_ignore_ascii_case(connector))
    })
}

fn compositor_command_for_popover_output(output: &PopoverOutput) -> Option<CompositorCommand> {
    match output {
        PopoverOutput::SetMonitorEnabled { name, on } => {
            Some(CompositorCommand::SetMonitorEnabled {
                name: name.clone(),
                on: *on,
            })
        }
        PopoverOutput::Opened | PopoverOutput::Closed | PopoverOutput::Command(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::compositors::Monitor;
    use glimpse_core::services::brightness::{BrightnessSource, BrightnessSourceKind};

    #[test]
    fn config_defaults_to_empty_label_and_ten_percent_scroll() {
        let config = Config::default();

        assert_eq!(config.label_format, "");
        assert_eq!(config.scroll_step, 10);
    }

    #[test]
    fn icon_uses_primary_source() {
        let state = State {
            available: true,
            sources: vec![BrightnessSource {
                id: "backlight:intel_backlight".into(),
                name: "Intel backlight".into(),
                connector: None,
                kind: BrightnessSourceKind::BuiltInDisplay,
                icon: "display-brightness-symbolic".into(),
                current: 50,
                max: 100,
                percent: 50,
                writable: true,
                primary: true,
                available: true,
            }],
            active: None,
        };

        assert_eq!(format::icon_name(&state), "display-brightness-symbolic");
    }

    #[test]
    fn visible_state_hides_builtin_display_when_internal_monitor_is_inactive() {
        let state = State {
            available: true,
            sources: vec![
                source(
                    "backlight:intel_backlight",
                    BrightnessSourceKind::BuiltInDisplay,
                    true,
                ),
                source("keyboard:upower", BrightnessSourceKind::Keyboard, false),
            ],
            active: None,
        };
        let compositor = CompositorState {
            monitors: vec![monitor("eDP-1", None)],
            ..CompositorState::default()
        };

        let visible = visible_state(&state, &compositor);

        assert_eq!(visible.sources.len(), 1);
        assert_eq!(visible.sources[0].id, "keyboard:upower");
        assert!(visible.sources[0].primary);
    }

    #[test]
    fn visible_state_keeps_builtin_display_when_internal_monitor_is_active() {
        let state = State {
            available: true,
            sources: vec![source(
                "backlight:intel_backlight",
                BrightnessSourceKind::BuiltInDisplay,
                true,
            )],
            active: None,
        };
        let compositor = CompositorState {
            monitors: vec![monitor("eDP-1", Some(1))],
            ..CompositorState::default()
        };

        let visible = visible_state(&state, &compositor);

        assert_eq!(visible.sources.len(), 1);
        assert_eq!(visible.sources[0].id, "backlight:intel_backlight");
    }

    #[test]
    fn visible_state_keeps_builtin_display_when_compositor_outputs_are_unknown() {
        let state = State {
            available: true,
            sources: vec![source(
                "backlight:intel_backlight",
                BrightnessSourceKind::BuiltInDisplay,
                true,
            )],
            active: None,
        };

        let visible = visible_state(&state, &CompositorState::default());

        assert_eq!(visible.sources.len(), 1);
    }

    #[test]
    fn scroll_source_prefers_panel_monitor_connector() {
        let state = display_state();

        let source = scroll_source(&state, Some("DP-2"), &CompositorState::default()).unwrap();

        assert_eq!(source.id, "ddcutil:2");
    }

    #[test]
    fn scroll_source_uses_focused_monitor_when_panel_monitor_is_unknown() {
        let state = display_state();
        let compositor = CompositorState {
            monitors: vec![focused_monitor("DP-2")],
            ..CompositorState::default()
        };

        let source = scroll_source(&state, None, &compositor).unwrap();

        assert_eq!(source.id, "ddcutil:2");
    }

    #[test]
    fn scroll_source_falls_back_to_primary_source() {
        let state = display_state();

        let source = scroll_source(&state, Some("HDMI-A-1"), &CompositorState::default()).unwrap();

        assert_eq!(source.id, "backlight:intel_backlight");
    }

    #[test]
    fn monitor_toggle_popover_output_maps_to_compositor_command() {
        let command = compositor_command_for_popover_output(&PopoverOutput::SetMonitorEnabled {
            name: "DP-2".into(),
            on: false,
        });

        assert!(matches!(
            command,
            Some(CompositorCommand::SetMonitorEnabled { name, on })
                if name == "DP-2" && !on
        ));
        assert!(
            compositor_command_for_popover_output(&PopoverOutput::Command(Command::Refresh))
                .is_none()
        );
    }

    fn source(id: &str, kind: BrightnessSourceKind, primary: bool) -> BrightnessSource {
        BrightnessSource {
            id: id.into(),
            name: id.into(),
            connector: None,
            kind,
            icon: "display-brightness-symbolic".into(),
            current: 50,
            max: 100,
            percent: 50,
            writable: true,
            primary,
            available: true,
        }
    }

    fn source_on_connector(
        id: &str,
        kind: BrightnessSourceKind,
        primary: bool,
        connector: &str,
    ) -> BrightnessSource {
        let mut source = source(id, kind, primary);
        source.connector = Some(connector.into());
        source
    }

    fn display_state() -> State {
        State {
            available: true,
            sources: vec![
                source_on_connector(
                    "backlight:intel_backlight",
                    BrightnessSourceKind::BuiltInDisplay,
                    true,
                    "eDP-1",
                ),
                source_on_connector(
                    "ddcutil:2",
                    BrightnessSourceKind::ExternalDisplay,
                    false,
                    "DP-2",
                ),
            ],
            active: None,
        }
    }

    fn monitor(name: &str, active_workspace: Option<usize>) -> Monitor {
        Monitor {
            id: None,
            name: name.into(),
            description: None,
            active_workspace,
            focused: false,
            make: None,
            model: None,
            enabled: true,
            built_in: false,
            current_mode: None,
        }
    }

    fn focused_monitor(name: &str) -> Monitor {
        Monitor {
            id: None,
            name: name.into(),
            description: None,
            active_workspace: Some(1),
            focused: true,
            make: None,
            model: None,
            enabled: true,
            built_in: false,
            current_mode: None,
        }
    }
}
