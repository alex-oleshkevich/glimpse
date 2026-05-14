use std::process::Stdio;
use std::time::Duration;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, gio, prelude::*},
};
use serde::Deserialize;
use tokio::process::Command as TokioCommand;

use crate::panels::applets::AppletConfig;

const SCROLL_DEBOUNCE_MS: u64 = 100;

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
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid command applet config, using defaults");
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
    root: gtk::Box,
    context_menu: gtk::PopoverMenu,
    scroll_v: Option<tokio::task::JoinHandle<()>>,
    scroll_h: Option<tokio::task::JoinHandle<()>>,
}

#[derive(Debug)]
pub struct Init {
    pub name: String,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    Activate,
    MiddleClick,
    ScrollUp,
    ScrollDown,
    HScrollLeft,
    HScrollRight,
    MenuCommand(usize),
    ShowContextMenu,
    Reconfigure(Config),
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
        root = gtk::Box {
            add_css_class: "applet",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            set_valign: gtk::Align::Center,
            #[watch]
            set_visible: model.view.visible,
            #[watch]
            set_tooltip_text: model.view.tooltip.as_deref(),

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |gesture, _, _, _| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    sender.input(Input::Activate);
                },
            },

            add_controller = gtk::GestureClick {
                set_button: 2,
                connect_pressed[sender] => move |gesture, _, _, _| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    sender.input(Input::MiddleClick);
                },
            },

            add_controller = gtk::GestureClick {
                set_button: 3,
                connect_pressed[sender] => move |gesture, _, _, _| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    sender.input(Input::ShowContextMenu);
                },
            },

            add_controller = gtk::EventControllerScroll {
                set_flags: gtk::EventControllerScrollFlags::BOTH_AXES,
                connect_scroll[sender] => move |_, dx, dy| {
                    let mut consumed = false;
                    if dy < 0.0 { sender.input(Input::ScrollUp); consumed = true; }
                    else if dy > 0.0 { sender.input(Input::ScrollDown); consumed = true; }
                    if dx < 0.0 { sender.input(Input::HScrollLeft); consumed = true; }
                    else if dx > 0.0 { sender.input(Input::HScrollRight); consumed = true; }
                    if consumed { gtk::glib::Propagation::Stop } else { gtk::glib::Propagation::Proceed }
                },
            },

            #[name = "named_icon"]
            gtk::Image {
                #[watch]
                set_visible: model.view.has_named_icon,
                #[watch]
                set_icon_name: model.view.icon_name.as_deref(),
                set_pixel_size: 16,
            },

            #[name = "path_icon"]
            gtk::Image {
                #[watch]
                set_visible: model.view.has_path_icon,
                #[watch]
                set_from_file: model.view.icon_path.as_deref(),
                set_pixel_size: 16,
            },

            #[name = "label"]
            gtk::Label {
                #[watch]
                set_visible: model.view.has_label,
                #[watch]
                set_label: &model.view.label,
                set_valign: gtk::Align::Center,
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let view = view_from_config(&init.config);
        let context_menu = build_context_menu(&root, &init.config, &sender);
        let model = Applet {
            name: init.name,
            config: init.config,
            view,
            root: root.clone(),
            context_menu,
            scroll_v: None,
            scroll_h: None,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            Input::Activate => self.spawn_command(&self.config.on_click),
            Input::MiddleClick => self.spawn_command(&self.config.on_middle_click),
            Input::ScrollUp => self.debounce_scroll_v(&self.config.on_scroll_up.clone()),
            Input::ScrollDown => self.debounce_scroll_v(&self.config.on_scroll_down.clone()),
            Input::HScrollLeft => self.debounce_scroll_h(&self.config.on_scroll_left.clone()),
            Input::HScrollRight => self.debounce_scroll_h(&self.config.on_scroll_right.clone()),
            Input::MenuCommand(index) => {
                if let Some(item) = self.config.menu.get(index) {
                    self.spawn_command(&item.command);
                    self.context_menu.popdown();
                }
            }
            Input::ShowContextMenu => {
                if has_visible_menu_items(&self.config.menu) {
                    self.context_menu.popup();
                }
            }
            Input::Reconfigure(config) => {
                if self.config == config {
                    return;
                }
                self.scroll_v.take().map(|h| h.abort());
                self.scroll_h.take().map(|h| h.abort());
                self.context_menu.popdown();
                self.context_menu.unparent();
                self.context_menu = build_context_menu(&self.root, &config, &sender);
                self.view = view_from_config(&config);
                self.config = config;
            }
        }
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.scroll_v.take().map(|h| h.abort());
        self.scroll_h.take().map(|h| h.abort());
        self.context_menu.popdown();
        self.context_menu.unparent();
    }
}

impl Applet {
    pub fn can_launch(config: &Config) -> bool {
        view_from_config(config).visible
    }

    fn debounce_scroll_v(&mut self, command: &[String]) {
        self.scroll_v.take().map(|h| h.abort());
        if command.is_empty() {
            return;
        }
        let name = self.name.clone();
        let cmd = command.to_vec();
        self.scroll_v = Some(relm4::spawn(async move {
            tokio::time::sleep(Duration::from_millis(SCROLL_DEBOUNCE_MS)).await;
            if let Err(e) = run_command(&name, cmd).await {
                tracing::warn!(%e, applet = %name, "scroll command failed");
            }
        }));
    }

    fn debounce_scroll_h(&mut self, command: &[String]) {
        self.scroll_h.take().map(|h| h.abort());
        if command.is_empty() {
            return;
        }
        let name = self.name.clone();
        let cmd = command.to_vec();
        self.scroll_h = Some(relm4::spawn(async move {
            tokio::time::sleep(Duration::from_millis(SCROLL_DEBOUNCE_MS)).await;
            if let Err(e) = run_command(&name, cmd).await {
                tracing::warn!(%e, applet = %name, "scroll command failed");
            }
        }));
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

async fn run_command(applet: &str, command: Vec<String>) -> anyhow::Result<()> {
    let Some((program, args)) = command.split_first() else {
        return Ok(());
    };

    tracing::debug!(applet, %program, ?args, "command applet running command");

    TokioCommand::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

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

fn has_visible_menu_items(menu: &[MenuItemConfig]) -> bool {
    menu.iter()
        .any(|item| !item.label.is_empty() && !item.command.is_empty())
}

fn build_context_menu(
    root: &gtk::Box,
    config: &Config,
    sender: &ComponentSender<Applet>,
) -> gtk::PopoverMenu {
    let action_group = gio::SimpleActionGroup::new();
    let menu = gio::Menu::new();

    for (index, item) in config.menu.iter().enumerate() {
        if item.label.is_empty() || item.command.is_empty() {
            continue;
        }

        let action_name = format!("item-{index}");
        let action = gio::SimpleAction::new(&action_name, None);
        action.connect_activate({
            let sender = sender.input_sender().clone();
            move |_, _| sender.emit(Input::MenuCommand(index))
        });
        action_group.add_action(&action);
        menu.append(Some(&item.label), Some(&format!("command.{action_name}")));
    }

    root.insert_action_group("command", Some(&action_group));
    let popover = gtk::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(root);
    popover.set_has_arrow(false);
    popover
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_accepts_empty_settings() {
        assert_eq!(Config::from_raw(&None), Config::default());
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
