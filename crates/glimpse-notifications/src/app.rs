use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use adw::prelude::*;
use futures_util::StreamExt;
use gettextrs::gettext;
use glimpse_config::{
    Config, DARK_STYLESHEET, NotificationEdge, PANEL_STYLESHEET, stylesheet, user_dark_stylesheet,
    user_stylesheet, watch_config, watch_theme,
};
use glimpse_dbus::notifications::{NotificationRecord, NotificationUrgency};
use glimpse_services::{
    CompositorHandle, CompositorState, NotificationsHandle, NotificationsState, ServiceState,
};
use glimpse_services::{SessionStatus, WindowRef};
use glimpse_widgets::{
    Action, Body, Notification, NotificationCard, Sheets, Styles, Urgency, notification_image,
};
use gtk4::{cairo, gdk, gio, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::{ComponentParts, ComponentSender, SimpleComponent};
use tokio::{task::JoinHandle, time::Instant};

use crate::services::NotificationServices;
use crate::state::{Delta, PopupState};
use glimpse_dbus::notifications::DEFAULT_ACTION;

const ANIMATION_MILLIS: u32 = 150;

pub struct Init {
    pub config: Config,
    pub config_path: Option<PathBuf>,
}

#[derive(Debug)]
pub enum Input {
    ServicesReady(Box<Result<NotificationServices, String>>),
    Notifications(NotificationsState),
    NotificationsHealth(ServiceState),
    Compositor(CompositorState),
    Session(Option<SessionStatus>),
    Config(Box<Config>),
    Theme,
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
}

struct Timer {
    countdown: Countdown,
    task: Option<JoinHandle<()>>,
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
}

pub struct App {
    root: gtk4::Window,
    stack: gtk4::Box,
    config: Config,
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
        watch_configuration(init.config_path, init.config.clone(), sender.clone());
        let theme_watch = watch_styles(&init.config.appearance.theme, sender.clone());
        watch_monitors(sender.clone());

        let window = root.clone();
        let widgets = view_output!();
        let styles = Styles::install(scheme);
        let surface_edge = init.config.notifications.edge;
        let model = Self {
            root: window.clone(),
            stack: widgets.stack.clone(),
            state: PopupState::new(init.config.notifications.clone()),
            config: init.config,
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
            Input::NotificationsHealth(state) => {
                if matches!(state, ServiceState::Stopped { .. }) {
                    let delta = self.state.disconnected();
                    self.apply(delta, &sender);
                }
            }
            Input::Compositor(state) => self.compositor(state),
            Input::Session(state) => self.session(state, &sender),
            Input::Config(config) => self.configure(*config, &sender),
            Input::Theme => self.reload_styles(),
            Input::MonitorsChanged => self.place(),
            Input::Expire(id) => {
                if !self
                    .rows
                    .get(&id)
                    .is_some_and(|entry| entry.timer.countdown.expired(Instant::now()))
                {
                    return;
                }
                let delta = self.state.hide(id);
                self.apply(delta, &sender);
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
                let delta = self.state.hide(id);
                self.apply(delta, &sender);
                if let Some(services) = &self.services {
                    focus_and_dismiss(
                        services.notifications.clone(),
                        services.compositor.clone(),
                        id,
                        record.and_then(|record| record.app_pid),
                    );
                }
            }
            Input::Invoke(id, action) => {
                let delta = self.state.hide(id);
                self.apply(delta, &sender);
                if let Some(services) = &self.services {
                    invoke_and_dismiss(
                        services.notifications.clone(),
                        id,
                        action,
                        activation_token(&self.root),
                    );
                }
            }
            Input::Settled(id) => self.settle(id, &sender),
            Input::Finalize(id) => self.finalize(id, &sender),
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
        glimpse_utils::report_language_change(
            self.config.regional.language(),
            config.regional.language(),
        );
        self.config = config;
        if let Some(services) = &self.services {
            services.reconfigure(&self.config);
        }
        let scheme = color_scheme(self.config.appearance.color_scheme);
        self.styles.set_color_scheme(scheme);
        self.styles
            .set_variant(&self.config.appearance.theme_variant);
        let delta = self.state.configure(self.config.notifications.clone());
        self.apply(delta, sender);
        if renamed {
            self.theme_watch.abort();
            self.theme_watch = watch_styles(&self.config.appearance.theme, sender.clone());
            self.reload_styles();
        }
        self.place();
    }

    fn apply(&mut self, delta: Delta, sender: &ComponentSender<Self>) {
        self.place();
        for id in delta.removed {
            if delta.immediate {
                self.discard(id);
            } else {
                self.remove(id);
            }
        }
        for id in delta.appeared {
            self.insert(id, sender);
        }
        for id in delta.replaced {
            self.refresh(id);
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
        self.fill(&card, &record);
        self.stack.append(&frame);

        let remaining = Duration::from_secs(self.state.settings().hide_delay);
        let timer = timer(id, remaining, sender.clone());
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
            },
        );
        animate_in(&frame, &animation, self.visible_edge());
    }

    fn refresh(&mut self, id: u32) {
        let Some(record) = self.state.record(id).cloned() else {
            return;
        };
        if let Some(entry) = self.rows.get(&id) {
            self.fill(&entry.card, &record);
            announce(&entry.card, &record);
        }
    }

    fn fill(&self, card: &NotificationCard, record: &NotificationRecord) {
        let mut notification = from_record(record, gettext("now"));
        notification.icon = record
            .icon
            .as_deref()
            .map(|name| gio::ThemedIcon::new(name).upcast());
        notification.image = record
            .image
            .as_deref()
            .and_then(|path| notification_image(Path::new(path)));
        card.set_notification(&notification);
        card.set_controls_visible(true);
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

    fn discard(&mut self, id: u32) {
        if let Some(mut entry) = self.rows.remove(&id) {
            if let Some(task) = entry.timer.task.take() {
                task.abort();
            }
            self.stack.remove(&entry.frame);
        }
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
        if let Some(mut entry) = self.rows.remove(&id) {
            if let Some(task) = entry.timer.task.take() {
                task.abort();
            }
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
        entry.timer.countdown.pause(Instant::now());
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
        if let Some(remaining) = entry.timer.countdown.resume(Instant::now()) {
            entry.timer.task = Some(timeout(id, remaining, sender.clone()));
        }
    }

    fn restart(&mut self, id: u32, sender: &ComponentSender<Self>) {
        let Some(entry) = self.rows.get_mut(&id) else {
            return;
        };
        if let Some(task) = entry.timer.task.take() {
            task.abort();
        }
        let duration = Duration::from_secs(self.state.settings().hide_delay);
        if entry.timer.countdown.restart(duration, Instant::now()) {
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
        self.styles.load(&Sheets {
            theme: stylesheet(&appearance.theme, PANEL_STYLESHEET),
            theme_dark: stylesheet(&appearance.theme, DARK_STYLESHEET),
            dropin: user_stylesheet(),
            dropin_dark: user_dark_stylesheet(),
        });
        self.styles.set_variant(&appearance.theme_variant);
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

fn timer(id: u32, remaining: Duration, sender: ComponentSender<App>) -> Timer {
    Timer {
        countdown: Countdown::running(remaining, Instant::now()),
        task: Some(timeout(id, remaining, sender)),
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
        ANIMATION_MILLIS,
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
    animation.reset();
    animation.set_value_from(frame.opacity());
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

fn dismiss(notifications: NotificationsHandle, id: u32) {
    relm4::spawn(async move {
        if let Err(error) = notifications.dismiss(id).await {
            tracing::warn!(%error, "notification dismiss failed");
        }
    });
}

fn focus_and_dismiss(
    notifications: NotificationsHandle,
    compositor: CompositorHandle,
    id: u32,
    pid: Option<i32>,
) {
    relm4::spawn(async move {
        if let Some(pid) = pid {
            match tokio::time::timeout(
                Duration::from_secs(5),
                compositor.focus_window(WindowRef::Pid { pid }),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => tracing::warn!(%error, "notification window focus failed"),
                Err(_) => tracing::warn!("notification window focus timed out"),
            }
        }
        if let Err(error) = notifications.dismiss(id).await {
            tracing::warn!(%error, "notification dismiss failed");
        }
    });
}

fn invoke_and_dismiss(
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
        if let Err(error) = notifications.dismiss(id).await {
            tracing::warn!(%error, "notification dismiss failed");
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

fn watch_configuration(path: Option<PathBuf>, current: Config, sender: ComponentSender<App>) {
    relm4::spawn(async move {
        let mut configs = Box::pin(watch_config(path, current));
        while let Some(config) = configs.next().await {
            sender.input(Input::Config(Box::new(config)));
        }
    });
}

fn watch_styles(theme: &str, sender: ComponentSender<App>) -> JoinHandle<()> {
    let themes = watch_theme(theme);
    relm4::spawn(async move {
        let mut themes = Box::pin(themes);
        while themes.next().await.is_some() {
            sender.input(Input::Theme);
        }
    })
}

fn activation_token(window: &gtk4::Window) -> Option<String> {
    let id = WidgetExt::display(window)
        .app_launch_context()
        .startup_notify_id(gio::AppInfo::NONE, &[])?;
    (!id.is_empty()).then(|| id.to_string())
}

fn color_scheme(scheme: glimpse_config::ColorScheme) -> adw::ColorScheme {
    match scheme {
        glimpse_config::ColorScheme::Light => adw::ColorScheme::ForceLight,
        glimpse_config::ColorScheme::Dark => adw::ColorScheme::ForceDark,
        glimpse_config::ColorScheme::Auto => adw::ColorScheme::Default,
    }
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
mod tests {
    use super::*;

    #[test]
    fn each_edge_has_the_expected_anchors_and_vertical_motion() {
        for (edge, expected, class) in [
            (
                NotificationEdge::TopLeft,
                &[Edge::Top, Edge::Left][..],
                "notification-popup__entry--top",
            ),
            (
                NotificationEdge::TopCenter,
                &[Edge::Top][..],
                "notification-popup__entry--top",
            ),
            (
                NotificationEdge::TopRight,
                &[Edge::Top, Edge::Right][..],
                "notification-popup__entry--top",
            ),
            (
                NotificationEdge::BottomLeft,
                &[Edge::Bottom, Edge::Left][..],
                "notification-popup__entry--bottom",
            ),
            (
                NotificationEdge::BottomCenter,
                &[Edge::Bottom][..],
                "notification-popup__entry--bottom",
            ),
            (
                NotificationEdge::BottomRight,
                &[Edge::Bottom, Edge::Right][..],
                "notification-popup__entry--bottom",
            ),
        ] {
            assert_eq!(anchors(edge), expected);
            assert_eq!(animation_class(edge), class);
        }
    }

    #[test]
    fn height_overflow_hides_only_the_oldest_and_keeps_one_card() {
        assert_eq!(height_overflow(&[3, 2, 1], 901, 900), Some(1));
        assert_eq!(height_overflow(&[3, 2, 1], 900, 900), None);
        assert_eq!(height_overflow(&[1], 901, 900), None);
    }

    #[test]
    fn input_region_encloses_fractional_card_bounds() {
        let rectangle = enclosing_rectangle(gtk4::graphene::Rect::new(0.75, 1.25, 100.75, 50.5));
        assert_eq!(
            (
                rectangle.x(),
                rectangle.y(),
                rectangle.width(),
                rectangle.height(),
            ),
            (0, 1, 102, 51)
        );
    }

    #[test]
    fn appearing_and_visible_cards_accept_input() {
        assert!(EntryState::Entering.accepts_input());
        assert!(EntryState::Visible.accepts_input());
        assert!(!EntryState::Leaving.accepts_input());
    }

    #[test]
    fn an_unrepresentable_deadline_expires_safely() {
        let now = Instant::now();
        assert_eq!(deadline(now, Duration::MAX), now);
    }

    #[test]
    fn countdowns_pause_independently_and_replacements_preserve_hover() {
        let start = Instant::now();
        let mut first = Countdown::running(Duration::from_secs(4), start);
        let mut second = Countdown::running(Duration::from_secs(4), start);

        first.pause(start + Duration::from_secs(1));
        second.pause(start + Duration::from_secs(2));
        assert_eq!(first.remaining, Duration::from_secs(3));
        assert_eq!(second.remaining, Duration::from_secs(2));

        assert!(!first.restart(Duration::from_secs(8), start));
        assert_eq!(
            first.resume(start + Duration::from_secs(5)),
            Some(Duration::from_secs(8))
        );
        assert!(first.restart(Duration::from_secs(6), start));
        assert_eq!(first.deadline, Some(start + Duration::from_secs(6)));
        assert!(!first.expired(start + Duration::from_secs(4)));
        assert!(first.expired(start + Duration::from_secs(6)));
        assert_eq!(second.deadline, None);
        assert!(!second.expired(start + Duration::from_secs(20)));
    }
}
