use gio::prelude::AppInfoExt;
use gio_unix::DesktopAppInfo;
use glimpse_core::compositors::Window;
use glimpse_core::services::compositor::{CompositorHandle, State};
use relm4::gtk::glib::object::Cast;
use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk::prelude::*};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::panels::applets::AppletConfig;
use crate::widgets::panel_indicator::PanelIndicator;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub label_format: String,
    pub icon: Option<String>,
    pub max_chars: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            label_format: "{title}".into(),
            icon: None,
            max_chars: 80,
        }
    }
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };
        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid window applet config, using defaults");
                Self::default()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct View {
    visible: bool,
    label: String,
    icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowState {
    windows_available: bool,
    focused_window: Option<usize>,
    windows: Vec<WindowInfo>,
}

impl From<&State> for WindowState {
    fn from(state: &State) -> Self {
        Self {
            windows_available: state.capabilities.windows,
            focused_window: state.focused_window,
            windows: state.windows.iter().map(WindowInfo::from).collect(),
        }
    }
}

fn view_from_state(config: &Config, state: &WindowState) -> View {
    if !state.windows_available {
        return View {
            visible: false,
            label: String::new(),
            icon: None,
        };
    }
    let focused = state
        .focused_window
        .and_then(|id| state.windows.iter().find(|w| w.id == id));
    let Some(window) = focused else {
        return View {
            visible: false,
            label: String::new(),
            icon: None,
        };
    };
    let label = format_window_label(&config.label_format, window, config.max_chars);
    let icon = resolve_icon(config.icon.as_deref(), window.app_id.as_deref());
    View {
        visible: true,
        label,
        icon,
    }
}

pub struct Applet {
    config: Config,
    state: WindowState,
    view: View,
    service: CompositorHandle,
    subscription_cancel: CancellationToken,
}

pub struct Init {
    pub service: CompositorHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    ServiceStateChanged(State),
    Reconfigure(Config),
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = PanelIndicator {
            #[watch]
            set_visible: model.view.visible,
            #[watch]
            set_label: if model.view.label.is_empty() {
                None
            } else {
                Some(model.view.label.as_str())
            },
            #[watch]
            set_icon: model.view.icon.as_deref(),
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let state = WindowState::from(&init.service.snapshot());
        let view = view_from_state(&init.config, &state);
        let model = Applet {
            config: init.config,
            state,
            view,
            service: init.service,
            subscription_cancel: CancellationToken::new(),
        };

        let service = model.service.clone();
        let cancel = model.subscription_cancel.clone();
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
                        if changed.is_err() { break; }
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

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::ServiceStateChanged(state) => {
                let state = WindowState::from(&state);
                if self.state != state {
                    self.state = state;
                    self.sync_view();
                }
            }
            Input::Reconfigure(config) => {
                self.config = config;
                self.sync_view();
            }
        }
    }
}

