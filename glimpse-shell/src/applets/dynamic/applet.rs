use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::mpsc,
};
use tokio_util::sync::CancellationToken;

use crate::{
    applets::exec::{
        components::StatusItemOutput,
        popover::{Input as PopoverInput, Output as PopoverOutput, Popover},
        protocol::{
            ChildCommand, EventPayload, PanelCommand, PopoverPayload,
            StatusItem as StatusItemModel, StatusPayload, TreeNode, encode_panel_command,
            parse_child_line,
        },
    },
    widgets::panel_indicator::PanelIndicator,
};

type ConnectionId = u64;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn next_id() -> ConnectionId {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

const OUTBOUND_BUFFER: usize = 64;
const MAX_CONNECTIONS: usize = 32;

struct ConnectionStatusItem {
    item: StatusItemModel,
    has_popover: bool,
    icon: gtk::Image,
    label: gtk::Label,
}

#[derive(Debug, Clone)]
struct ConnectionStatusItemInit {
    item: StatusItemModel,
    has_popover: bool,
}

#[derive(Debug)]
enum ConnectionStatusItemInput {
    Click(u32),
    Scroll(f64),
    Reconfigure {
        item: StatusItemModel,
        has_popover: bool,
    },
}

#[allow(unused_assignments)]
#[relm4::component]
impl SimpleComponent for ConnectionStatusItem {
    type Init = ConnectionStatusItemInit;
    type Input = ConnectionStatusItemInput;
    type Output = StatusItemOutput;

    view! {
        root = gtk::Box {
            add_css_class: "dynamic-status-item",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            set_valign: gtk::Align::Center,
            #[watch]
            set_tooltip_text: model.item.tooltip.as_deref(),

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |gesture, _, _, _| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    sender.input(ConnectionStatusItemInput::Click(1));
                }
            },
            add_controller = gtk::GestureClick {
                set_button: 2,
                connect_pressed[sender] => move |gesture, _, _, _| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    sender.input(ConnectionStatusItemInput::Click(2));
                }
            },
            add_controller = gtk::GestureClick {
                set_button: 3,
                connect_pressed[sender] => move |gesture, _, _, _| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    sender.input(ConnectionStatusItemInput::Click(3));
                }
            },

            #[name = "icon"]
            gtk::Image {
                set_pixel_size: 16,
                set_valign: gtk::Align::Center,
                set_visible: false,
            },

            #[name = "label"]
            gtk::Label {
                set_valign: gtk::Align::Center,
                set_xalign: 0.5,
                set_visible: false,
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = ConnectionStatusItem {
            item: init.item,
            has_popover: init.has_popover,
            icon: gtk::Image::new(),
            label: gtk::Label::new(None),
        };
        let widgets = view_output!();

        let scroll_sender = sender.input_sender().clone();
        let scroll = gtk::EventControllerScroll::new(
            gtk::EventControllerScrollFlags::BOTH_AXES | gtk::EventControllerScrollFlags::DISCRETE,
        );
        scroll.connect_scroll(move |_, _dx, dy| {
            scroll_sender.emit(ConnectionStatusItemInput::Scroll(dy));
            gtk::glib::Propagation::Stop
        });
        widgets.root.add_controller(scroll);

        let mut model = model;
        model.icon = widgets.icon.clone();
        model.label = widgets.label.clone();
        model.apply_view();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            ConnectionStatusItemInput::Click(button) => {
                if button == 3 {
                    let _ = sender.output(StatusItemOutput::ContextMenu);
                    return;
                }

                let event = self.item.id.as_ref().map(|id| click_event(id, button));
                if button == 1 {
                    let output = match event {
                        Some(event) => StatusItemOutput::Activate(Some(event)),
                        None if self.has_popover => StatusItemOutput::TogglePopover,
                        None => return,
                    };
                    let _ = sender.output(output);
                    return;
                }

                if let Some(event) = event {
                    let _ = sender.output(StatusItemOutput::Event(event));
                }
            }
            ConnectionStatusItemInput::Scroll(delta_y) => {
                if let Some(id) = &self.item.id {
                    let _ = sender.output(StatusItemOutput::Event(scroll_event(id, delta_y)));
                }
            }
            ConnectionStatusItemInput::Reconfigure { item, has_popover } => {
                self.item = item;
                self.has_popover = has_popover;
                self.apply_view();
            }
        }
    }
}

impl ConnectionStatusItem {
    fn apply_view(&self) {
        apply_status_icon(&self.icon, self.item.icon.as_deref());
        match self.item.label.as_deref().filter(|label| !label.is_empty()) {
            Some(label) => {
                self.label.set_label(label);
                self.label.set_visible(true);
            }
            None => {
                self.label.set_label("");
                self.label.set_visible(false);
            }
        }
    }
}

