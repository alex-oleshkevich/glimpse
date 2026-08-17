use regex::Regex;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use crate::utils::subscribe_service;

use glimpse_core::ThemeMode;

use crate::{
    compositors::Window,
    panels::applets::AppletConfig,
    services::{
        compositor::{Command as CompositorCommand, CompositorHandle, State as CompositorState},
        framework::ServiceCommand,
        notifications::{
            NotificationsHandle,
            model::{Command, NotificationEntry, State},
        },
    },
    widgets::{panel_indicator::PanelIndicator, status_dot::StatusDotStatus},
};

use super::{
    activation, format,
    popover::{Popover, PopoverInit, PopoverInput, PopoverOutput},
    popup::{Popup, PopupInit, PopupInput, PopupPosition},
};

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BadgeStyle {
    None,
    Count,
    #[default]
    Dot,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UrgencyLevel {
    Low,
    Normal,
    Critical,
}

impl UrgencyLevel {
    fn as_u8(self) -> u8 {
        match self {
            Self::Low => 0,
            Self::Normal => 1,
            Self::Critical => 2,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UrgencyRemap {
    pub app_pattern: String,
    pub urgency: UrgencyLevel,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(alias = "label")]
    pub label_format: String,
    #[serde(alias = "tooltip")]
    pub tooltip_format: String,
    pub badge_style: BadgeStyle,
    pub popup_timeout_ms: u32,
    pub popup_visible_limit: usize,
    pub popup_position: PopupPosition,
    pub popup_margin_x: i32,
    pub popup_margin_y: i32,
    pub popup_monitor: Option<String>,
    pub max_history: usize,
    pub filter_regex: Vec<String>,
    pub urgency_remap: Vec<UrgencyRemap>,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "invalid notifications applet config, using defaults"
                );
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
            badge_style: BadgeStyle::default(),
            popup_timeout_ms: 5000,
            popup_visible_limit: 8,
            popup_position: PopupPosition::TopRight,
            popup_margin_x: 12,
            popup_margin_y: 12,
            popup_monitor: None,
            max_history: 100,
            filter_regex: Vec::new(),
            urgency_remap: Vec::new(),
        }
    }
}

pub struct Applet {
    config: Config,
    state: State,
    compositor_state: CompositorState,
    service: NotificationsHandle,
    compositor: CompositorHandle,
    icon_name: String,
    label: String,
    tooltip: String,
    badge_label: String,
    badge_visible: bool,
    badge_classes: Vec<&'static str>,
    popover: Controller<Popover>,
    popup: Option<Controller<Popup>>,
    subscription_cancel: CancellationToken,
    compositor_cancel: CancellationToken,
    theme_mode: ThemeMode,
    urgency_remaps: Vec<(Regex, u8)>,
}

pub struct Init {
    pub service: NotificationsHandle,
    pub compositor: CompositorHandle,
    pub config: Config,
    pub panel_monitor: Option<String>,
    pub theme_mode: ThemeMode,
}

