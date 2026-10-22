mod view;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use adw::prelude::*;
use glimpse_config::{Config, Theme};
use glimpse_widgets::{Styles, color_scheme};
use gtk4::glib;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::{ComponentParts, ComponentSender, Receiver, Sender, SimpleComponent};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

use crate::ask::{Input, Prompt, Reply, Source};
use crate::dialog::{AgentDialog, Response};
use crate::queue::Queue;
use crate::{bluetooth, network};

pub use view::view_for;

pub(crate) const SHOW_CLOSE: Duration = Duration::from_secs(60);

pub struct Init {
    pub config: Config,
    pub theme: Theme,
    pub session: zbus::Connection,
    pub system: zbus::Connection,
    pub runtime: Handle,
    pub inbox: Receiver<Input>,
    pub cancelled: Arc<Mutex<HashSet<String>>>,
}

pub struct App {
    queue: Queue,
    surface: Option<Surface>,
    closing: Option<(gtk4::Window, glib::JoinHandle<()>)>,
    shown: Option<Head>,
    typed: String,
    polkit: crate::polkit::Driver,
    expiry: Option<JoinHandle<()>>,
    paired: Option<JoinHandle<()>>,
    headless: bool,
    blur: bool,
    _session: zbus::Connection,
    system: zbus::Connection,
    runtime: Handle,
    _styles: Option<Styles>,
}

#[derive(Clone, PartialEq, Eq)]
struct Head {
    source: Source,
    key: String,
    shown: bool,
}

struct Surface {
    window: gtk4::Window,
    backdrop: gtk4::Box,
    dialog: AgentDialog,
}

impl Surface {
    fn new(window: gtk4::Window, dialog: AgentDialog) -> Self {
        let backdrop = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .css_classes(["agent-backdrop"])
            .hexpand(true)
            .vexpand(true)
            .halign(gtk4::Align::Fill)
            .valign(gtk4::Align::Fill)
            .build();
        backdrop.append(&dialog);
        window.set_child(Some(&backdrop));
        Self {
            window,
            backdrop,
            dialog,
        }
    }
}

impl SimpleComponent for App {
    type Init = Init;
    type Input = Input;
    type Output = ();
    type Root = gtk4::Window;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk4::Window::new()
    }

    fn init(init: Init, _root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let appearance = &init.config.appearance;
        let styles = Styles::install(color_scheme(appearance.color_scheme));
        styles.load(&init.theme);
        styles.set_variant(&appearance.theme_variant);
        styles.set_animation_speed(appearance.animation_speed);

        let forward = sender.input_sender().clone();
        let inbox = init.inbox;
        init.runtime.spawn(async move {
            while let Some(input) = inbox.recv().await {
                if forward.send(input).is_err() {
                    break;
                }
            }
        });
        init.runtime.spawn(crate::lock::follow(
            init.system.clone(),
            sender.input_sender().clone(),
        ));
        let polkit = crate::polkit::Driver::with_cancelled(
            init.runtime.clone(),
            sender.input_sender().clone(),
            std::path::PathBuf::from(crate::polkit::helper::SOCKET),
            crate::polkit::helper::SETUID
                .iter()
                .map(std::path::PathBuf::from)
                .collect(),
            init.cancelled,
        );

        let model = Self {
            queue: Queue::default(),
            surface: None,
            closing: None,
            shown: None,
            typed: String::new(),
            polkit,
            expiry: None,
            paired: None,
            headless: false,
            blur: blurred(&init.config),
            _session: init.session,
            system: init.system,
            runtime: init.runtime,
            _styles: Some(styles),
        };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        self.ingest(input);
        self.render_with(sender.input_sender());
    }
}