fn apply_status_icon(image: &gtk::Image, icon: Option<&str>) {
    match icon.filter(|icon| !icon.is_empty()) {
        Some(icon) if dynamic_icon_is_path(icon) => {
            image.set_from_file(Some(std::path::Path::new(icon)));
            image.set_visible(true);
        }
        Some(icon) => {
            image.set_icon_name(Some(icon));
            image.set_visible(true);
        }
        None => {
            image.set_icon_name(None::<&str>);
            image.set_visible(false);
        }
    }
}

fn dynamic_icon_is_path(icon: &str) -> bool {
    icon.starts_with('/') || icon.starts_with("./") || icon.starts_with("../") || icon.contains('/')
}

fn click_event(id: &str, button: u32) -> EventPayload {
    EventPayload {
        id: id.into(),
        kind: crate::applets::exec::protocol::EventKind::Click,
        source: crate::applets::exec::protocol::EventSource::Status,
        button: Some(crate::applets::exec::protocol::MouseButton::from_number(
            button,
        )),
        active: None,
        value: None,
        delta_y: None,
    }
}

fn scroll_event(id: &str, delta_y: f64) -> EventPayload {
    EventPayload {
        id: id.into(),
        kind: crate::applets::exec::protocol::EventKind::Scroll,
        source: crate::applets::exec::protocol::EventSource::Status,
        button: None,
        active: None,
        value: None,
        delta_y: Some(delta_y),
    }
}

pub struct Applet {
    connections: HashMap<ConnectionId, ConnectionState>,
    root: gtk::Box,
    runtime_container: gtk::Box,
    cancel: CancellationToken,
}

struct ConnectionState {
    status: Vec<StatusItemModel>,
    rendered_status: Vec<StatusItemModel>,
    root_node: Option<TreeNode>,
    rendered_has_popover: bool,
    indicator: PanelIndicator,
    status_items: Vec<RenderedStatusItem>,
    popover: Controller<Popover>,
    outbound_tx: mpsc::Sender<PanelCommand>,
    applet_css_class: Option<String>,
}

struct RenderedStatusItem {
    key: String,
    controller: Controller<ConnectionStatusItem>,
}

pub struct Init {
    pub runtime_container: gtk::Box,
}