#[derive(Debug)]
pub enum Input {
    ServiceStateChanged(State),
    CompositorStateChanged(CompositorState),
    Reconfigure {
        config: Config,
        theme_mode: ThemeMode,
    },
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
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },
            #[watch]
            set_icon: Some(model.icon_name.as_str()),
            #[watch]
            set_label: if model.label.is_empty() { None } else { Some(model.label.as_str()) },
            #[watch]
            set_extra_visible: model.badge_visible,
            connect_activated[sender] => move |_| {
                sender.input(Input::TogglePopover);
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_valign: gtk::Align::Center,
                set_halign: gtk::Align::Center,
                #[watch]
                set_css_classes: &model.badge_classes,
                #[watch]
                set_visible: model.badge_visible,

                gtk::Label {
                    set_valign: gtk::Align::Center,
                    set_halign: gtk::Align::Center,
                    #[watch]
                    set_label: &model.badge_label,
                    #[watch]
                    set_visible: model.badge_style_uses_label(),
                }
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
        let owns_popup = applet_owns_popup(
            init.config.popup_monitor.as_deref(),
            init.panel_monitor.as_deref(),
        );
        let popup = if owns_popup {
            tracing::info!(
                panel_monitor = ?init.panel_monitor,
                popup_monitor = ?init.config.popup_monitor,
                "creating notifications popup window"
            );
            Some(
                Popup::builder()
                    .launch(PopupInit {
                        timeout_ms: init.config.popup_timeout_ms,
                        visible_limit: init.config.popup_visible_limit,
                        position: init.config.popup_position,
                        margin_x: init.config.popup_margin_x,
                        margin_y: init.config.popup_margin_y,
                        popup_monitor: init.config.popup_monitor.clone(),
                        theme_mode: init.theme_mode,
                    })
                    .forward(sender.input_sender(), Input::PopoverOutput),
            )
        } else {
            tracing::info!(
                panel_monitor = ?init.panel_monitor,
                popup_monitor = ?init.config.popup_monitor,
                "skipping notifications popup window (another panel owns it)"
            );
            None
        };

        let state = init.service.snapshot();
        let compositor_state = init.compositor.snapshot();
        let urgency_remaps = compile_urgency_remaps(&init.config.urgency_remap);
        let subscription_cancel = subscribe_service(
            init.service.subscribe(),
            sender.input_sender().clone(),
            Input::ServiceStateChanged,
        );
        let compositor_cancel = subscribe_service(
            init.compositor.subscribe(),
            sender.input_sender().clone(),
            Input::CompositorStateChanged,
        );
        let mut model = Applet {
            icon_name: format::icon_name(&state).into(),
            label: format::label(&init.config.label_format, &state),
            tooltip: format::tooltip(&init.config.tooltip_format, &state),
            badge_label: String::new(),
            badge_visible: false,
            badge_classes: Vec::new(),
            config: init.config,
            state,
            compositor_state,
            service: init.service,
            compositor: init.compositor,
            popover,
            popup,
            subscription_cancel,
            compositor_cancel,
            theme_mode: init.theme_mode,
            urgency_remaps,
        };
        model.apply_state(model.state.clone());
        model.send_notification(Command::SetMaxHistory(model.config.max_history));
        model.send_notification(Command::SetFilterRegex(model.config.filter_regex.clone()));

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::ServiceStateChanged(state) => self.apply_state(state),
            Input::CompositorStateChanged(state) => self.compositor_state = state,
            Input::Reconfigure { config, theme_mode } => {
                self.config = config;
                self.theme_mode = theme_mode;
                self.send_notification(Command::SetMaxHistory(self.config.max_history));
                self.send_notification(Command::SetFilterRegex(self.config.filter_regex.clone()));
                self.apply_state(self.service.snapshot());
                if let Some(popup) = &self.popup {
                    popup.emit(PopupInput::Reconfigure {
                        timeout_ms: self.config.popup_timeout_ms,
                        visible_limit: self.config.popup_visible_limit,
                        position: self.config.popup_position,
                        margin_x: self.config.popup_margin_x,
                        margin_y: self.config.popup_margin_y,
                        popup_monitor: self.config.popup_monitor.clone(),
                        theme_mode,
                    });
                }
            }
            Input::TogglePopover => {
                self.sync_popover(&self.state.clone());
                self.popover.emit(PopoverInput::Toggle);
            }
            Input::PopoverOutput(output) => self.handle_output(output),
        }
    }
}

impl Applet {
    fn apply_state(&mut self, mut state: State) {
        for notification in &mut state.notifications {
            if let Some(level) = self.urgency_for(&notification.app_name) {
                notification.urgency = level;
            }
        }
        self.icon_name = format::icon_name(&state).into();
        self.label = format::label(&self.config.label_format, &state);
        self.tooltip = format::tooltip(&self.config.tooltip_format, &state);
        self.sync_badge(&state.notifications, state.dnd);
        if self.popover.widget().is_visible() {
            self.sync_popover(&state);
        }
        if let Some(popup) = &self.popup {
            popup.emit(PopupInput::Update {
                notifications: state.notifications.clone(),
                dnd: state.dnd,
            });
        }
        self.state = state;
    }

