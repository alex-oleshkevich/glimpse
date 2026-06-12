use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    panels::applets::AppletConfig,
    services::{
        brightness::{BrightnessHandle, Command, State},
        compositor::{CompositorHandle, State as CompositorState},
        framework::ServiceCommand,
    },
    utils::subscribe_service,
    widgets::panel_indicator::PanelIndicator,
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
    popover_state: State,
    popover_monitors: Vec<glimpse_core::compositors::Monitor>,
    visible_state: State,
    icon_name: String,
    label: String,
    tooltip: String,
    service: BrightnessHandle,
    popover: Controller<Popover>,
    display_cancel: CancellationToken,
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
        root = PanelIndicator {
            #[watch]
            set_visible: model.visible_state.available,
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },
            #[watch]
            set_icon: Some(model.icon_name.as_str()),
            #[watch]
            set_label: if model.label.is_empty() { None } else { Some(model.label.as_str()) },
            connect_activated[sender] => move |_| {
                sender.input(Input::TogglePopover);
            },
            connect_scrolled[sender] => move |_, _dx, dy| {
                if dy != 0.0 {
                    sender.input(Input::Scroll(dy));
                }
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
                parent: root.clone().upcast::<gtk::Box>(),
            })
            .forward(sender.input_sender(), Input::PopoverOutput);

        let service_state = init.service.snapshot();
        let compositor_state = init.compositor.snapshot();
        let state = visible_state(&service_state, &compositor_state);
        let display_cancel = subscribe_service(
            init.service.subscribe(),
            sender.input_sender().clone(),
            Input::ServiceStateChanged,
        );
        let compositor_cancel = subscribe_service(
            init.compositor.subscribe(),
            sender.input_sender().clone(),
            Input::CompositorStateChanged,
        );
        let model = Applet {
            icon_name: format::icon_name(&state).into(),
            label: format::label_with_monitors(
                &init.config.label_format,
                &state,
                &compositor_state.monitors,
            ),
            tooltip: format::tooltip_with_monitors(
                &init.config.tooltip_format,
                &state,
                &compositor_state.monitors,
            ),
            config: init.config,
            panel_monitor: init.panel_monitor,
            service_state,
            compositor_state,
            popover_state: state.clone(),
            popover_monitors: Vec::new(),
            visible_state: state,
            service: init.service,
            popover,
            display_cancel,
            compositor_cancel,
        };

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
                self.sync_popover_monitors_if_changed();
            }
            Input::Reconfigure(config) => {
                self.config = config;
                self.service_state = self.service.snapshot();
                self.apply_filtered_state();
            }
            Input::Scroll(dy) => {
                let Some(source) = scroll_source(
                    &self.visible_state,
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
                self.sync_popover_state();
                self.send_command(Command::Refresh);
                self.popover.emit(PopoverInput::Toggle);
            }
            Input::PopoverOutput(output) => match output {
                PopoverOutput::Command(command) => {
                    self.send_command(command);
                }
            },
        }
    }
}

impl Applet {
    fn apply_filtered_state(&mut self) {
        let state = visible_state(&self.service_state, &self.compositor_state);
        self.icon_name = format::icon_name(&state).into();
        self.label = format::label_with_monitors(
            &self.config.label_format,
            &state,
            &self.compositor_state.monitors,
        );
        self.tooltip = format::tooltip_with_monitors(
            &self.config.tooltip_format,
            &state,
            &self.compositor_state.monitors,
        );
        self.visible_state = state.clone();
        if set_if_changed(&mut self.popover_state, state.clone()) {
            self.popover.emit(PopoverInput::UpdateState(state));
        }
    }

    fn sync_popover_state(&mut self) {
        self.popover_state = self.visible_state.clone();
        self.popover
            .emit(PopoverInput::UpdateState(self.visible_state.clone()));
        self.popover_monitors = self.compositor_state.monitors.clone();
        self.popover.emit(PopoverInput::UpdateMonitors(
            self.compositor_state.monitors.clone(),
        ));
    }

    fn sync_popover_monitors_if_changed(&mut self) {
        let monitors = self.compositor_state.monitors.clone();
        if set_if_changed(&mut self.popover_monitors, monitors.clone()) {
            self.popover.emit(PopoverInput::UpdateMonitors(monitors));
        }
    }

    fn send_command(&self, command: Command) {
        let service = self.service.clone();
        relm4::spawn(async move {
            if let Err(error) = service.send(ServiceCommand::Command(command)).await {
                tracing::warn!(%error, "failed to send brightness command");
            }
        });
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.display_cancel.cancel();
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

fn set_if_changed<T>(slot: &mut T, value: T) -> bool
where
    T: PartialEq,
{
    if *slot == value {
        return false;
    }
    *slot = value;
    true
}

fn scroll_source<'a>(
    state: &'a State,
    panel_monitor: Option<&str>,
    compositor: &CompositorState,
) -> Option<&'a glimpse_core::services::brightness::BrightnessSource> {
    let monitor = compositor
        .monitors
        .iter()
        .find(|monitor| monitor.focused)
        .map(|monitor| monitor.name.as_str())
        .or(panel_monitor);

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

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::compositors::Monitor;
    use glimpse_core::services::brightness::{BrightnessSource, BrightnessSourceKind};

    #[test]
    fn config_defaults_to_empty_label_and_ten_percent_scroll() {
        let config = Config::default();

        assert_eq!(config.label_format, "");
        assert_eq!(config.tooltip_format, "{source}: {percent}%");
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
    fn scroll_source_prefers_focused_monitor_over_panel_monitor() {
        let state = display_state();
        let compositor = CompositorState {
            monitors: vec![focused_monitor("DP-2")],
            ..CompositorState::default()
        };

        let source = scroll_source(&state, Some("eDP-1"), &compositor).unwrap();

        assert_eq!(source.id, "ddcutil:2");
    }

    #[test]
    fn scroll_source_falls_back_to_primary_source() {
        let state = display_state();

        let source = scroll_source(&state, Some("HDMI-A-1"), &CompositorState::default()).unwrap();

        assert_eq!(source.id, "backlight:intel_backlight");
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