#[derive(Debug)]
pub enum Input {
    NewConnection {
        id: ConnectionId,
        outbound_tx: mpsc::Sender<PanelCommand>,
    },
    StatusChanged {
        id: ConnectionId,
        payload: StatusPayload,
    },
    PopoverChanged {
        id: ConnectionId,
        payload: PopoverPayload,
    },
    CssClass {
        id: ConnectionId,
        class: String,
    },
    ClosePopover {
        id: ConnectionId,
    },
    Disconnected {
        id: ConnectionId,
    },
    StatusItemOutput {
        id: ConnectionId,
        output: StatusItemOutput,
    },
    PopoverOutput {
        id: ConnectionId,
        output: PopoverOutput,
    },
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = gtk::Box {
            set_visible: false,
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();
        let cancel = CancellationToken::new();
        let sender_clone = sender.input_sender().clone();
        let cancel_clone = cancel.clone();
        relm4::spawn(async move {
            run_listener(sender_clone, cancel_clone).await;
        });
        let model = Applet {
            connections: HashMap::new(),
            root: widgets.root.clone(),
            runtime_container: init.runtime_container,
            cancel,
        };
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            Input::NewConnection { id, outbound_tx } => {
                let indicator = PanelIndicator::new();
                indicator.add_css_class("dynamic-connection");
                indicator.set_visible(false);
                indicator.set_valign(gtk::Align::Center);
                let popover_parent = indicator.clone().upcast::<gtk::Box>();
                let popover = Popover::builder()
                    .launch(crate::applets::exec::popover::Init {
                        parent: popover_parent,
                    })
                    .forward(sender.input_sender(), move |output| Input::PopoverOutput {
                        id,
                        output,
                    });
                self.runtime_container.append(&indicator);
                self.connections.insert(
                    id,
                    ConnectionState {
                        status: Vec::new(),
                        rendered_status: Vec::new(),
                        root_node: None,
                        rendered_has_popover: false,
                        indicator,
                        status_items: Vec::new(),
                        popover,
                        outbound_tx,
                        applet_css_class: None,
                    },
                );
            }
            Input::StatusChanged { id, payload } => {
                if let Some(conn) = self.connections.get_mut(&id) {
                    conn.status = payload.items;
                }
                self.rebuild_if_needed(id, &sender);
            }
            Input::PopoverChanged { id, payload } => {
                if let Some(conn) = self.connections.get_mut(&id) {
                    if conn.root_node != payload.root {
                        conn.root_node = payload.root.clone();
                        conn.popover.emit(PopoverInput::SetRoot(payload.root));
                    }
                }
                self.rebuild_if_needed(id, &sender);
            }
            Input::CssClass { id, class } => {
                if let Some(conn) = self.connections.get_mut(&id) {
                    if let Some(previous) = &conn.applet_css_class {
                        conn.indicator
                            .remove_css_class(&format!("applet-{previous}"));
                    }
                    conn.indicator.add_css_class(&format!("applet-{class}"));
                    conn.applet_css_class = Some(class.clone());
                    conn.popover.emit(PopoverInput::SetCssClass(class));
                }
            }
            Input::ClosePopover { id } => {
                if let Some(conn) = self.connections.get(&id) {
                    conn.popover.emit(PopoverInput::Close);
                }
            }
            Input::Disconnected { id } => {
                if let Some(conn) = self.connections.remove(&id) {
                    self.runtime_container.remove(&conn.indicator);
                    tracing::debug!(connection = id, "dynamic applet disconnected");
                    self.sync_root_visibility();
                }
            }
            Input::StatusItemOutput { id, output } => {
                let conn = self.connections.get(&id);
                match output {
                    StatusItemOutput::TogglePopover => {
                        if let Some(conn) = conn {
                            if conn.root_node.is_some() {
                                conn.popover.emit(PopoverInput::Toggle);
                            }
                        }
                    }
                    StatusItemOutput::ContextMenu | StatusItemOutput::RestartCommand => {}
                    StatusItemOutput::Event(event) => {
                        if let Some(conn) = conn {
                            send_event(&conn.outbound_tx, id, event);
                        }
                    }
                    StatusItemOutput::Activate(event) => {
                        if let Some(conn) = conn {
                            if let Some(event) = event {
                                send_event(&conn.outbound_tx, id, event);
                            }
                            if conn.root_node.is_some() {
                                conn.popover.emit(PopoverInput::Toggle);
                            }
                        }
                    }
                }
            }
            Input::PopoverOutput {
                id,
                output: PopoverOutput::Event(event),
            } => {
                if let Some(conn) = self.connections.get(&id) {
                    send_event(&conn.outbound_tx, id, event);
                }
            }
        }
    }
}

impl Applet {
    fn sync_root_visibility(&self) {
        let any_visible = self.connections.values().any(|c| !c.status.is_empty());
        self.runtime_container.set_visible(any_visible);
        self.root.set_visible(false);
    }

    fn rebuild_if_needed(&mut self, id: ConnectionId, sender: &ComponentSender<Self>) {
        let Some(conn) = self.connections.get_mut(&id) else {
            return;
        };
        let has_popover = conn.root_node.is_some();
        if conn.rendered_status == conn.status && conn.rendered_has_popover == has_popover {
            return;
        }

        let mut existing = std::mem::take(&mut conn.status_items);
        let mut next = Vec::with_capacity(conn.status.len());
        conn.indicator.clear_extra();

        for (index, item) in conn.status.iter().enumerate() {
            let key = status_item_key(index, item);
            let controller = if let Some(pos) = existing.iter().position(|r| r.key == key) {
                let rendered = existing.remove(pos);
                rendered
                    .controller
                    .emit(ConnectionStatusItemInput::Reconfigure {
                        item: item.clone(),
                        has_popover,
                    });
                rendered.controller
            } else {
                ConnectionStatusItem::builder()
                    .launch(ConnectionStatusItemInit {
                        item: item.clone(),
                        has_popover,
                    })
                    .forward(sender.input_sender(), move |output| {
                        Input::StatusItemOutput { id, output }
                    })
            };
            let widget = controller.widget().clone().upcast::<gtk::Widget>();
            conn.indicator.append_extra(&widget);
            next.push(RenderedStatusItem { key, controller });
        }
        for rendered in existing {
            detach_widget(rendered.controller.widget());
        }
        conn.status_items = next;
        conn.rendered_status = conn.status.clone();
        conn.rendered_has_popover = has_popover;
        conn.indicator.set_visible(!conn.status.is_empty());
        self.sync_root_visibility();
    }
}

fn status_item_key(index: usize, item: &StatusItemModel) -> String {
    item.id
        .as_ref()
        .filter(|id| !id.is_empty())
        .map(|id| format!("id:{id}"))
        .unwrap_or_else(|| format!("index:{index}"))
}

