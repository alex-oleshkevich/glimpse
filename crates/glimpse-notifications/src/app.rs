use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::Path,
    rc::Rc,
    time::Duration,
};

use adw::prelude::*;
use futures_util::StreamExt;
use gettextrs::gettext;
use glimpse_config::{
    Config, Documents, NotificationEdge, PANEL_STYLESHEET, Reload, Theme, watch_config, watch_theme,
};
use glimpse_dbus::notifications::{NotificationRecord, NotificationUrgency};
use glimpse_services::SessionStatus;
use glimpse_services::{
    CompositorHandle, CompositorState, NotificationsHandle, NotificationsState, ServiceState,
};
use glimpse_widgets::actions::ComponentActions;
use glimpse_widgets::blur;
use glimpse_widgets::color_scheme;
use glimpse_widgets::{
    Action, Body, Notification, NotificationCard, Styles, Urgency, notification_icon,
    notification_image,
};
use gtk4::{cairo, gdk, gio, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::{ComponentParts, ComponentSender, SimpleComponent};
use tokio::{task::JoinHandle, time::Instant};

use crate::services::NotificationServices;
use crate::state::{Delta, PopupState};
use glimpse_dbus::notifications::DEFAULT_ACTION;

pub struct Init {
    pub config: Config,
    pub theme: Theme,
    pub failure: Option<String>,
}

#[derive(Debug)]
pub enum Input {
    ServicesReady(Box<Result<NotificationServices, String>>),
    Notifications(NotificationsState),
    NotificationsHealth(ServiceState),
    Compositor(CompositorState),
    Session(Option<SessionStatus>),
    Config(Box<Config>),
    ConfigVerdict(Result<(), String>),
    Theme(Reload<Theme>),
    MonitorsChanged,
    Expire(u32),
    Pause(u32),
    Resume(u32),
    Hide(u32),
    Dismiss(u32),
    Activate(u32),
    Invoke(u32, String),
    Settled(u32),
    Finalize(u32),
    ImageLoaded(u32, gdk::Texture),
    Allocated,
}

struct Timer {
    countdown: Option<Countdown>,
    task: Option<JoinHandle<()>>,
    hovered: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryState {
    Entering,
    Visible,
    Leaving,
}

impl EntryState {
    fn accepts_input(self) -> bool {
        !self.is_leaving()
    }

    fn is_leaving(self) -> bool {
        matches!(self, Self::Leaving)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Countdown {
    remaining: Duration,
    deadline: Option<Instant>,
}

impl Countdown {
    fn running(duration: Duration, now: Instant) -> Self {
        Self {
            remaining: duration,
            deadline: Some(deadline(now, duration)),
        }
    }

    fn pause(&mut self, now: Instant) {
        if let Some(deadline) = self.deadline.take() {
            self.remaining = deadline.saturating_duration_since(now);
        }
    }

    fn resume(&mut self, now: Instant) -> Option<Duration> {
        self.deadline.is_none().then(|| {
            self.deadline = Some(deadline(now, self.remaining));
            self.remaining
        })
    }

    fn restart(&mut self, duration: Duration, now: Instant) -> bool {
        self.remaining = duration;
        let running = self.deadline.is_some();
        self.deadline = running.then_some(deadline(now, duration));
        running
    }

    fn expired(&self, now: Instant) -> bool {
        self.deadline.is_some_and(|deadline| deadline <= now)
    }
}

struct Entry {
    frame: gtk4::Box,
    card: NotificationCard,
    timer: Timer,
    animation: adw::TimedAnimation,
    state: EntryState,
    image: Option<glib::JoinHandle<()>>,
}

impl Drop for Entry {
    fn drop(&mut self) {
        if let Some(task) = self.image.take() {
            task.abort();
        }
        if let Some(task) = self.timer.task.take() {
            task.abort();
        }
    }
}

pub struct App {
    actions: ComponentActions,
    root: gtk4::Window,
    stack: gtk4::Box,
    config: Config,
    theme: Theme,
    services: Option<NotificationServices>,
    services_start: JoinHandle<()>,
    state: PopupState,
    rows: HashMap<u32, Entry>,
    surface_edge: NotificationEdge,
    theme_watch: JoinHandle<()>,
    styles: Styles,
    input_region_pending: Rc<Cell<bool>>,
    input_region_cards: Rc<RefCell<Vec<NotificationCard>>>,
}

#[relm4::component(pub)]
impl SimpleComponent for App {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = gtk4::Window {
            set_visible: false,
            set_decorated: false,
            set_deletable: false,
            set_resizable: false,
            set_overflow: gtk4::Overflow::Visible,
            add_css_class: "notification-popup",

            #[name(stack)]
            gtk4::Box {
                set_orientation: gtk4::Orientation::Vertical,
                set_overflow: gtk4::Overflow::Visible,
                add_css_class: "notification-popup__paint-frame",
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.init_layer_shell();
        root.set_namespace(Some("glimpse-notifications"));
        root.set_layer(Layer::Top);
        root.set_keyboard_mode(KeyboardMode::None);
        root.set_exclusive_zone(0);
        let scheme = color_scheme(init.config.appearance.color_scheme);

        let services_start = start_services(init.config.clone(), sender.clone());
        let actions = ComponentActions::install(
            &relm4::main_application(),
            crate::cli::build::PKG_VERSION,
            init.failure.clone().map_or(Ok(()), Err),
        );
        watch_configuration(init.config.clone(), init.failure, sender.clone());
        let theme_watch = watch_styles(
            &init.config.appearance.theme,
            init.theme.clone(),
            sender.clone(),
        );
        watch_monitors(sender.clone());

        let window = root.clone();
        let widgets = view_output!();
        let styles = Styles::install(scheme);
        let surface_edge = init.config.notifications.edge;
        blur::apply(&window, blurred(&init.config));
        let model = Self {
            actions,
            root: window.clone(),
            stack: widgets.stack.clone(),
            state: PopupState::new(init.config.notifications.clone()),
            config: init.config,
            theme: init.theme,
            services: None,
            services_start,
            rows: HashMap::new(),
            surface_edge,
            theme_watch,
            styles,
            input_region_pending: Rc::new(Cell::new(false)),
            input_region_cards: Rc::new(RefCell::new(Vec::new())),
        };
        model.reload_styles();
        window.present();
        window.set_visible(false);
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, sender: ComponentSender<Self>) {
        match input {
            Input::ServicesReady(services) => match *services {
                Ok(services) => self.connect_services(services, &sender),
                Err(reason) => {
                    tracing::error!(%reason, "cannot serve notifications; exiting");
                    relm4::main_application().quit();
                }
            },
            Input::Notifications(state) => self.notifications(state, &sender),
            Input::NotificationsHealth(state) => self.update_notification_health(state, &sender),
            Input::Compositor(state) => self.compositor(state),
            Input::Session(state) => self.session(state, &sender),
            Input::Config(config) => self.configure(*config, &sender),
            Input::ConfigVerdict(verdict) => self.actions.report(verdict),
            Input::Theme(Reload::Changed(theme)) => {
                self.theme = theme;
                self.reload_styles();
            }
            Input::Theme(Reload::Recovered) => tracing::info!("stylesheet recovered"),
            Input::Theme(Reload::Rejected(reason)) => {
                tracing::error!(%reason, "stylesheet rejected")
            }
            Input::MonitorsChanged => self.place(),
            Input::Expire(id) => {
                if !self
                    .rows
                    .get(&id)
                    .and_then(|entry| entry.timer.countdown)
                    .is_some_and(|countdown| countdown.expired(Instant::now()))
                {
                    return;
                }
                let delta = self.state.hide(id);
                self.apply(delta, &sender);
                if let Some(services) = &self.services {
                    expire(services.notifications.clone(), id);
                }
            }
            Input::Hide(id) => {
                let delta = self.state.hide(id);
                self.apply(delta, &sender);
            }
            Input::Pause(id) => self.pause(id),
            Input::Resume(id) => self.resume(id, &sender),
            Input::Dismiss(id) => {
                let delta = self.state.hide(id);
                self.apply(delta, &sender);
                if let Some(services) = &self.services {
                    dismiss(services.notifications.clone(), id);
                }
            }
            Input::Activate(id) => {
                let record = self.state.record(id).cloned();
                let token = activation_token(&self.root);
                let delta = self.state.hide(id);
                self.apply(delta, &sender);
                if let (Some(services), Some(record)) = (&self.services, record) {
                    focus_and_activate(
                        services.notifications.clone(),
                        services.compositor.clone(),
                        record,
                        token,
                    );
                }
            }
            Input::Invoke(id, action) => {
                let token = activation_token(&self.root);
                if !self.state.record(id).is_some_and(|record| record.resident) {
                    let delta = self.state.hide(id);
                    self.apply(delta, &sender);
                }
                if let Some(services) = &self.services {
                    invoke(services.notifications.clone(), id, action, token);
                }
            }
            Input::Settled(id) => self.settle(id, &sender),
            Input::Finalize(id) => self.finalize(id, &sender),
            Input::ImageLoaded(id, texture) => {
                self.image_loaded(id, &texture, sender.input_sender().clone());
            }
            Input::Allocated => {
                self.update_input_region();
                self.constrain_height(&sender);
            }
        }
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        self.services_start.abort();
        if let Some(services) = self.services.take() {
            relm4::spawn(services.shutdown());
        }
    }
}

impl App {
    fn connect_services(&mut self, services: NotificationServices, sender: &ComponentSender<Self>) {
        services.reconfigure(&self.config);
        let mut notifications = services.notifications.subscribe();
        let mut notification_health = services.notifications.health();
        let mut compositor = services.compositor.subscribe();
        let mut session = services.session.subscribe();

        self.compositor(compositor.borrow_and_update().clone());
        self.session(session.borrow_and_update().clone(), sender);
        self.notifications(notifications.borrow_and_update().clone(), sender);
        self.update_notification_health(notification_health.borrow_and_update().clone(), sender);

        watch_notifications(notifications, sender.clone());
        watch_notification_health(notification_health, sender.clone());
        watch_compositor(compositor, sender.clone());
        watch_session(session, sender.clone());
        self.services = Some(services);
    }

    fn notifications(&mut self, state: NotificationsState, sender: &ComponentSender<Self>) {
        if let Some(list) = state.list {
            let delta = self.state.update(list.notifications);
            self.apply(delta, sender);
        }
        if let Some(dnd) = state.dnd {
            let delta = self.state.set_dnd(dnd.dnd.enabled);
            self.apply(delta, sender);
        }
    }

    fn update_notification_health(&mut self, state: ServiceState, sender: &ComponentSender<Self>) {
        if matches!(state, ServiceState::Stopped { .. }) {
            let delta = self.state.disconnected();
            self.apply(delta, sender);
        }
    }

    fn compositor(&mut self, state: CompositorState) {
        if let Some(outputs) = state.outputs
            && self.state.set_outputs(outputs.outputs)
        {
            self.place();
        }
    }

    fn session(&mut self, state: Option<SessionStatus>, sender: &ComponentSender<Self>) {
        if let Some(state) = state {
            let delta = self.state.set_session(state.locked, state.private);
            self.apply(delta, sender);
        }
    }

    fn configure(&mut self, config: Config, sender: &ComponentSender<Self>) {
        let renamed = config.appearance.theme != self.config.appearance.theme;
        self.config = config;
        if let Some(services) = &self.services {
            services.reconfigure(&self.config);
        }
        let scheme = color_scheme(self.config.appearance.color_scheme);
        self.styles.set_color_scheme(scheme);
        self.styles
            .set_variant(&self.config.appearance.theme_variant);
        self.styles
            .set_animation_speed(self.config.appearance.animation_speed);
        blur::apply(&self.root, blurred(&self.config));
        let delta = self.state.configure(self.config.notifications.clone());
        self.apply(delta, sender);
        if renamed {
            self.theme_watch.abort();
            self.theme_watch = watch_styles(
                &self.config.appearance.theme,
                self.theme.clone(),
                sender.clone(),
            );
        }
        self.place();
    }

    fn apply(&mut self, delta: Delta, sender: &ComponentSender<Self>) {
        self.place();
        if let Some(services) = &self.services {
            for id in delta.skipped {
                expire(services.notifications.clone(), id);
            }
        }
        for id in delta.removed {
            self.remove(id);
        }
        for id in delta.appeared {
            self.insert(id, sender);
        }
        for id in delta.replaced {
            self.refresh(id, sender);
            self.restart(id, sender);
        }
        self.reorder();
        self.place();
        self.root.set_visible(!self.rows.is_empty());
        self.update_input_region();
    }

    fn insert(&mut self, id: u32, sender: &ComponentSender<Self>) {
        let Some(record) = self.state.record(id).cloned() else {
            return;
        };
        let card = NotificationCard::new();
        card.set_overflow(gtk4::Overflow::Visible);
        let frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        frame.set_overflow(gtk4::Overflow::Visible);
        frame.add_css_class("notification-popup__entry");
        frame.append(&card);
        self.connect(id, &card, sender);
        let image = Self::fill(&card, &record, sender.input_sender());
        self.stack.append(&frame);

        let hide_delay = Duration::from_secs(self.state.settings().hide_delay);
        let timer = timer(id, lifetime(&record, hide_delay), sender.clone());
        let animation = animation(&frame);
        animation.connect_done({
            let sender = sender.clone();
            move |animation| {
                if animation.value_to() == 0.0 {
                    sender.input(Input::Finalize(id));
                } else {
                    sender.input(Input::Settled(id));
                }
            }
        });
        self.rows.insert(
            id,
            Entry {
                frame: frame.clone(),
                card,
                timer,
                animation: animation.clone(),
                state: EntryState::Entering,
                image,
            },
        );
        animate_in(&frame, &animation, self.visible_edge());
    }

    fn refresh(&mut self, id: u32, sender: &ComponentSender<Self>) {
        let Some(record) = self.state.record(id).cloned() else {
            return;
        };
        if let Some(entry) = self.rows.get_mut(&id) {
            if let Some(task) = entry.image.take() {
                task.abort();
            }
            entry.image = Self::fill(&entry.card, &record, sender.input_sender());
            announce(&entry.card, &record);
        }
    }

    fn fill(
        card: &NotificationCard,
        record: &NotificationRecord,
        sender: &relm4::Sender<Input>,
    ) -> Option<glib::JoinHandle<()>> {
        let mut notification = from_record(record, gettext("now"));
        notification.icon = Some(notification_icon(
            record.icon.as_deref().unwrap_or(&record.app_id),
        ));
        card.set_notification(&notification);
        card.set_controls_visible(true);
        record
            .image
            .as_ref()
            .map(|path| load_image(record.id, Path::new(path), sender.clone()))
    }

    fn image_loaded(&self, id: u32, texture: &gdk::Texture, sender: relm4::Sender<Input>) {
        let Some(entry) = self
            .rows
            .get(&id)
            .filter(|entry| entry.state.accepts_input())
        else {
            return;
        };
        entry.card.set_image(Some(texture));
        self.update_input_region();
        let wait_for_allocation = Cell::new(true);
        self.root.add_tick_callback(move |_, _| {
            if wait_for_allocation.replace(false) {
                return glib::ControlFlow::Continue;
            }
            let _ = sender.send(Input::Allocated);
            glib::ControlFlow::Break
        });
    }

    fn connect(&self, id: u32, card: &NotificationCard, sender: &ComponentSender<Self>) {
        card.connect_activated({
            let sender = sender.clone();
            move |_| sender.input(Input::Activate(id))
        });
        card.connect_dismissed({
            let sender = sender.clone();
            move |_| sender.input(Input::Dismiss(id))
        });
        card.connect_action_invoked({
            let sender = sender.clone();
            move |_, action| sender.input(Input::Invoke(id, action))
        });

        let right = gtk4::GestureClick::new();
        right.set_button(3);
        right.connect_released({
            let sender = sender.clone();
            move |gesture, _, _, _| {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                sender.input(Input::Hide(id));
            }
        });
        card.add_controller(right);

        let motion = gtk4::EventControllerMotion::new();
        motion.connect_enter({
            let sender = sender.clone();
            move |_, _, _| sender.input(Input::Pause(id))
        });
        motion.connect_leave({
            let sender = sender.clone();
            move |_| sender.input(Input::Resume(id))
        });
        card.add_controller(motion);
    }

    fn remove(&mut self, id: u32) {
        let edge = self.visible_edge();
        let Some(entry) = self.rows.get_mut(&id) else {
            return;
        };
        if entry.state.is_leaving() {
            return;
        }
        entry.state = EntryState::Leaving;
        if let Some(task) = entry.timer.task.take() {
            task.abort();
        }
        animate_out(&entry.frame, &entry.animation, edge);
        self.update_input_region();
    }

    fn settle(&mut self, id: u32, sender: &ComponentSender<Self>) {
        let Some(entry) = self.rows.get_mut(&id) else {
            return;
        };
        if entry.state.is_leaving() {
            return;
        }
        entry.state = EntryState::Visible;
        if let Some(record) = self.state.record(id) {
            announce(&entry.card, record);
        }
        self.update_input_region();
        self.constrain_height(sender);
    }

    fn finalize(&mut self, id: u32, sender: &ComponentSender<Self>) {
        if let Some(entry) = self.rows.remove(&id) {
            self.stack.remove(&entry.frame);
        }
        self.root.set_visible(!self.rows.is_empty());
        self.update_input_region();
        self.constrain_height(sender);
    }

    fn pause(&mut self, id: u32) {
        let Some(entry) = self.rows.get_mut(&id) else {
            return;
        };
        if entry.state.is_leaving() {
            return;
        }
        entry.timer.hovered = true;
        if let Some(countdown) = entry.timer.countdown.as_mut() {
            countdown.pause(Instant::now());
        }
        if let Some(task) = entry.timer.task.take() {
            task.abort();
        }
    }

    fn resume(&mut self, id: u32, sender: &ComponentSender<Self>) {
        let Some(entry) = self.rows.get_mut(&id) else {
            return;
        };
        if entry.state.is_leaving() {
            return;
        }
        entry.timer.hovered = false;
        if let Some(remaining) = entry
            .timer
            .countdown
            .as_mut()
            .and_then(|countdown| countdown.resume(Instant::now()))
        {
            entry.timer.task = Some(timeout(id, remaining, sender.clone()));
        }
    }

    fn restart(&mut self, id: u32, sender: &ComponentSender<Self>) {
        let hide_delay = Duration::from_secs(self.state.settings().hide_delay);
        let lifetime = self
            .state
            .record(id)
            .and_then(|record| lifetime(record, hide_delay));
        let Some(entry) = self.rows.get_mut(&id) else {
            return;
        };
        if let Some(task) = entry.timer.task.take() {
            task.abort();
        }
        let Some(duration) = lifetime else {
            entry.timer.countdown = None;
            return;
        };
        let now = Instant::now();
        let running = match entry.timer.countdown.as_mut() {
            Some(countdown) => countdown.restart(duration, now),
            None => {
                let mut countdown = Countdown::running(duration, now);
                if entry.timer.hovered {
                    countdown.pause(now);
                }
                entry.timer.countdown = Some(countdown);
                !entry.timer.hovered
            }
        };
        if running {
            entry.timer.task = Some(timeout(id, duration, sender.clone()));
        }
    }

    fn reorder(&self) {
        let mut ids = self.state.visible().to_vec();
        if matches!(
            self.visible_edge(),
            NotificationEdge::BottomLeft
                | NotificationEdge::BottomCenter
                | NotificationEdge::BottomRight
        ) {
            ids.reverse();
        }
        let mut previous: Option<gtk4::Widget> = None;
        for id in ids {
            let Some(entry) = self.rows.get(&id) else {
                continue;
            };
            self.stack
                .reorder_child_after(&entry.frame, previous.as_ref());
            previous = Some(entry.frame.clone().upcast());
        }
    }

    fn place(&mut self) {
        let Some(placement) = self.state.placement() else {
            return;
        };
        self.surface_edge = placement.edge;
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            self.root.set_anchor(edge, false);
        }
        for &edge in anchors(placement.edge) {
            self.root.set_anchor(edge, true);
        }
        let monitor = placement.connector.as_deref().and_then(gdk_monitor);
        self.root.set_monitor(monitor.as_ref());
    }

    fn visible_edge(&self) -> NotificationEdge {
        self.state
            .placement()
            .map_or(self.surface_edge, |placement| placement.edge)
    }

    fn reload_styles(&self) {
        let appearance = &self.config.appearance;
        self.styles.load(&self.theme);
        self.styles.set_variant(&appearance.theme_variant);
        self.styles.set_animation_speed(appearance.animation_speed);
        self.update_input_region();
    }

    fn update_input_region(&self) {
        *self.input_region_cards.borrow_mut() = self
            .rows
            .values()
            .filter(|entry| entry.state.accepts_input())
            .map(|entry| entry.card.clone())
            .collect();
        if self.input_region_cards.borrow().is_empty()
            && let Some(surface) = self.root.surface()
        {
            surface.set_input_region(Some(&cairo::Region::create()));
        }
        if self.input_region_pending.replace(true) {
            return;
        }
        let root = self.root.clone();
        let window = root.clone();
        let pending = self.input_region_pending.clone();
        let cards = self.input_region_cards.clone();
        let wait_for_allocation = Cell::new(true);
        root.add_tick_callback(move |_, _| {
            if wait_for_allocation.replace(false) {
                return glib::ControlFlow::Continue;
            }
            let Some(surface) = window.surface() else {
                pending.set(false);
                return glib::ControlFlow::Break;
            };
            let region = cairo::Region::create();
            for card in cards.borrow().iter() {
                let Some(bounds) = card.compute_bounds(window.upcast_ref::<gtk4::Widget>()) else {
                    continue;
                };
                let rectangle = enclosing_rectangle(bounds);
                let _ = region.union_rectangle(&rectangle);
            }
            surface.set_input_region(Some(&region));
            pending.set(false);
            glib::ControlFlow::Break
        });
    }

    fn constrain_height(&mut self, sender: &ComponentSender<Self>) {
        if self.rows.values().any(|entry| entry.state.is_leaving()) {
            return;
        }
        let Some(connector) = self
            .state
            .placement()
            .and_then(|placement| placement.connector.as_deref())
        else {
            return;
        };
        let Some(monitor) = gdk_monitor(connector) else {
            return;
        };
        let Some(id) = height_overflow(
            self.state.visible(),
            self.root.height(),
            monitor.geometry().height(),
        ) else {
            return;
        };
        let delta = self.state.hide(id);
        self.apply(delta, sender);
    }
}

fn timer(id: u32, lifetime: Option<Duration>, sender: ComponentSender<App>) -> Timer {
    Timer {
        countdown: lifetime.map(|lifetime| Countdown::running(lifetime, Instant::now())),
        task: lifetime.map(|lifetime| timeout(id, lifetime, sender)),
        hovered: false,
    }
}

fn lifetime(record: &NotificationRecord, hide_delay: Duration) -> Option<Duration> {
    if record.urgency == NotificationUrgency::Critical {
        return None;
    }
    match u64::try_from(record.expire_timeout) {
        Ok(0) => None,
        Ok(millis) => Some(Duration::from_millis(millis)),
        Err(_) => Some(hide_delay),
    }
}

fn timeout(id: u32, remaining: Duration, sender: ComponentSender<App>) -> JoinHandle<()> {
    relm4::spawn(async move {
        tokio::time::sleep(remaining).await;
        sender.input(Input::Expire(id));
    })
}

fn deadline(now: Instant, duration: Duration) -> Instant {
    now.checked_add(duration).unwrap_or(now)
}

fn animation(frame: &gtk4::Box) -> adw::TimedAnimation {
    let animation = adw::TimedAnimation::new(
        frame,
        0.0,
        1.0,
        glimpse_widgets::duration_ms(),
        adw::PropertyAnimationTarget::new(frame, "opacity"),
    );
    animation.set_easing(adw::Easing::EaseOutCubic);
    animation
}

fn animate_in(frame: &gtk4::Box, animation: &adw::TimedAnimation, edge: NotificationEdge) {
    let class = animation_class(edge);
    frame.add_css_class(class);
    frame.set_opacity(0.0);
    animation.reset();
    animation.set_value_from(0.0);
    animation.set_value_to(1.0);
    let animation = animation.clone();
    frame.add_tick_callback(move |frame, _| {
        if animation.value_to() != 1.0 {
            return glib::ControlFlow::Break;
        }
        if frame.width() == 0 || frame.height() == 0 {
            return glib::ControlFlow::Continue;
        }
        frame.remove_css_class(class);
        animation.play();
        glib::ControlFlow::Break
    });
}

fn animate_out(frame: &gtk4::Box, animation: &adw::TimedAnimation, edge: NotificationEdge) {
    let from = frame.opacity();
    animation.reset();
    animation.set_value_from(from);
    animation.set_value_to(0.0);
    frame.add_css_class(animation_class(edge));
    animation.play();
}

fn animation_class(edge: NotificationEdge) -> &'static str {
    match edge {
        NotificationEdge::TopLeft | NotificationEdge::TopCenter | NotificationEdge::TopRight => {
            "notification-popup__entry--top"
        }
        NotificationEdge::BottomLeft
        | NotificationEdge::BottomCenter
        | NotificationEdge::BottomRight => "notification-popup__entry--bottom",
    }
}

fn announce(card: &NotificationCard, record: &NotificationRecord) {
    let message = match card.body().filter(|body| !body.is_empty()) {
        Some(body) => format!("{}: {}. {body}", record.app_name, record.summary),
        None => format!("{}: {}", record.app_name, record.summary),
    };
    let priority = match record.urgency {
        NotificationUrgency::Critical => gtk4::AccessibleAnnouncementPriority::High,
        NotificationUrgency::Low | NotificationUrgency::Normal | NotificationUrgency::Unknown => {
            gtk4::AccessibleAnnouncementPriority::Medium
        }
    };
    card.announce(&message, priority);
}

fn anchors(edge: NotificationEdge) -> &'static [Edge] {
    match edge {
        NotificationEdge::TopLeft => &[Edge::Top, Edge::Left],
        NotificationEdge::TopCenter => &[Edge::Top],
        NotificationEdge::TopRight => &[Edge::Top, Edge::Right],
        NotificationEdge::BottomLeft => &[Edge::Bottom, Edge::Left],
        NotificationEdge::BottomCenter => &[Edge::Bottom],
        NotificationEdge::BottomRight => &[Edge::Bottom, Edge::Right],
    }
}

fn blurred(config: &Config) -> bool {
    config.appearance.blur
}

fn enclosing_rectangle(bounds: gtk4::graphene::Rect) -> cairo::RectangleInt {
    let left = bounds.x().floor() as i32;
    let top = bounds.y().floor() as i32;
    let right = (bounds.x() + bounds.width()).ceil() as i32;
    let bottom = (bounds.y() + bounds.height()).ceil() as i32;
    cairo::RectangleInt::new(left, top, right - left, bottom - top)
}

fn height_overflow(visible: &[u32], surface_height: i32, output_height: i32) -> Option<u32> {
    (surface_height > output_height && visible.len() > 1)
        .then(|| visible.last().copied())
        .flatten()
}

fn gdk_monitor(connector: &str) -> Option<gdk::Monitor> {
    let display = gdk::Display::default()?;
    let monitors = display.monitors();
    (0..monitors.n_items())
        .filter_map(|index| monitors.item(index).and_downcast::<gdk::Monitor>())
        .find(|monitor| monitor.connector().as_deref() == Some(connector))
}

fn expire(notifications: NotificationsHandle, id: u32) {
    relm4::spawn(async move {
        if let Err(error) = notifications.expire(id).await {
            tracing::warn!(%error, "notification expiry failed");
        }
    });
}

fn dismiss(notifications: NotificationsHandle, id: u32) {
    relm4::spawn(async move {
        if let Err(error) = notifications.dismiss(id).await {
            tracing::warn!(%error, "notification dismiss failed");
        }
    });
}

fn focus_and_activate(
    notifications: NotificationsHandle,
    compositor: CompositorHandle,
    record: NotificationRecord,
    activation_token: Option<String>,
) {
    relm4::spawn(async move {
        match tokio::time::timeout(
            Duration::from_secs(5),
            compositor.focus_app(record.app_pid, &[&record.app_id, &record.app_name]),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(%error, app_id = record.app_id, "notification window focus failed")
            }
            Err(_) => tracing::warn!("notification window focus timed out"),
        }
        if let Err(error) = notifications.activate(record.id, activation_token).await {
            tracing::warn!(%error, "notification activation failed");
        }
    });
}

fn invoke(
    notifications: NotificationsHandle,
    id: u32,
    action: String,
    activation_token: Option<String>,
) {
    relm4::spawn(async move {
        if let Err(error) = notifications
            .invoke_action(id, action, activation_token)
            .await
        {
            tracing::warn!(%error, "notification action failed");
        }
    });
}

fn start_services(config: Config, sender: ComponentSender<App>) -> JoinHandle<()> {
    relm4::spawn(async move {
        let started = NotificationServices::start(&config)
            .await
            .map_err(|error| format!("{error:#}"));
        sender.input(Input::ServicesReady(Box::new(started)));
    })
}

fn watch_notifications(
    mut states: tokio::sync::watch::Receiver<NotificationsState>,
    sender: ComponentSender<App>,
) {
    relm4::spawn(async move {
        while states.changed().await.is_ok() {
            sender.input(Input::Notifications(states.borrow_and_update().clone()));
        }
    });
}

fn watch_notification_health(
    mut states: tokio::sync::watch::Receiver<ServiceState>,
    sender: ComponentSender<App>,
) {
    relm4::spawn(async move {
        while states.changed().await.is_ok() {
            sender.input(Input::NotificationsHealth(
                states.borrow_and_update().clone(),
            ));
        }
    });
}

fn watch_compositor(
    mut states: tokio::sync::watch::Receiver<CompositorState>,
    sender: ComponentSender<App>,
) {
    relm4::spawn(async move {
        while states.changed().await.is_ok() {
            sender.input(Input::Compositor(states.borrow_and_update().clone()));
        }
    });
}

fn watch_session(
    mut states: tokio::sync::watch::Receiver<Option<SessionStatus>>,
    sender: ComponentSender<App>,
) {
    relm4::spawn(async move {
        while states.changed().await.is_ok() {
            sender.input(Input::Session(states.borrow_and_update().clone()));
        }
    });
}

fn watch_monitors(sender: ComponentSender<App>) {
    let monitor_sender = sender.input_sender().clone();
    glimpse_widgets::watch_monitors(move || {
        let _ = monitor_sender.send(Input::MonitorsChanged);
    });
}

fn watch_configuration(current: Config, failure: Option<String>, sender: ComponentSender<App>) {
    relm4::spawn(async move {
        let mut configs = Box::pin(watch_config(Documents::Config, current, failure));
        while let Some(reload) = configs.next().await {
            let verdict = match reload {
                Reload::Changed(config) => {
                    sender.input(Input::Config(Box::new(config)));
                    Ok(())
                }
                Reload::Recovered => Ok(()),
                Reload::Rejected(reason) => Err(reason),
            };
            sender.input(Input::ConfigVerdict(verdict));
        }
    });
}

fn watch_styles(name: &str, current: Theme, sender: ComponentSender<App>) -> JoinHandle<()> {
    let themes = watch_theme(name, PANEL_STYLESHEET, current);
    relm4::spawn(async move {
        let mut themes = Box::pin(themes);
        while let Some(reload) = themes.next().await {
            sender.input(Input::Theme(reload));
        }
    })
}

fn activation_token(window: &gtk4::Window) -> Option<String> {
    let id = WidgetExt::display(window)
        .app_launch_context()
        .startup_notify_id(gio::AppInfo::NONE, &[])?;
    (!id.is_empty()).then(|| id.to_string())
}

fn load_image(id: u32, path: &Path, sender: relm4::Sender<Input>) -> glib::JoinHandle<()> {
    let path = path.to_path_buf();
    glib::spawn_future_local(async move {
        match gio::spawn_blocking(move || notification_image(&path)).await {
            Ok(Some(texture)) => {
                let _ = sender.send(Input::ImageLoaded(id, texture));
            }
            Ok(None) => {}
            Err(_) => tracing::debug!("notification image load panicked"),
        }
    })
}

fn from_record(record: &NotificationRecord, when: String) -> Notification {
    let active = record.unread;
    Notification {
        key: record.id.to_string(),
        app_name: record.app_name.clone(),
        summary: record.summary.clone(),
        body: record.body.clone().map(Body::Markup),
        when,
        urgency: match record.urgency {
            NotificationUrgency::Critical => Urgency::Critical,
            _ => Urgency::Normal,
        },
        actions: if active {
            record
                .actions
                .iter()
                .filter(|action| action.key != DEFAULT_ACTION)
                .map(|action| Action {
                    key: action.key.clone(),
                    label: action.label.clone(),
                })
                .collect()
        } else {
            Vec::new()
        },
        progress: record.progress,
        unread: active,
        activatable: active,
        ..Notification::default()
    }
}

#[cfg(test)]
mod tests;