    /// Find the first remap rule whose pattern matches `app_name`; returns
    /// the urgency u8 to apply.
    fn urgency_for(&self, app_name: &str) -> Option<u8> {
        self.urgency_remaps
            .iter()
            .find(|(rx, _)| rx.is_match(app_name))
            .map(|(_, level)| *level)
    }

    fn sync_badge(&mut self, notifications: &[NotificationEntry], dnd: bool) {
        let count = notifications.len();
        self.badge_visible = count > 0 && !dnd && self.config.badge_style != BadgeStyle::None;
        self.badge_label = if count > 9 {
            "9+".into()
        } else {
            count.to_string()
        };
        self.badge_classes = match self.config.badge_style {
            BadgeStyle::Count => vec!["notification-badge-anchor", "badge", "is-accent"],
            BadgeStyle::Dot | BadgeStyle::None => {
                let status = if notifications.iter().any(|n| n.urgency == 2) {
                    StatusDotStatus::Warning
                } else {
                    StatusDotStatus::Neutral
                };
                vec![
                    "notification-badge-anchor",
                    "status-dot",
                    status.css_class(),
                ]
            }
        };
    }

    fn badge_style_uses_label(&self) -> bool {
        matches!(self.config.badge_style, BadgeStyle::Count)
    }

    fn sync_popover(&self, state: &State) {
        self.popover.emit(PopoverInput::Update {
            notifications: state.notifications.clone(),
            dnd: state.dnd,
        });
    }

    fn handle_output(&mut self, output: PopoverOutput) {
        match output {
            PopoverOutput::Dismiss(id) => self.send_notification(Command::Dismiss { id }),
            PopoverOutput::DismissGroup(ids) => self.dismiss_notifications(ids),
            PopoverOutput::DismissAll => self.send_notification(Command::DismissAll),
            PopoverOutput::SetDnd(enabled) => self.send_notification(Command::SetDnd(enabled)),
            PopoverOutput::FocusAndDismiss(id) => self.focus_and_dismiss_notification(id),
            PopoverOutput::InvokeAction { id, action_key } => {
                self.invoke_action_and_dismiss(id, action_key);
            }
        }
    }

    fn focus_and_dismiss_notification(&self, id: u32) {
        let Some(notification) = self.state.notifications.iter().find(|item| item.id == id) else {
            tracing::debug!(id, "notification disappeared before focus and dismiss");
            self.send_notification(Command::Dismiss { id });
            return;
        };

        let focus_window = self.resolve_focus_window(notification);
        let compositor = self.compositor.clone();
        let service = self.service.clone();
        relm4::spawn(async move {
            if let Some(window) = focus_window {
                send_compositor_command(&compositor, CompositorCommand::FocusWindow(window)).await;
            }
            send_notification_command(&service, Command::Dismiss { id }).await;
        });
    }

    fn invoke_action_and_dismiss(&self, id: u32, action_key: String) {
        let notification = self.state.notifications.iter().find(|item| item.id == id);
        let activation_token = notification.and_then(|notification| {
            activation::startup_notify_token(
                notification.desktop_entry.as_deref(),
                gtk::gdk::CURRENT_TIME,
            )
        });
        let mut focus_window = None;
        if activation_token.is_none() {
            if let Some(notification) = notification {
                focus_window = self.resolve_focus_window(notification);
            } else {
                tracing::debug!(id, "notification disappeared before action activation");
            }
        }

        let service = self.service.clone();
        let compositor = self.compositor.clone();
        relm4::spawn(async move {
            if let Some(window) = focus_window {
                send_compositor_command(&compositor, CompositorCommand::FocusWindow(window)).await;
            }
            let invoke = Command::InvokeAction {
                id,
                action_key,
                activation_token,
            };
            if send_notification_command(&service, invoke).await {
                send_notification_command(&service, Command::Dismiss { id }).await;
            }
        });
    }

    fn resolve_focus_window(&self, notification: &NotificationEntry) -> Option<usize> {
        let window = matching_window(&self.compositor_state, notification);
        if window.is_none() {
            tracing::debug!(
                id = notification.id,
                app_name = %notification.app_name,
                desktop_entry = ?notification.desktop_entry,
                "could not detect notification source window to focus"
            );
        }
        window
    }