fn detach_widget(widget: &impl IsA<gtk::Widget>) {
    if let Some(parent) = widget.as_ref().parent()
        && let Ok(parent) = parent.downcast::<gtk::Box>()
    {
        parent.remove(widget);
    }
}

fn send_event(tx: &mpsc::Sender<PanelCommand>, id: ConnectionId, event: EventPayload) {
    if let Err(e) = tx.try_send(PanelCommand::Event(event)) {
        tracing::warn!(%e, connection = id, "dynamic applet failed to queue event");
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.cancel.cancel();
        for (_, conn) in self.connections.drain() {
            self.runtime_container.remove(&conn.indicator);
        }
        self.runtime_container.set_visible(false);
    }
}

async fn run_listener(sender: relm4::Sender<Input>, cancel: CancellationToken) {
    let socket_path = glimpse_core::ipc::applets_socket_path();
    if let Some(parent) = socket_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::remove_file(&socket_path);
    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(
                ?e,
                path = %socket_path.display(),
                "dynamic applets socket bind failed — \
                 only one __dynamic__ slot is allowed per panel"
            );
            return;
        }
    };
    let _ = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600));
    tracing::info!(path = %socket_path.display(), "dynamic applets socket ready");
    let active = Arc::new(AtomicUsize::new(0));
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            result = listener.accept() => match result {
                Ok((stream, _)) => {
                    if active.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
                        tracing::warn!(
                            limit = MAX_CONNECTIONS,
                            "dynamic applets connection limit reached; rejecting connection"
                        );
                        drop(stream);
                        continue;
                    }
                    active.fetch_add(1, Ordering::Relaxed);
                    let id = next_id();
                    let (outbound_tx, outbound_rx) = mpsc::channel(OUTBOUND_BUFFER);
                    let _ = sender.send(Input::NewConnection { id, outbound_tx });
                    let sender_clone = sender.clone();
                    let active_clone = Arc::clone(&active);
                    tokio::spawn(async move {
                        run_connection(id, stream, outbound_rx, sender_clone).await;
                        active_clone.fetch_sub(1, Ordering::Relaxed);
                    });
                }
                Err(e) => {
                    tracing::warn!(?e, "dynamic applets accept error");
                }
            },
        }
    }
    let _ = std::fs::remove_file(&socket_path);
}

async fn run_connection(
    id: ConnectionId,
    stream: UnixStream,
    mut outbound_rx: mpsc::Receiver<PanelCommand>,
    sender: relm4::Sender<Input>,
) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    loop {
        tokio::select! {
            outbound = outbound_rx.recv() => match outbound {
                Some(cmd) => {
                    let mut line = encode_panel_command(&cmd).into_bytes();
                    line.push(b'\n');
                    if writer.write_all(&line).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            result = lines.next_line() => match result {
                Ok(Some(line)) => {
                    let trimmed = line.trim();
                    if trimmed.starts_with("init ") || trimmed == "init" {
                        continue;
                    }
                    match parse_child_line(trimmed) {
                        Ok(cmd) => {
                            let msg = match cmd {
                                ChildCommand::Status(payload) => {
                                    Input::StatusChanged { id, payload }
                                }
                                ChildCommand::Popover(payload) => {
                                    Input::PopoverChanged { id, payload }
                                }
                                ChildCommand::Class(class) => Input::CssClass { id, class },
                                ChildCommand::ClosePopover => Input::ClosePopover { id },
                            };
                            if sender.send(msg).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                %e,
                                raw = %trimmed,
                                connection = id,
                                "dynamic applet ignored line"
                            );
                        }
                    }
                }
                Ok(None) | Err(_) => break,
            },
        }
    }

    let _ = sender.send(Input::Disconnected { id });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_event_targets_status_item() {
        assert_eq!(
            click_event("cpu", 1),
            EventPayload {
                id: "cpu".into(),
                kind: crate::applets::exec::protocol::EventKind::Click,
                source: crate::applets::exec::protocol::EventSource::Status,
                button: Some(crate::applets::exec::protocol::MouseButton::Left),
                active: None,
                value: None,
                delta_y: None,
            }
        );
    }

    #[test]
    fn scroll_event_targets_status_item() {
        assert_eq!(
            scroll_event("cpu", -1.5),
            EventPayload {
                id: "cpu".into(),
                kind: crate::applets::exec::protocol::EventKind::Scroll,
                source: crate::applets::exec::protocol::EventSource::Status,
                button: None,
                active: None,
                value: None,
                delta_y: Some(-1.5),
            }
        );
    }
}
