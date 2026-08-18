use std::process::Stdio;
use std::time::{Duration, Instant};

use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk::prelude::*};
use serde::Deserialize;
use tokio::process::Command as TokioCommand;

use crate::{
    panels::applets::AppletConfig,
    widgets::panel_indicator::{PanelIndicator, PanelMenu, PanelMenuItem},
};

/// Minimum gap between fired scroll commands. Leading-edge: the first notch
/// in a burst fires immediately; further notches within the window are
/// dropped instead of only ever firing the last one after scrolling stops.
const SCROLL_THROTTLE_MS: u64 = 100;

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub icon: Option<String>,
    pub label: Option<String>,
    pub tooltip: Option<String>,
    #[serde(alias = "left_click", alias = "command")]
    pub on_click: Vec<String>,
    #[serde(alias = "middle_click")]
    pub on_middle_click: Vec<String>,
    #[serde(alias = "scroll_up")]
    pub on_scroll_up: Vec<String>,
    #[serde(alias = "scroll_down")]
    pub on_scroll_down: Vec<String>,
    #[serde(alias = "h_scroll_left")]
    pub on_scroll_left: Vec<String>,
    #[serde(alias = "h_scroll_right")]
    pub on_scroll_right: Vec<String>,
    #[serde(alias = "right_click_menu")]
    pub menu: Vec<MenuItemConfig>,
}

impl Config {
    pub fn from_raw(name: &str, raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(applet = name, %error, "invalid command applet config, using defaults");
                Self::default()
            }
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct MenuItemConfig {
    #[serde(alias = "name")]
    pub label: String,
    pub command: Vec<String>,
}

pub struct Applet {
    name: String,
    config: Config,
    view: View,
    root: PanelIndicator,
    scroll_v_throttle: Option<Instant>,
    scroll_h_throttle: Option<Instant>,
}

#[derive(Debug)]
pub struct Init {
    pub name: String,
    pub config: Config,
}

#[derive(Debug, Clone)]
pub enum Input {
    Activate,
    MiddleClick,
    ScrollUp,
    ScrollDown,
    HScrollLeft,
    HScrollRight,
    MenuCommand(usize),
    Reconfigure(Option<AppletConfig>),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct View {
    visible: bool,
    has_named_icon: bool,
    icon_name: Option<String>,
    has_path_icon: bool,
    icon_path: Option<String>,
    has_label: bool,
    label: String,
    tooltip: Option<String>,
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
            set_tooltip_text: model.view.tooltip.as_deref(),
            #[watch]
            set_icon: model.view.icon_name.as_deref().or(model.view.icon_path.as_deref()),
            #[watch]
            set_label: if model.view.has_label { Some(model.view.label.as_str()) } else { None },
            connect_activated[sender] => move |_| {
                sender.input(Input::Activate);
            },
            connect_middle_clicked[sender] => move |_| {
                sender.input(Input::MiddleClick);
            },
            connect_scrolled[sender] => move |_, dx, dy| {
                if dy < 0.0 { sender.input(Input::ScrollUp); }
                else if dy > 0.0 { sender.input(Input::ScrollDown); }
                if dx < 0.0 { sender.input(Input::HScrollLeft); }
                else if dx > 0.0 { sender.input(Input::HScrollRight); }
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let view = view_from_config(&init.config);
        sync_context_menu(&root, &init.config, &sender);
        let model = Applet {
            name: init.name,
            config: init.config,
            view,
            root: root.clone(),
            scroll_v_throttle: None,
            scroll_h_throttle: None,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            Input::Activate => self.spawn_command(&self.config.on_click),
            Input::MiddleClick => self.spawn_command(&self.config.on_middle_click),
            Input::ScrollUp => self.throttle_scroll_v(&self.config.on_scroll_up.clone()),
            Input::ScrollDown => self.throttle_scroll_v(&self.config.on_scroll_down.clone()),
            Input::HScrollLeft => self.throttle_scroll_h(&self.config.on_scroll_left.clone()),
            Input::HScrollRight => self.throttle_scroll_h(&self.config.on_scroll_right.clone()),
            Input::MenuCommand(index) => {
                if let Some(item) = self.config.menu.get(index) {
                    self.spawn_command(&item.command);
                    self.root.popdown_context_menu();
                }
            }
            Input::Reconfigure(raw) => {
                let config = Config::from_raw(&self.name, &raw);
                if self.config == config {
                    return;
                }
                sync_context_menu(&self.root, &config, &sender);
                self.view = view_from_config(&config);
                self.config = config;
            }
        }
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.root.clear_context_menu();
    }
}

impl Applet {
    pub fn can_launch(config: &Config) -> bool {
        view_from_config(config).visible
    }

    fn throttle_scroll_v(&mut self, command: &[String]) {
        if command.is_empty() || !scroll_throttle_ready(&mut self.scroll_v_throttle) {
            return;
        }
        self.spawn_command(command);
    }

    fn throttle_scroll_h(&mut self, command: &[String]) {
        if command.is_empty() || !scroll_throttle_ready(&mut self.scroll_h_throttle) {
            return;
        }
        self.spawn_command(command);
    }