    fn send_notification(&self, command: Command) {
        let service = self.service.clone();
        relm4::spawn(async move {
            send_notification_command(&service, command).await;
        });
    }

    fn dismiss_notifications(&self, ids: Vec<u32>) {
        let service = self.service.clone();
        relm4::spawn(async move {
            for id in ids {
                if !send_notification_command(&service, Command::Dismiss { id }).await {
                    break;
                }
            }
        });
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
        self.compositor_cancel.cancel();
    }
}

/// Decide whether this applet instance should own the singleton popup window.
///
/// With multiple panels (one per monitor), every notifications applet would otherwise spin up its
/// own popup window — they all stack at the same anchor and you end up needing one click per
/// monitor to dismiss what looks like a single popup. Restrict popup ownership to a single
/// connector to avoid that.
///
/// - `popup_monitor = Some(name)`: own iff this applet's panel sits on `name`. Falls back to no
///   popup at all if the configured monitor is not currently connected (matches the "fall back to
///   compositor placement when missing" UX decision — except in this case, with no applet on that
///   connector, there is no popup until the monitor returns and panels reconcile).
/// - `popup_monitor = None`: own iff this applet's panel sits on the alphabetically-first
///   currently-connected connector. Deterministic single-popup default, no extra config required.
/// - `panel_monitor = None`: never own, because an unbound applet cannot prove it is the singleton
///   popup owner.
/// Compile the list of urgency-remap rules into `(regex, urgency u8)` pairs.
/// Invalid regexes are skipped with a warning so a single bad rule doesn't
/// disable the whole feature.
fn compile_urgency_remaps(rules: &[UrgencyRemap]) -> Vec<(Regex, u8)> {
    rules
        .iter()
        .filter_map(|rule| match Regex::new(&rule.app_pattern) {
            Ok(rx) => Some((rx, rule.urgency.as_u8())),
            Err(error) => {
                tracing::warn!(
                    pattern = rule.app_pattern,
                    %error,
                    "invalid urgency_remap regex; skipping rule"
                );
                None
            }
        })
        .collect()
}

fn applet_owns_popup(popup_monitor: Option<&str>, panel_monitor: Option<&str>) -> bool {
    match (popup_monitor, panel_monitor) {
        (Some(target), Some(panel)) => target == panel,
        (Some(_), None) => false,
        (None, Some(panel)) => primary_connector().as_deref() == Some(panel),
        (None, None) => false,
    }
}

fn primary_connector() -> Option<String> {
    let display = gtk::gdk::Display::default()?;
    let monitors = display.monitors();
    let mut connectors = (0..monitors.n_items())
        .filter_map(|index| monitors.item(index))
        .filter_map(|object| object.downcast::<gtk::gdk::Monitor>().ok())
        .filter_map(|monitor| monitor.connector().map(|s| s.to_string()))
        .collect::<Vec<_>>();
    connectors.sort();
    connectors.into_iter().next()
}

async fn send_notification_command(service: &NotificationsHandle, command: Command) -> bool {
    match service.send(ServiceCommand::Command(command)).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(%error, "failed to send notifications command");
            false
        }
    }
}

async fn send_compositor_command(
    compositor: &CompositorHandle,
    command: CompositorCommand,
) -> bool {
    match compositor.send(ServiceCommand::Command(command)).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(%error, "failed to send compositor command");
            false
        }
    }
}

fn matching_window(state: &CompositorState, notification: &NotificationEntry) -> Option<usize> {
    let keys = notification_keys(notification);
    if keys.is_empty() {
        return None;
    }

    state
        .windows
        .iter()
        .find(|window| window_matches(window, &keys))
        .map(|window| window.id)
}

fn notification_keys(notification: &NotificationEntry) -> Vec<String> {
    [
        notification.desktop_entry.as_deref(),
        Some(&notification.app_name),
    ]
    .into_iter()
    .flatten()
    .filter_map(normalize_app_key)
    .collect()
}

