use std::{
    collections::HashMap,
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

use crate::applets::exec::{
    components::{StatusItem, StatusItemInit, StatusItemInput, StatusItemOutput},
    popover::{Input as PopoverInput, Output as PopoverOutput, Popover},
    protocol::{
        ChildCommand, EventPayload, PanelCommand, PopoverPayload, StatusItem as StatusItemModel,
        StatusPayload, TreeNode, encode_panel_command, parse_child_line,
    },
};

type ConnectionId = u64;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn next_id() -> ConnectionId {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

const OUTBOUND_BUFFER: usize = 64;
const MAX_CONNECTIONS: usize = 32;

pub struct Applet {
    connections: HashMap<ConnectionId, ConnectionState>,
    root: gtk::Box,
}

struct ConnectionState {
    status: Vec<StatusItemModel>,
    rendered_status: Vec<StatusItemModel>,
    root_node: Option<TreeNode>,
    rendered_has_popover: bool,
    slot_box: gtk::Box,
    status_items: Vec<RenderedStatusItem>,
    popover: Controller<Popover>,
    outbound_tx: mpsc::Sender<PanelCommand>,
}

struct RenderedStatusItem {
    key: String,
    controller: Controller<StatusItem>,
}

pub struct Init;

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
            add_css_class: "applet",
            set_orientation: gtk::Orientation::Horizontal,
            set_visible: false,
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();
        let sender_clone = sender.input_sender().clone();
        relm4::spawn(async move {
            run_listener(sender_clone).await;
        });
        let model = Applet {
            connections: HashMap::new(),
            root: widgets.root.clone(),
        };
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            Input::NewConnection { id, outbound_tx } => {
                let slot_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                slot_box.set_visible(false);
                slot_box.set_valign(gtk::Align::Center);
                let popover = Popover::builder()
                    .launch(crate::applets::exec::popover::Init {
                        parent: slot_box.clone(),
                    })
                    .forward(sender.input_sender(), move |output| Input::PopoverOutput {
                        id,
                        output,
                    });
                self.root.append(&slot_box);
                self.connections.insert(
                    id,
                    ConnectionState {
                        status: Vec::new(),
                        rendered_status: Vec::new(),
                        root_node: None,
                        rendered_has_popover: false,
                        slot_box,
                        status_items: Vec::new(),
                        popover,
                        outbound_tx,
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
                    conn.root_node = payload.root.clone();
                    conn.popover.emit(PopoverInput::SetRoot(payload.root));
                }
                self.rebuild_if_needed(id, &sender);
            }
            Input::CssClass { id, class } => {
                if let Some(conn) = self.connections.get(&id) {
                    conn.slot_box.add_css_class(&format!("applet-{class}"));
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
                    self.root.remove(&conn.slot_box);
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
                    StatusItemOutput::ContextMenu => {}
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
        self.root.set_visible(any_visible);
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
        let mut previous: Option<gtk::Widget> = None;

        for (index, item) in conn.status.iter().enumerate() {
            let key = status_item_key(index, item);
            let controller = if let Some(pos) = existing.iter().position(|r| r.key == key) {
                let rendered = existing.remove(pos);
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
                    .forward(sender.input_sender(), move |output| {
                        Input::StatusItemOutput { id, output }
                    })
            };
            let widget = controller.widget().clone().upcast::<gtk::Widget>();
            place_widget(&conn.slot_box, &widget, previous.as_ref());
            previous = Some(widget);
            next.push(RenderedStatusItem { key, controller });
        }
        for rendered in existing {
            detach_widget(rendered.controller.widget());
        }
        conn.status_items = next;
        conn.rendered_status = conn.status.clone();
        conn.rendered_has_popover = has_popover;
        conn.slot_box.set_visible(!conn.status.is_empty());
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

fn place_widget(container: &gtk::Box, widget: &gtk::Widget, sibling: Option<&gtk::Widget>) {
    match widget.parent() {
        Some(parent) if parent == container.clone().upcast::<gtk::Widget>() => {
            container.reorder_child_after(widget, sibling);
        }
        Some(_) => {
            detach_widget(widget);
            container.insert_child_after(widget, sibling);
        }
        None => {
            container.insert_child_after(widget, sibling);
        }
    }
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

async fn run_listener(sender: relm4::Sender<Input>) {
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
    tracing::info!(path = %socket_path.display(), "dynamic applets socket ready");
    let active = Arc::new(AtomicUsize::new(0));
    loop {
        match listener.accept().await {
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
        }
    }
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
                            let _ = sender.send(msg);
                        }
                        Err(e) => {
                            tracing::debug!(%e, connection = id, "dynamic applet ignored line");
                        }
                    }
                }
                Ok(None) | Err(_) => break,
            },
        }
    }

    let _ = sender.send(Input::Disconnected { id });
}
