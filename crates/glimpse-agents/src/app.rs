mod view;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use adw::prelude::*;
use futures_util::StreamExt as _;
use glimpse_config::{Config, Documents, Reload, Theme, watch_config};
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
    session: crate::session::State,
    session_surface: Option<Surface>,
    session_focus: bool,
    session_focus_timer: Option<JoinHandle<()>>,
    surface: Option<Surface>,
    closing: Option<(gtk4::Window, glib::JoinHandle<()>)>,
    shown: Option<Head>,
    typed: String,
    polkit: crate::polkit::Driver,
    expiry: Option<JoinHandle<()>>,
    paired: Option<JoinHandle<()>>,
    headless: bool,
    blur: bool,
    user: zbus::Connection,
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

        let watched = init.config.clone();
        let watch_sender = sender.input_sender().clone();
        init.runtime.spawn(async move {
            let mut configs = Box::pin(watch_config(Documents::Config, watched, None));
            while let Some(Reload::Changed(config)) = configs.next().await {
                if watch_sender
                    .send(Input::ConfigChanged(Box::new(config)))
                    .is_err()
                {
                    break;
                }
            }
        });

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
            session: crate::session::State::default(),
            session_surface: None,
            session_focus: false,
            session_focus_timer: None,
            surface: None,
            closing: None,
            shown: None,
            typed: String::new(),
            polkit,
            expiry: None,
            paired: None,
            headless: false,
            blur: blurred(&init.config),
            user: init.session,
            system: init.system,
            runtime: init.runtime,
            _styles: Some(styles),
        };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, input: Input, sender: ComponentSender<Self>) {
        self.ingest(input, sender.input_sender());
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
            session: crate::session::State::default(),
            session_surface: None,
            session_focus: false,
            session_focus_timer: None,
            surface: None,
            closing: None,
            shown: None,
            typed: String::new(),
            polkit,
            expiry: None,
            paired: None,
            headless: true,
            blur: false,
            user: system.clone(),
            system,
            runtime,
            _styles: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn drive(&mut self, input: Input, sender: &Sender<Input>) {
        self.ingest(input, sender);
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

    fn ingest(&mut self, input: Input, sender: &Sender<Input>) {
        match input {
            Input::ConfigChanged(config) => {
                self.blur = blurred(&config);
                if let Some(styles) = &self._styles {
                    styles.set_color_scheme(color_scheme(config.appearance.color_scheme));
                    styles.set_variant(&config.appearance.theme_variant);
                    styles.set_animation_speed(config.appearance.animation_speed);
                }
                if let Some(surface) = &self.surface {
                    glimpse_widgets::blur::apply(&surface.window, self.blur);
                }
                if let Some(surface) = &self.session_surface {
                    glimpse_widgets::blur::apply(&surface.window, self.blur);
                }
            }
            Input::Ask(ask) => {
                self.cancel_session();
                self.queue.push(ask);
            }
            Input::Withdraw { source, key } => self.queue.withdraw(source, key.as_deref()),
            Input::Locked(locked) => {
                self.session.locked = Some(locked);
                if locked {
                    self.cancel_session();
                }
                self.queue.lock(locked);
            }
            Input::LockUnknown => {
                self.session.locked = None;
                self.cancel_session();
            }
            Input::SessionRequest(request) => {
                if let Some((id, action)) = self.session.admit(request, self.queue.head().is_some())
                {
                    let system = self.system.clone();
                    let sender = sender.clone();
                    self.runtime.spawn(async move {
                        let view = crate::session::prepare(&system, action).await;
                        sender.emit(Input::SessionReady { id, view });
                    });
                }
            }
            Input::SessionReady { id, view } => {
                if self
                    .session
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.id == id)
                {
                    match view {
                        Ok(view) => {
                            if let Some(pending) = self.session.pending.as_mut() {
                                pending.view = Some(view);
                            }
                        }
                        Err(reason) => {
                            self.cancel_session();
                            self.notify_session(reason);
                        }
                    }
                }
            }
            Input::SessionResponse { id, response } => {
                let confirm = matches!(response, Response::Confirm { .. });
                let ready = self
                    .session
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.id == id && pending.view.is_some());
                if ready && let Some(pending) = self.session.take(id) {
                    let allowed = self.session.locked == Some(false)
                        && self.queue.head().is_none()
                        && (self.headless || self.session_focus);
                    self.release_session_surface();
                    if confirm && allowed {
                        self.session.executing = true;
                        let system = self.system.clone();
                        let user = self.user.clone();
                        let sender = sender.clone();
                        self.runtime.spawn(async move {
                            sender.emit(Input::SessionDone(
                                crate::session::execute(&system, &user, pending.action).await,
                            ));
                        });
                    }
                }
            }
            Input::SessionFocus { id, active } => {
                if self
                    .session
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.id == id)
                {
                    if active {
                        self.session_focus = true;
                        if let Some(timer) = self.session_focus_timer.take() {
                            timer.abort();
                        }
                    } else if self.session_focus {
                        self.cancel_session();
                    }
                }
            }
            Input::SessionFocusTimeout(id) => {
                if !self.session_focus
                    && self
                        .session
                        .pending
                        .as_ref()
                        .is_some_and(|pending| pending.id == id)
                {
                    self.cancel_session();
                }
            }
            Input::SessionDone(result) => {
                self.session.executing = false;
                if let Err(reason) = result {
                    self.notify_session(reason);
                }
            }
            Input::Line { key, line } => self.polkit.on_line(&mut self.queue, &key, line),
            Input::Dialog {
                source,
                key,
                response,
            } => self.respond(source, &key, response),
        }
    }

    fn render_with(&mut self, sender: &Sender<Input>) {
        if self.session.pending.is_some() {
            self.render_session(sender);
            return;
        }
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

    fn render_session(&mut self, sender: &Sender<Input>) {
        if self.headless || self.session_surface.is_some() {
            return;
        }
        let Some(pending) = self.session.pending.as_ref() else {
            return;
        };
        let Some(view) = pending.view.as_ref() else {
            return;
        };
        let id = pending.id;
        if let Some((window, animation)) = self.closing.take() {
            animation.abort();
            window.destroy();
        }
        let dialog = AgentDialog::new();
        dialog.set_halign(gtk4::Align::Center);
        dialog.set_valign(gtk4::Align::Center);
        dialog.set_vexpand(true);
        dialog.show(view);
        dialog.connect_response({
            let sender = sender.clone();
            move |dialog, response| {
                if dialog.is_sensitive() {
                    sender.emit(Input::SessionResponse { id, response });
                }
            }
        });
        let surface = Surface::new(surface(self.blur), dialog);
        surface.window.connect_is_active_notify({
            let sender = sender.clone();
            move |window| {
                sender.emit(Input::SessionFocus {
                    id,
                    active: window.is_active(),
                })
            }
        });
        self.session_focus = false;
        let timeout = sender.clone();
        self.session_focus_timer = Some(self.runtime.spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            timeout.emit(Input::SessionFocusTimeout(id));
        }));
        surface.window.present();
        self.session_surface = Some(surface);
    }

    fn cancel_session(&mut self) {
        self.session.pending = None;
        self.release_session_surface();
    }

    fn release_session_surface(&mut self) {
        if let Some(timer) = self.session_focus_timer.take() {
            timer.abort();
        }
        self.session_focus = false;
        if let Some(surface) = self.session_surface.take() {
            surface.dialog.set_sensitive(false);
            if surface.window.is_layer_window() {
                surface.window.set_keyboard_mode(KeyboardMode::None);
            }
            surface.window.destroy();
        }
    }

    fn notify_session(&self, reason: String) {
        let session = self.user.clone();
        self.runtime.spawn(async move {
            crate::session::notify(&session, &reason).await;
        });
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
