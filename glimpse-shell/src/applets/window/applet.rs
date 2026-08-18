use gio::prelude::AppInfoExt;
use gio_unix::DesktopAppInfo;
use glimpse_core::compositors::Window;
use glimpse_core::services::compositor::{CompositorHandle, State};
use relm4::gtk::glib::object::Cast;
use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk::prelude::*};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::panels::applets::AppletConfig;
use crate::utils::subscribe_service;
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
        let result: Result<Self, _> = raw.settings.clone().try_into();
        match result {
            Ok(mut config) => {
                config.max_chars = config.max_chars.clamp(1, 200);
                config
            }
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
    /// The untruncated title, set only when `label` actually truncated it —
    /// otherwise the full title is unrecoverable from the panel.
    tooltip: Option<String>,
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

fn view_from_state(
    config: &Config,
    state: &WindowState,
    icon_cache: &mut std::collections::HashMap<String, Option<String>>,
) -> View {
    if !state.windows_available {
        return View {
            visible: false,
            label: String::new(),
            icon: None,
            tooltip: None,
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
            tooltip: None,
        };
    };
    let label = format_window_label(&config.label_format, window, config.max_chars);
    let icon = resolve_icon_cached(icon_cache, config.icon.as_deref(), window.app_id.as_deref());
    let tooltip = window
        .title
        .as_deref()
        .and_then(|title| (title.chars().count() > config.max_chars).then(|| title.to_owned()));
    View {
        visible: true,
        label,
        icon,
        tooltip,
    }
}

pub struct Applet {
    config: Config,
    state: WindowState,
    view: View,
    icon_cache: std::collections::HashMap<String, Option<String>>,
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
            #[watch]
            set_tooltip_text: model.view.tooltip.as_deref(),
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let state = WindowState::from(&init.service.snapshot());
        let mut icon_cache = std::collections::HashMap::new();
        let view = view_from_state(&init.config, &state, &mut icon_cache);
        let subscription_cancel = subscribe_service(
            init.service.subscribe(),
            sender.input_sender().clone(),
            Input::ServiceStateChanged,
        );
        let model = Applet {
            config: init.config,
            state,
            view,
            icon_cache,
            subscription_cancel,
        };

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
        let view = view_from_state(&self.config, &self.state, &mut self.icon_cache);
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

/// Memoizes resolve_icon's desktop-file DB lookup by app_id, since
/// view_from_state runs on every compositor state change (including every
/// title keystroke) and the lookup is a synchronous main-thread disk read.
/// Safe without cache invalidation: the mapping from app_id to icon name
/// doesn't depend on any other config field.
fn resolve_icon_cached(
    cache: &mut std::collections::HashMap<String, Option<String>>,
    icon_config: Option<&str>,
    app_id: Option<&str>,
) -> Option<String> {
    if icon_config != Some("app") {
        return resolve_icon(icon_config, app_id);
    }
    let app_id = app_id?;
    if let Some(cached) = cache.get(app_id) {
        return cached.clone();
    }
    let resolved = resolve_icon(icon_config, Some(app_id));
    cache.insert(app_id.to_owned(), resolved.clone());
    resolved
}

pub fn resolve_icon(icon_config: Option<&str>, app_id: Option<&str>) -> Option<String> {
    match icon_config {
        None => None,
        Some("app") => {
            let app_id = app_id?;
            // DesktopAppInfo::new expects a desktop file id (basename incl.
            // ".desktop"), never a bare app_id — a candidate for the bare
            // string can never match, so it's not worth trying.
            if let Some(icon) = icon_from_desktop_id(&format!("{app_id}.desktop")) {
                return Some(icon);
            }
            // app_id doesn't match the desktop file's basename (common with
            // Flatpak/Snap-style reverse-DNS ids); fall back to a fuzzy
            // search and use the best match.
            let best_match = DesktopAppInfo::search(app_id)
                .into_iter()
                .next()
                .and_then(|group| group.into_iter().next())?;
            icon_from_desktop_id(&best_match)
        }
        Some(other) => Some(other.to_string()),
    }
}

fn icon_from_desktop_id(desktop_id: &str) -> Option<String> {
    let info = DesktopAppInfo::new(desktop_id)?;
    let icon = info.icon()?;
    let themed = icon.downcast::<gio::ThemedIcon>().ok()?;
    themed.names().first().map(|name| name.to_string())
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
    fn from_raw_clamps_max_chars_to_a_sane_range() {
        let mut table = toml::map::Map::new();
        table.insert("max_chars".into(), toml::Value::Integer(0));
        let raw = Some(AppletConfig {
            extends: None,
            settings: toml::Value::Table(table),
        });
        assert_eq!(Config::from_raw(&raw).max_chars, 1);

        let mut table = toml::map::Map::new();
        table.insert("max_chars".into(), toml::Value::Integer(999));
        let raw = Some(AppletConfig {
            extends: None,
            settings: toml::Value::Table(table),
        });
        assert_eq!(Config::from_raw(&raw).max_chars, 200);
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
    fn resolve_icon_cached_returns_cached_value_without_relookup() {
        let mut cache = std::collections::HashMap::new();
        cache.insert(
            "definitely-not-a-real-app-id".to_string(),
            Some("cached-icon-name".to_string()),
        );
        assert_eq!(
            resolve_icon_cached(
                &mut cache,
                Some("app"),
                Some("definitely-not-a-real-app-id")
            ),
            Some("cached-icon-name".to_string())
        );
    }

    #[test]
    fn resolve_icon_cached_populates_cache_after_lookup() {
        let mut cache = std::collections::HashMap::new();
        resolve_icon_cached(&mut cache, Some("app"), Some("some-app-id"));
        assert!(cache.contains_key("some-app-id"));
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
        let s = WindowState {
            windows_available: false,
            focused_window: None,
            windows: vec![],
        };
        let v = view_from_state(
            &Config::default(),
            &s,
            &mut std::collections::HashMap::new(),
        );
        assert!(!v.visible);
    }

    #[test]
    fn view_hidden_when_no_focused_window() {
        let s = state(vec![window(1, Some("Firefox"), None, None)], None);
        let v = view_from_state(
            &Config::default(),
            &s,
            &mut std::collections::HashMap::new(),
        );
        assert!(!v.visible);
    }

    #[test]
    fn view_hidden_when_focused_id_not_in_windows_list() {
        let s = state(vec![window(1, Some("Firefox"), None, None)], Some(99));
        let v = view_from_state(
            &Config::default(),
            &s,
            &mut std::collections::HashMap::new(),
        );
        assert!(!v.visible);
    }

    #[test]
    fn view_shows_focused_window_title() {
        let s = state(vec![window(1, Some("Firefox"), None, None)], Some(1));
        let v = view_from_state(
            &Config::default(),
            &s,
            &mut std::collections::HashMap::new(),
        );
        assert!(v.visible);
        assert_eq!(v.label, "Firefox");
    }

    #[test]
    fn view_sets_tooltip_to_untruncated_title_when_truncated() {
        let config = Config {
            max_chars: 5,
            ..Config::default()
        };
        let s = state(
            vec![window(1, Some("A Very Long Window Title"), None, None)],
            Some(1),
        );
        let v = view_from_state(&config, &s, &mut std::collections::HashMap::new());
        assert_eq!(v.tooltip.as_deref(), Some("A Very Long Window Title"));
    }

    #[test]
    fn view_has_no_tooltip_when_title_is_not_truncated() {
        let s = state(vec![window(1, Some("Firefox"), None, None)], Some(1));
        let v = view_from_state(
            &Config::default(),
            &s,
            &mut std::collections::HashMap::new(),
        );
        assert_eq!(v.tooltip, None);
    }
}