    fn spawn_command(&self, command: &[String]) {
        if command.is_empty() {
            return;
        }

        let name = self.name.clone();
        let command = command.to_vec();
        relm4::spawn(async move {
            if let Err(error) = run_command(&name, command).await {
                tracing::warn!(%error, applet = %name, "command applet command failed to start");
            }
        });
    }
}

/// Leading-edge throttle: fires (and returns true) if enough time has
/// passed since the last fire, updating `last` in that case. Otherwise
/// returns false and leaves `last` untouched.
fn scroll_throttle_ready(last: &mut Option<Instant>) -> bool {
    let now = Instant::now();
    let ready = match *last {
        Some(previous) => now.duration_since(previous) >= Duration::from_millis(SCROLL_THROTTLE_MS),
        None => true,
    };
    if ready {
        *last = Some(now);
    }
    ready
}

async fn run_command(applet: &str, command: Vec<String>) -> anyhow::Result<()> {
    let Some((program, args)) = command.split_first() else {
        return Ok(());
    };

    tracing::debug!(applet, %program, ?args, "command applet running command");

    let status = TokioCommand::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if !status.success() {
        tracing::warn!(applet, %program, ?args, ?status, "command applet command exited with failure");
    }

    Ok(())
}

fn view_from_config(config: &Config) -> View {
    let label = config.label.clone().unwrap_or_default();
    let icon = config.icon.as_deref().filter(|icon| !icon.is_empty());
    let (icon_name, icon_path) = icon
        .map(|icon| {
            if is_icon_path(icon) {
                (None, Some(icon.to_owned()))
            } else {
                (Some(icon.to_owned()), None)
            }
        })
        .unwrap_or((None, None));
    let has_icon = icon_name.is_some() || icon_path.is_some();
    let has_label = !label.is_empty();

    View {
        visible: has_icon || has_label,
        has_named_icon: icon_name.is_some(),
        icon_name,
        has_path_icon: icon_path.is_some(),
        icon_path,
        has_label,
        label,
        tooltip: config.tooltip.clone().filter(|tooltip| !tooltip.is_empty()),
    }
}

fn is_icon_path(icon: &str) -> bool {
    icon.starts_with('/') || icon.starts_with("./") || icon.starts_with("../") || icon.contains('/')
}

#[cfg(test)]
fn has_visible_menu_items(menu: &[MenuItemConfig]) -> bool {
    menu.iter()
        .any(|item| !item.label.is_empty() && !item.command.is_empty())
}

fn sync_context_menu(root: &PanelIndicator, config: &Config, sender: &ComponentSender<Applet>) {
    let items = config
        .menu
        .iter()
        .enumerate()
        .filter(|(_, item)| !item.label.is_empty() && !item.command.is_empty())
        .map(|(index, item)| PanelMenuItem::Action {
            label: item.label.clone(),
            input: Input::MenuCommand(index),
            enabled: true,
        })
        .collect::<Vec<_>>();

    if items.is_empty() {
        root.clear_context_menu();
        return;
    }

    root.set_context_menu(PanelMenu { items }, sender.input_sender().clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_accepts_empty_settings() {
        assert_eq!(Config::from_raw("test", &None), Config::default());
    }

    #[tokio::test]
    async fn run_command_returns_ok_when_child_exits_nonzero() {
        let result =
            run_command("test", vec!["/bin/sh".into(), "-c".into(), "exit 3".into()]).await;
        assert!(result.is_ok());
    }

    #[test]
    fn scroll_throttle_fires_on_first_call_then_blocks_until_window_elapses() {
        let mut last = None;
        assert!(scroll_throttle_ready(&mut last), "first notch always fires");
        assert!(
            !scroll_throttle_ready(&mut last),
            "an immediate second notch within the window must not fire"
        );

        // Simulate the window having elapsed by backdating the stored instant.
        last = Instant::now().checked_sub(Duration::from_millis(SCROLL_THROTTLE_MS));
        assert!(
            scroll_throttle_ready(&mut last),
            "a notch after the window elapses must fire"
        );
    }

    #[test]
    fn empty_command_config_does_not_launch() {
        assert!(!Applet::can_launch(&Config::default()));
    }

    #[test]
    fn icon_only_config_can_launch() {
        assert!(Applet::can_launch(&Config {
            icon: Some("camera-photo-symbolic".into()),
            ..Config::default()
        }));
    }

    #[test]
    fn label_only_config_can_launch() {
        assert!(Applet::can_launch(&Config {
            label: Some("Shot".into()),
            ..Config::default()
        }));
    }

    #[test]
    fn view_splits_icon_names_and_paths() {
        let named = view_from_config(&Config {
            icon: Some("camera-photo-symbolic".into()),
            ..Config::default()
        });
        assert!(named.has_named_icon);
        assert_eq!(named.icon_name.as_deref(), Some("camera-photo-symbolic"));
        assert!(!named.has_path_icon);
        assert_eq!(named.icon_path, None);

        let path = view_from_config(&Config {
            icon: Some("/tmp/icon.png".into()),
            ..Config::default()
        });
        assert!(!path.has_named_icon);
        assert_eq!(path.icon_name, None);
        assert!(path.has_path_icon);
        assert_eq!(path.icon_path.as_deref(), Some("/tmp/icon.png"));
    }

    #[test]
    fn menu_visibility_ignores_empty_items() {
        assert!(!has_visible_menu_items(&[MenuItemConfig {
            label: "Open".into(),
            command: Vec::new(),
        }]));
        assert!(has_visible_menu_items(&[MenuItemConfig {
            label: "Open".into(),
            command: vec!["true".into()],
        }]));
    }

    #[test]
    fn empty_tooltip_is_ignored() {
        let view = view_from_config(&Config {
            icon: Some("camera-photo-symbolic".into()),
            tooltip: Some(String::new()),
            ..Config::default()
        });

        assert_eq!(view.tooltip, None);
    }
}
