use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::panels::applets::AppletConfig;
use glimpse_core::ipc::IpcEmitter;

use super::{
    components::{StatusItem, StatusItemInit, StatusItemInput, StatusItemOutput},
    popover::{Input as PopoverInput, Output as PopoverOutput, Popover},
    protocol::{
        EventPayload, PanelCommand, PopoverPayload, StatusItem as StatusItemModel, StatusPayload,
        TreeNode,
    },
    supervisor::{self, Control},
};

const DEFAULT_RESTART_DELAY_MS: u64 = 1000;
const MIN_RESTART_DELAY_MS: u64 = 50;
const OUTBOUND_EVENT_BUFFER: usize = 128;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub command: Vec<String>,
    pub restart_delay_ms: u64,
    pub options: serde_json::Value,
    pub env_forward: bool,
    pub env: std::collections::HashMap<String, String>,
    pub work_dir: Option<std::path::PathBuf>,
}

impl Config {
    pub fn from_raw(name: &str, raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        let mut config: Self = match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(applet = name, %error, "invalid exec applet config, using defaults");
                Self::default()
            }
        };
        config.normalize();
        config
    }

    fn normalize(&mut self) {
        if self.restart_delay_ms < MIN_RESTART_DELAY_MS {
            tracing::warn!(
                requested_ms = self.restart_delay_ms,
                clamped_ms = MIN_RESTART_DELAY_MS,
                "exec applet restart_delay_ms below minimum; clamping"
            );
            self.restart_delay_ms = MIN_RESTART_DELAY_MS;
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            command: Vec::new(),
            restart_delay_ms: DEFAULT_RESTART_DELAY_MS,
            options: serde_json::json!({}),
            env_forward: false,
            env: std::collections::HashMap::new(),
            work_dir: None,
        }
    }
}

pub struct Applet {
    name: String,
    config: Config,
    status: Vec<StatusItemModel>,
    rendered_status: Vec<StatusItemModel>,
    root_node: Option<TreeNode>,
    rendered_has_popover: bool,
    root: gtk::Box,
    popover: Controller<Popover>,
    status_box: gtk::Box,
    status_items: Vec<RenderedStatusItem>,
    outbound_tx: mpsc::Sender<PanelCommand>,
    control_tx: mpsc::UnboundedSender<Control>,
    applet_css_class: Option<String>,
}

#[derive(Debug)]
pub struct Init {
    pub name: String,
    pub config: Config,
    pub ipc: IpcEmitter,
}

#[derive(Debug)]
pub enum Input {
    StatusChanged(StatusPayload),
    PopoverChanged(PopoverPayload),
    ChildExited,
    Reconfigure(Option<AppletConfig>),
    CssClass(String),
    ClosePopover,
    StatusItemOutput(StatusItemOutput),
    PopoverOutput(PopoverOutput),
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