impl App {
    #[cfg(test)]
    pub(crate) fn headless(
        runtime: Handle,
        input: Sender<Input>,
        system: zbus::Connection,
        socket: Option<std::path::PathBuf>,
    ) -> Self {
        let polkit = match socket {
            Some(socket) => {
                crate::polkit::Driver::with_helpers(runtime.clone(), input, socket, Vec::new())
            }
            None => crate::polkit::Driver::new(runtime.clone(), input),
        };
        Self {
            queue: Queue::default(),
            surface: None,
            closing: None,
            shown: None,
            typed: String::new(),
            polkit,
            expiry: None,
            paired: None,
            headless: true,
            blur: false,
            _session: system.clone(),
            system,
            runtime,
            _styles: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn drive(&mut self, input: Input, sender: &Sender<Input>) {
        self.ingest(input);
        self.render_with(sender);
    }

    #[cfg(test)]
    pub(crate) fn set_typed(&mut self, typed: &str) {
        self.typed = typed.to_owned();
    }

    #[cfg(test)]
    pub(crate) fn typed(&self) -> &str {
        &self.typed
    }

    #[cfg(test)]
    pub(crate) fn showing(&self) -> bool {
        self.shown.is_some()
    }

    #[cfg(test)]
    pub(crate) fn conversation(&self) -> Option<&crate::polkit::Conversation> {
        self.polkit.conversation.as_ref()
    }

    fn ingest(&mut self, input: Input) {
        match input {
            Input::Ask(ask) => self.queue.push(ask),
            Input::Withdraw { source, key } => self.queue.withdraw(source, key.as_deref()),
            Input::Locked(locked) => self.queue.lock(locked),
            Input::Line { key, line } => self.polkit.on_line(&mut self.queue, &key, line),
            Input::Dialog {
                source,
                key,
                response,
            } => self.respond(source, &key, response),
        }
    }

    fn render_with(&mut self, sender: &Sender<Input>) {
        let Some(ask) = self.queue.shown() else {
            self.close();
            return;
        };
        let head = Head {
            source: ask.source,
            key: ask.key.clone(),
            shown: matches!(ask.reply, Reply::Shown),
        };
        let opened = self.shown.as_ref() == Some(&head);
        if !opened {
            self.begin(&head, sender);
            self.typed.clear();
            self.shown = Some(head.clone());
        }
        if self.headless {
            return;
        }
        if let Some((window, animation)) = self.closing.take() {
            animation.abort();
            window.destroy();
        }
        let Some(ask) = self.queue.shown() else {
            return;
        };
        let view = view_for(&ask.prompt, self.polkit.conversation.as_ref());
        if opened {
            if let Some(surface) = &self.surface {
                surface.dialog.show(&view);
            }
            return;
        }
        let dialog = AgentDialog::new();
        dialog.set_halign(gtk4::Align::Center);
        dialog.set_valign(gtk4::Align::Center);
        dialog.set_vexpand(true);
        dialog.show(&view);
        dialog.connect_response({
            let sender = sender.clone();
            let source = head.source;
            let key = head.key.clone();
            move |dialog, response| {
                if !dialog.is_sensitive() {
                    return;
                }
                sender.emit(Input::Dialog {
                    source,
                    key: key.clone(),
                    response,
                })
            }
        });
        let surface = match self.surface.take() {
            Some(mut surface) => {
                surface.backdrop.remove(&surface.dialog);
                surface.backdrop.append(&dialog);
                surface.dialog = dialog;
                surface
            }
            None => Surface::new(surface(self.blur), dialog),
        };
        surface.window.present();
        self.surface = Some(surface);
    }

    fn close(&mut self) {
        let open = self.shown.take().is_some() || self.surface.is_some();
        self.end();
        if open {
            self.typed.clear();
        }
        if let Some(surface) = self.surface.take() {
            let duration = glimpse_widgets::duration_ms();
            if self.headless
                || duration == 0
                || !surface.window.settings().is_gtk_enable_animations()
            {
                surface.window.destroy();
                return;
            }
            surface.dialog.set_sensitive(false);
            surface.window.add_css_class("closing");
            let window = surface.window.clone();
            let animation = glib::MainContext::default().spawn_local(async move {
                glib::timeout_future(Duration::from_millis(u64::from(duration))).await;
                window.destroy();
            });
            self.closing = Some((surface.window, animation));
        }
    }

    fn begin(&mut self, head: &Head, sender: &Sender<Input>) {
        self.end();
        if head.shown {
            let key = head.key.clone();
            let expiry = sender.clone();
            self.expiry = Some(self.runtime.spawn(async move {
                tokio::time::sleep(SHOW_CLOSE).await;
                expiry.emit(Input::Withdraw {
                    source: Source::Bluetooth,
                    key: Some(key),
                });
            }));
            let system = self.system.clone();
            let paired = sender.clone();
            let device = head.key.clone();
            self.paired = Some(self.runtime.spawn(async move {
                bluetooth::follow_paired(&system, &device, &paired).await;
            }));
        }
        if head.source == Source::Polkit {
            self.polkit.start(&self.queue);
        }
    }

    fn end(&mut self) {
        if let Some(expiry) = self.expiry.take() {
            expiry.abort();
        }
        if let Some(paired) = self.paired.take() {
            paired.abort();
        }
        self.polkit.stop();
    }

    fn respond(&mut self, source: Source, key: &str, response: Response) {
        match route(&mut self.queue, source, key, response) {
            Routed::Polkit(response) => self.polkit.on_response(&mut self.queue, response),
            Routed::CancelPairing(device) => self.cancel_pairing(device),
            Routed::Sent | Routed::Dropped => {}
        }
    }

    fn cancel_pairing(&self, device: String) {
        let system = self.system.clone();
        self.runtime.spawn(async move {
            let cancelled = async {
                glimpse_dbus::bluez::Device1Proxy::builder(&system)
                    .path(device)?
                    .build()
                    .await?
                    .cancel_pairing()
                    .await
            };
            if let Err(error) = cancelled.await {
                tracing::debug!(%error, "CancelPairing failed");
            }
        });
    }
}

enum Routed {
    Dropped,
    Polkit(Response),
    Sent,
    CancelPairing(String),
}

fn route(queue: &mut Queue, source: Source, key: &str, response: Response) -> Routed {
    let shown = queue
        .shown()
        .is_some_and(|ask| ask.source == source && ask.key == key);
    if !shown {
        return Routed::Dropped;
    }
    if source == Source::Polkit {
        return Routed::Polkit(response);
    }
    let Some(ask) = queue.take_head() else {
        return Routed::Dropped;
    };
    match (ask.prompt, ask.reply) {
        (Prompt::Secret(request), Reply::Secret(reply)) => {
            let _ = reply.send(network_answer(&request, response));
            Routed::Sent
        }
        (Prompt::Pairing(request), Reply::Pairing(reply)) => {
            let _ = reply.send(bluetooth_answer(&request.kind, response));
            Routed::Sent
        }
        (Prompt::Pairing(request), Reply::Shown) => Routed::CancelPairing(request.device),
        _ => Routed::Dropped,
    }
}

fn surface(blur: bool) -> gtk4::Window {
    let window = gtk4::Window::new();
    window.init_layer_shell();
    window.set_namespace(Some("glimpse-agents"));
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }

    window.set_exclusive_zone(-1);
    window.add_css_class("agent-surface");
    glimpse_widgets::blur::apply(&window, blur);
    window
}

fn blurred(config: &Config) -> bool {
    config.appearance.blur
}

pub fn network_answer(request: &network::Request, response: Response) -> network::Answer {
    match response {
        Response::Confirm {
            username,
            secret,
            autoconnect,
            ..
        } => network::Answer::Secret {
            secret,
            username: view::asks_username(request).then_some(username),
            autoconnect,
        },
        Response::Cancel | Response::Account(_) => network::Answer::Refused,
    }
}

pub fn bluetooth_answer(kind: &bluetooth::Kind, response: Response) -> bluetooth::Answer {
    let Response::Confirm { secret, .. } = response else {
        return bluetooth::Answer::Deny;
    };
    match kind {
        bluetooth::Kind::Confirm { .. } | bluetooth::Kind::Authorize => bluetooth::Answer::Confirm,
        bluetooth::Kind::Pin => bluetooth::Answer::Pin(secret.as_str().to_owned()),
        bluetooth::Kind::Passkey => secret
            .parse::<u32>()
            .ok()
            .filter(|passkey| *passkey <= crate::dialog::PASSKEY_MAX)
            .map_or(bluetooth::Answer::Deny, bluetooth::Answer::Passkey),
        bluetooth::Kind::ShowPin { .. } | bluetooth::Kind::ShowPasskey { .. } => {
            bluetooth::Answer::Deny
        }
    }
}

#[cfg(test)]
mod tests;