fn normalize_app_key(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    Some(
        value
            .strip_suffix(".desktop")
            .unwrap_or(value)
            .to_ascii_lowercase(),
    )
}

fn window_matches(window: &Window, keys: &[String]) -> bool {
    let Some(app_id) = window.app_id.as_deref().and_then(normalize_app_key) else {
        return false;
    };

    keys.iter().any(|key| key == &app_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_matching_ignores_desktop_suffix_and_case() {
        let mut state = CompositorState::default();
        state.windows.push(Window {
            id: 7,
            title: None,
            app_id: Some("org.mozilla.firefox".into()),
            pid: None,
            layout_order: None,
            workspace: None,
            focused: false,
            urgent: false,
            fullscreen: false,
            floating: None,
        });
        let notification = NotificationEntry {
            id: 1,
            app_name: "Firefox".into(),
            app_icon: String::new(),
            desktop_entry: Some("Org.Mozilla.Firefox.desktop".into()),
            summary: String::new(),
            body: String::new(),
            urgency: 1,
            actions: Vec::new(),
            image: None,
            timestamp: 0,
            resident: false,
        };

        assert_eq!(matching_window(&state, &notification), Some(7));
    }

    #[test]
    fn config_accepts_bottom_right_popup_position() {
        let raw = AppletConfig {
            extends: None,
            settings: toml::toml! {
                popup_position = "bottom_right"
                popup_margin_x = 24
                popup_margin_y = 40
                popup_visible_limit = 6
            }
            .into(),
        };

        let config = Config::from_raw(&Some(raw));

        assert_eq!(config.popup_position, PopupPosition::BottomRight);
        assert_eq!(config.popup_margin_x, 24);
        assert_eq!(config.popup_margin_y, 40);
        assert_eq!(config.popup_visible_limit, 6);
    }

    #[test]
    fn config_defaults_popup_position_to_top_right() {
        assert_eq!(Config::default().popup_position, PopupPosition::TopRight);
    }

    #[test]
    fn config_defaults_popup_visible_limit_to_eight() {
        assert_eq!(Config::default().popup_visible_limit, 8);
    }

    #[test]
    fn config_defaults_popup_monitor_to_none() {
        assert_eq!(Config::default().popup_monitor, None);
    }

    #[test]
    fn config_accepts_popup_monitor_connector() {
        let raw = AppletConfig {
            extends: None,
            settings: toml::toml! {
                popup_monitor = "DP-2"
            }
            .into(),
        };

        let config = Config::from_raw(&Some(raw));

        assert_eq!(config.popup_monitor.as_deref(), Some("DP-2"));
    }

    #[test]
    fn config_accepts_notification_filter_regex_rules() {
        let raw = AppletConfig {
            extends: None,
            settings: toml::toml! {
                filter_regex = ["(?i)^discord$", "build succeeded"]
            }
            .into(),
        };

        let config = Config::from_raw(&Some(raw));

        assert_eq!(
            config.filter_regex,
            vec!["(?i)^discord$".to_string(), "build succeeded".to_string()]
        );
    }

    #[test]
    fn config_defaults_filter_regex_to_empty_list() {
        assert!(Config::default().filter_regex.is_empty());
    }

    #[test]
    fn applet_owns_popup_pinned_to_matching_panel() {
        assert!(applet_owns_popup(Some("eDP-1"), Some("eDP-1")));
    }

    #[test]
    fn applet_owns_popup_rejects_panel_on_other_monitor_when_pinned() {
        assert!(!applet_owns_popup(Some("eDP-1"), Some("DP-2")));
    }

    #[test]
    fn applet_owns_popup_rejects_unbound_panel_when_pinned() {
        // With popup_monitor explicitly set, panels with no monitor binding can never own
        // the popup — otherwise a stray applet would steal it from the configured target.
        assert!(!applet_owns_popup(Some("eDP-1"), None));
    }

    #[test]
    fn applet_owns_popup_rejects_unbound_panel_when_unpinned() {
        // Unbound panels cannot safely choose a singleton popup owner on their own.
        assert!(!applet_owns_popup(None, None));
    }
}