            #[name = "status_box"]
            gtk::Box {
                add_css_class: "exec-status-box",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 0,
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = Popover::builder()
            .launch(super::popover::Init {
                parent: root.clone(),
            })
            .forward(sender.input_sender(), Input::PopoverOutput);

        let (outbound_tx, outbound_rx) = mpsc::channel(OUTBOUND_EVENT_BUFFER);
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let name = init.name.clone();
        let config = init.config.clone();
        let ipc = init.ipc.clone();
        let supervisor_sender = sender.input_sender().clone();
        relm4::spawn(async move {
            supervisor::run(
                name,
                config,
                outbound_rx,
                control_rx,
                supervisor_sender,
                ipc,
            )
            .await;
        });

        let widgets = view_output!();
        widgets.root.set_visible(false);

        let model = Applet {
            name: init.name,
            config: init.config,
            status: Vec::new(),
            rendered_status: Vec::new(),
            root_node: None,
            rendered_has_popover: false,
            root: widgets.root.clone(),
            popover,
            status_box: widgets.status_box.clone(),
            status_items: Vec::new(),
            outbound_tx,
            control_tx,
            applet_css_class: None,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            Input::StatusChanged(payload) => {
                self.status = payload.items;
                self.rebuild_status_if_needed(&sender);
            }
            Input::PopoverChanged(payload) => {
                if self.root_node != payload.root {
                    self.root_node = payload.root;
                    self.popover
                        .emit(PopoverInput::SetRoot(self.root_node.clone()));
                }
                self.rebuild_status_if_needed(&sender);
            }
            Input::ChildExited => {
                self.root_node = None;
                self.status = vec![crashed_placeholder(&self.name)];
                self.popover.emit(PopoverInput::Close);
                self.rebuild_status_if_needed(&sender);
            }
            Input::Reconfigure(raw) => {
                let config = Config::from_raw(&self.name, &raw);
                if self.config == config {
                    return;
                }
                self.config = config.clone();
                self.popover.emit(PopoverInput::Close);
                if let Err(error) = self.control_tx.send(Control::Reconfigure(config)) {
                    tracing::warn!(%error, applet = %self.name, "exec applet failed to reconfigure");
                }
            }
            Input::CssClass(class) => {
                if let Some(previous) = &self.applet_css_class {
                    self.root.remove_css_class(&format!("applet-{previous}"));
                }
                self.root.add_css_class(&format!("applet-{class}"));
                self.applet_css_class = Some(class.clone());
                self.popover.emit(PopoverInput::SetCssClass(class));
            }
            Input::ClosePopover => {
                self.popover.emit(PopoverInput::Close);
            }
            Input::StatusItemOutput(output) => match output {
                StatusItemOutput::TogglePopover => {
                    self.toggle_popover_if_available();
                }
                StatusItemOutput::ContextMenu => {}
                StatusItemOutput::RestartCommand => {
                    self.popover.emit(PopoverInput::Close);
                    self.restart_command();
                }
                StatusItemOutput::Event(event) => self.send_event(event),
                StatusItemOutput::Activate(event) => {
                    if let Some(event) = event {
                        self.send_event(event);
                    }
                    self.toggle_popover_if_available();
                }
            },
            Input::PopoverOutput(PopoverOutput::Event(event)) => self.send_event(event),
        }
    }
}

impl Applet {
    pub fn can_launch(config: &Config) -> bool {
        !config.command.is_empty()
    }

    fn has_popover_content(&self) -> bool {
        self.root_node.is_some()
    }

    fn toggle_popover_if_available(&self) {
        if !self.has_popover_content() {
            return;
        }
        self.popover.emit(PopoverInput::Toggle);
    }

    fn rebuild_status_if_needed(&mut self, sender: &ComponentSender<Self>) {
        let has_popover = self.has_popover_content();
        if self.rendered_status == self.status && self.rendered_has_popover == has_popover {
            return;
        }

        let mut existing = std::mem::take(&mut self.status_items);
        let mut next = Vec::with_capacity(self.status.len());
        let mut previous: Option<gtk::Widget> = None;
        for (index, item) in self.status.iter().enumerate() {
            let key = status_item_key(index, item);
            let controller =
                if let Some(position) = existing.iter().position(|rendered| rendered.key == key) {
                    let rendered = existing.remove(position);
                    rendered.controller.emit(StatusItemInput::Reconfigure {
                        item: item.clone(),
                        has_popover,
                    });
                    rendered.controller
                } else {
                    StatusItem::builder()
                        .launch(StatusItemInit {
                            item: item.clone(),
                            has_popover,
                        })
                        .forward(sender.input_sender(), Input::StatusItemOutput)
                };
            let widget = controller.widget().clone().upcast::<gtk::Widget>();
            place_status_widget(&self.status_box, &widget, previous.as_ref());
            previous = Some(widget);
            next.push(RenderedStatusItem { key, controller });
        }
        for rendered in existing {
            detach_status_widget(rendered.controller.widget());
        }
        self.status_items = next;

        self.rendered_status = self.status.clone();
        self.rendered_has_popover = has_popover;
        self.root.set_visible(!self.status.is_empty());
    }

    fn send_event(&self, event: EventPayload) {
        if let Err(error) = self.outbound_tx.try_send(PanelCommand::Event(event)) {
            tracing::warn!(%error, applet = %self.name, "exec applet failed to queue event");
        }
    }

    fn restart_command(&self) {
        if let Err(error) = self.control_tx.send(Control::Restart) {
            tracing::warn!(%error, applet = %self.name, "exec applet failed to restart");
        }
    }
}