impl Applet {
    fn sync_view(&mut self) {
        let view = view_from_state(&self.config, &self.state);
        if self.view != view {
            self.view = view;
        }
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub id: usize,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub layout_order: Option<usize>,
}

impl From<&Window> for WindowInfo {
    fn from(w: &Window) -> Self {
        Self {
            id: w.id,
            title: w.title.clone(),
            app_id: w.app_id.clone(),
            layout_order: w.layout_order,
        }
    }
}

pub fn truncate_title(title: &str, max_chars: usize) -> String {
    if title.chars().count() <= max_chars {
        title.to_string()
    } else {
        let truncated: String = title.chars().take(max_chars).collect();
        format!("{truncated}\u{2026}")
    }
}

pub fn format_window_label(format: &str, window: &WindowInfo, max_chars: usize) -> String {
    let title = truncate_title(window.title.as_deref().unwrap_or(""), max_chars);
    let app_id = window.app_id.as_deref().unwrap_or("").to_string();
    let id = window.id.to_string();
    let index = window
        .layout_order
        .map(|o| (o + 1).to_string())
        .unwrap_or_default();

    format
        .replace("{title}", &title)
        .replace("{app_id}", &app_id)
        .replace("{id}", &id)
        .replace("{index}", &index)
}

pub fn resolve_icon(icon_config: Option<&str>, app_id: Option<&str>) -> Option<String> {
    match icon_config {
        None => None,
        Some("app") => {
            let app_id = app_id?;
            let candidates = [app_id.to_string(), format!("{app_id}.desktop")];
            for candidate in &candidates {
                if let Some(info) = DesktopAppInfo::new(candidate) {
                    if let Some(icon) = info.icon() {
                        if let Ok(themed) = icon.downcast::<gio::ThemedIcon>() {
                            if let Some(name) = themed.names().first() {
                                return Some(name.to_string());
                            }
                        }
                    }
                }
            }
            None
        }
        Some(other) => Some(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(
        id: usize,
        title: Option<&str>,
        app_id: Option<&str>,
        layout_order: Option<usize>,
    ) -> WindowInfo {
        WindowInfo {
            id,
            title: title.map(str::to_owned),
            app_id: app_id.map(str::to_owned),
            layout_order,
        }
    }

    #[test]
    fn truncate_title_leaves_short_titles_unchanged() {
        assert_eq!(truncate_title("hello", 80), "hello");
        assert_eq!(truncate_title("hello", 5), "hello");
    }

    #[test]
    fn truncate_title_adds_ellipsis_when_over_limit() {
        assert_eq!(truncate_title("hello world", 5), "hello\u{2026}");
    }

    #[test]
    fn truncate_title_counts_chars_not_bytes() {
        let s = "héllo";
        assert_eq!(truncate_title(s, 5), "héllo");
        assert_eq!(truncate_title(s, 4), "héll\u{2026}");
    }

    #[test]
    fn format_window_label_expands_title_token() {
        let w = window(1, Some("Firefox"), None, None);
        assert_eq!(format_window_label("{title}", &w, 80), "Firefox");
    }

    #[test]
    fn format_window_label_truncates_title_token() {
        let w = window(1, Some("Very Long Title"), None, None);
        assert_eq!(format_window_label("{title}", &w, 4), "Very\u{2026}");
    }

    #[test]
    fn format_window_label_expands_all_tokens() {
        let w = window(42, Some("Title"), Some("firefox"), Some(1));
        let result = format_window_label("{id} {index} {app_id} {title}", &w, 80);
        assert_eq!(result, "42 2 firefox Title");
    }

    #[test]
    fn format_window_label_uses_empty_string_for_absent_fields() {
        let w = window(1, None, None, None);
        assert_eq!(format_window_label("{title}{app_id}", &w, 80), "");
    }

    #[test]
    fn format_window_label_uses_empty_string_for_absent_layout_order() {
        let w = window(1, None, None, None);
        assert_eq!(format_window_label("{index}", &w, 80), "");
    }

    #[test]
    fn resolve_icon_returns_none_when_config_absent() {
        assert_eq!(resolve_icon(None, Some("firefox")), None);
    }

    #[test]
    fn resolve_icon_returns_literal_name_for_non_app_value() {
        assert_eq!(
            resolve_icon(Some("terminal"), Some("kitty")),
            Some("terminal".to_string())
        );
    }

    #[test]
    fn resolve_icon_returns_none_for_app_mode_with_no_app_id() {
        assert_eq!(resolve_icon(Some("app"), None), None);
    }

    fn state(windows: Vec<WindowInfo>, focused: Option<usize>) -> WindowState {
        WindowState {
            windows_available: true,
            focused_window: focused,
            windows,
        }
    }

    #[test]
    fn view_hidden_when_windows_unavailable() {
        let s = WindowState { windows_available: false, focused_window: None, windows: vec![] };
        let v = view_from_state(&Config::default(), &s);
        assert!(!v.visible);
    }

    #[test]
    fn view_hidden_when_no_focused_window() {
        let s = state(vec![window(1, Some("Firefox"), None, None)], None);
        let v = view_from_state(&Config::default(), &s);
        assert!(!v.visible);
    }

    #[test]
    fn view_hidden_when_focused_id_not_in_windows_list() {
        let s = state(vec![window(1, Some("Firefox"), None, None)], Some(99));
        let v = view_from_state(&Config::default(), &s);
        assert!(!v.visible);
    }

    #[test]
    fn view_shows_focused_window_title() {
        let s = state(vec![window(1, Some("Firefox"), None, None)], Some(1));
        let v = view_from_state(&Config::default(), &s);
        assert!(v.visible);
        assert_eq!(v.label, "Firefox");
    }
}