fn place_status_widget(container: &gtk::Box, widget: &gtk::Widget, sibling: Option<&gtk::Widget>) {
    match widget.parent() {
        Some(parent) if parent == container.clone().upcast::<gtk::Widget>() => {
            container.reorder_child_after(widget, sibling);
        }
        Some(_) => {
            detach_status_widget(widget);
            container.insert_child_after(widget, sibling);
        }
        None => {
            container.insert_child_after(widget, sibling);
        }
    }
}

fn detach_status_widget(widget: &impl IsA<gtk::Widget>) {
    if let Some(parent) = widget.as_ref().parent()
        && let Ok(parent) = parent.downcast::<gtk::Box>()
    {
        parent.remove(widget);
    }
}

struct RenderedStatusItem {
    key: String,
    controller: Controller<StatusItem>,
}

/// Key used to reuse a `StatusItem` relm4 controller across status updates
/// when its `id` is stable. Intentionally derived from `id` only (not the
/// rest of the wire model) — visual fields like `label`, `icon`, `tooltip`,
/// and `css_classes` change on every tick and are picked up via the
/// `Reconfigure` input, so including them here would needlessly tear down
/// and re-create the underlying GTK widget on each update.
fn status_item_key(index: usize, item: &StatusItemModel) -> String {
    item.id
        .as_ref()
        .filter(|id| !id.is_empty())
        .map(|id| format!("id:{id}"))
        .unwrap_or_else(|| format!("index:{index}"))
}

/// Placeholder shown in place of the real status when the child crashes.
/// Every `StatusItem` carries a "Restart" context menu unconditionally, so
/// keeping this one indicator visible (instead of clearing the applet down
/// to nothing) keeps manual restart reachable exactly when it's needed.
fn crashed_placeholder(name: &str) -> StatusItemModel {
    StatusItemModel {
        id: None,
        icon: Some("dialog-error-symbolic".into()),
        label: None,
        tooltip: Some(format!("{name} crashed — right-click to restart")),
        css_classes: vec!["exec-status-error".into()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_command_configs_do_not_launch() {
        assert!(!Applet::can_launch(&Config::default()));
    }

    #[test]
    fn config_defaults_to_not_forwarding_environment() {
        assert!(!Config::default().env_forward);
    }

    #[test]
    fn config_accepts_env_forward_opt_in() {
        let raw = AppletConfig {
            settings: toml::toml! {
                env_forward = true
            }
            .into(),
            ..AppletConfig::default()
        };

        let config = Config::from_raw("test", &Some(raw));

        assert!(config.env_forward);
    }

    #[test]
    fn command_configs_can_launch() {
        let config = Config {
            command: vec!["/tmp/example".into()],
            ..Config::default()
        };

        assert!(Applet::can_launch(&config));
    }

    #[test]
    fn status_item_keys_prefer_protocol_ids() {
        let item = StatusItemModel {
            id: Some("cpu".into()),
            icon: None,
            label: Some("10%".into()),
            tooltip: None,
            css_classes: vec![],
        };

        assert_eq!(status_item_key(3, &item), "id:cpu");
    }

    #[test]
    fn status_item_keys_fall_back_to_index_without_id() {
        let item = StatusItemModel {
            id: None,
            icon: None,
            label: Some("10%".into()),
            tooltip: None,
            css_classes: vec![],
        };

        assert_eq!(status_item_key(3, &item), "index:3");
    }

    #[test]
    fn crashed_placeholder_carries_an_error_icon_and_named_tooltip() {
        let item = crashed_placeholder("sysmonitor");

        assert_eq!(item.icon.as_deref(), Some("dialog-error-symbolic"));
        assert!(item.tooltip.as_deref().unwrap().contains("sysmonitor"));
        assert!(item.tooltip.as_deref().unwrap().contains("crashed"));
        assert!(item.css_classes.iter().any(|c| c == "exec-status-error"));
    }

    #[test]
    fn crashed_placeholder_has_no_id_so_it_never_dispatches_click_events() {
        // StatusItem::Click only sends an event when item.id is Some — the
        // placeholder must stay inert (Restart menu still works via the
        // context menu, which every StatusItem carries unconditionally).
        assert_eq!(crashed_placeholder("x").id, None);
    }
}
