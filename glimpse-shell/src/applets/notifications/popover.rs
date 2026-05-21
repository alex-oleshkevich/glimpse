use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

use crate::{
    components::{
        animated_popover::AnimatedPopover, hero::HeroView, popover_scroll,
        popover_shell::PopoverShell,
    },
    services::notifications::model::NotificationEntry,
    widgets::message::Message,
};
use glimpse_core::services::notifications::model::State as NotificationState;

use super::format;

pub struct Popover {
    animation: AnimatedPopover,
    refresh_timer: Option<glib::SourceId>,
    notifications: Vec<NotificationEntry>,
    dnd: bool,
    rows: HashMap<u32, Message>,
    list: gtk::Box,
    scroller: gtk::ScrolledWindow,
    empty: gtk::Box,
    hero_icon: gtk::Image,
    hero_subtitle: gtk::Label,
    hero_toggle: gtk::Switch,
    updating_dnd: Rc<Cell<bool>>,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    Update {
        notifications: Vec<NotificationEntry>,
        dnd: bool,
    },
    Dismiss(u32),
    DismissAll,
    SetDnd(bool),
    FocusAndDismiss(u32),
    RefreshTimes,
    InvokeAction {
        id: u32,
        action_key: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopoverOutput {
    Opened,
    Closed,
    Dismiss(u32),
    DismissAll,
    SetDnd(bool),
    FocusAndDismiss(u32),
    InvokeAction { id: u32, action_key: String },
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = gtk::Popover {
            add_css_class: "notifications-popover",
            add_css_class: "popover-size-xxlarge",
            set_hexpand: false,

            #[template]
            PopoverShell {
                #[template_child]
                content {
                    #[name = "hero"]
                    #[template]
                    HeroView {},

                    gtk::Separator {
                        set_orientation: gtk::Orientation::Horizontal,
                    },

                    #[name = "empty"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 12,
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        set_vexpand: true,
                        set_hexpand: true,
                        add_css_class: "empty-state",
                        add_css_class: "empty-state--notifications",

                        gtk::Image {
                            add_css_class: "empty-state__icon",
                            set_icon_name: Some("preferences-system-notifications-symbolic"),
                            set_pixel_size: 64,
                        },

                        gtk::Label {
                            add_css_class: "empty-state__title",
                            set_label: "No notifications",
                        },

                        gtk::Label {
                            add_css_class: "empty-state__subtitle",
                            set_label: "You're caught up.",
                        },
                    },

                    #[name = "scroller"]
                    gtk::ScrolledWindow {
                        set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                        set_vexpand: true,
                        set_propagate_natural_height: true,

                        #[name = "list"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 4,
                            add_css_class: "notification-list",
                        }
                    },
                },

                #[template_child]
                footer {
                    gtk::Button {
                        add_css_class: "flat",
                        add_css_class: "footer-action",
                        set_label: "Clear All",
                        connect_clicked => PopoverInput::DismissAll,
                    }
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);
        popover_scroll::install_half_monitor_limit(&widgets.root, &widgets.scroller, &init.parent);

        let opened_sender = _sender.clone();
        widgets.root.connect_show(move |_| {
            let _ = opened_sender.output(PopoverOutput::Opened);
        });

        let closed_sender = _sender.clone();
        widgets.root.connect_closed(move |_| {
            let _ = closed_sender.output(PopoverOutput::Closed);
        });

        widgets
            .hero
            .icon
            .set_icon_name(Some(notification_popover_icon_name(false)));
        widgets.hero.title.set_label("Notifications");
        widgets.hero.subtitle.set_label("No notifications");
        widgets.hero.trailing.set_visible(true);

        let updating_dnd = Rc::new(Cell::new(false));
        widgets.hero.toggle.connect_state_set({
            let sender = _sender.clone();
            let updating_dnd = updating_dnd.clone();
            move |_, active| {
                if !updating_dnd.get() {
                    sender.input(PopoverInput::SetDnd(!active));
                }
                glib::Propagation::Proceed
            }
        });

        let refresh_timer = glib::timeout_add_seconds_local(60, {
            let sender = _sender.input_sender().clone();
            let root = widgets.root.clone();
            move || {
                if root.is_visible() {
                    let _ = sender.send(PopoverInput::RefreshTimes);
                }
                glib::ControlFlow::Continue
            }
        });

        let model = Popover {
            animation: AnimatedPopover::new(&widgets.root),
            refresh_timer: Some(refresh_timer),
            notifications: Vec::new(),
            dnd: false,
            rows: HashMap::new(),
            list: widgets.list.clone(),
            scroller: widgets.scroller.clone(),
            empty: widgets.empty.clone(),
            hero_icon: widgets.hero.icon.clone(),
            hero_subtitle: widgets.hero.subtitle.clone(),
            hero_toggle: widgets.hero.toggle.clone(),
            updating_dnd,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => self.animation.toggle(),
            PopoverInput::Update { notifications, dnd } => {
                self.notifications = notifications;
                self.dnd = dnd;
                self.sync(&sender);
            }
            PopoverInput::Dismiss(id) => {
                let _ = sender.output(PopoverOutput::Dismiss(id));
            }
            PopoverInput::DismissAll => {
                let _ = sender.output(PopoverOutput::DismissAll);
            }
            PopoverInput::SetDnd(enabled) => {
                let _ = sender.output(PopoverOutput::SetDnd(enabled));
            }
            PopoverInput::FocusAndDismiss(id) => {
                let _ = sender.output(PopoverOutput::FocusAndDismiss(id));
            }
            PopoverInput::RefreshTimes => self.refresh_times(),
            PopoverInput::InvokeAction { id, action_key } => {
                let _ = sender.output(PopoverOutput::InvokeAction { id, action_key });
            }
        }
    }
}

impl Popover {
    fn sync(&mut self, sender: &ComponentSender<Self>) {
        let now = format::now_ms();
        self.empty.set_visible(self.notifications.is_empty());
        self.scroller.set_visible(!self.notifications.is_empty());
        self.hero_icon
            .set_icon_name(Some(notification_popover_icon_name(self.dnd)));
        let subtitle = if self.dnd {
            "Do Not Disturb".into()
        } else {
            format::count_label(self.notifications.len())
        };
        self.hero_subtitle.set_label(&subtitle);
        self.updating_dnd.set(true);
        self.hero_toggle.set_active(!self.dnd);
        self.updating_dnd.set(false);

        let rows = &mut self.rows;
        let list = &self.list;
        let mut seen: HashSet<u32> = HashSet::new();
        let mut previous: Option<gtk::Widget> = None;
        for notification in &self.notifications {
            let id = notification.id;
            seen.insert(id);
            let msg = rows
                .entry(id)
                .or_insert_with(|| {
                    let msg = Message::new();
                    wire_signals(&msg, id, sender);
                    list.append(&msg);
                    msg
                })
                .clone();
            update_row(&msg, notification, now);
            list.reorder_child_after(&msg, previous.as_ref());
            previous = Some(msg.upcast());
        }

        self.rows.retain(|id, msg| {
            let keep = seen.contains(id);
            if !keep {
                list.remove(msg);
            }
            keep
        });
    }

    fn refresh_times(&self) {
        let now = format::now_ms();
        for notification in &self.notifications {
            if let Some(msg) = self.rows.get(&notification.id) {
                msg.set_time(&format::relative_time(now, notification.timestamp));
            }
        }
    }
}

impl Drop for Popover {
    fn drop(&mut self) {
        if let Some(refresh_timer) = self.refresh_timer.take() {
            refresh_timer.remove();
        }
    }
}

fn wire_signals(msg: &Message, id: u32, sender: &ComponentSender<Popover>) {
    let s = sender.clone();
    msg.connect_closed(move |_| s.input(PopoverInput::Dismiss(id)));
    let s = sender.clone();
    msg.connect_clicked(move |_| s.input(PopoverInput::FocusAndDismiss(id)));
    let s = sender.clone();
    msg.connect_secondary_clicked(move |_| s.input(PopoverInput::Dismiss(id)));
    let s = sender.clone();
    msg.connect_action(move |_, action_key| {
        s.input(PopoverInput::InvokeAction {
            id,
            action_key: action_key.to_owned(),
        });
    });
}

fn update_row(msg: &Message, notification: &NotificationEntry, now: u64) {
    if notification.urgency == 2 {
        msg.add_css_class("message--critical");
    } else {
        msg.remove_css_class("message--critical");
    }
    msg.set_icon(Some(format::app_icon(notification)));
    msg.set_app_name(format::source_name(notification));
    msg.set_time(&format::relative_time(now, notification.timestamp));
    msg.set_title(&notification.summary);
    msg.set_body(&notification.body);
    msg.set_content_paintable(format::load_image(notification).as_ref());

    msg.clear_actions();
    for (action_key, label) in format::visible_actions(notification) {
        msg.add_action(action_key, label);
    }
}

fn notification_popover_icon_name(dnd: bool) -> &'static str {
    format::icon_name(&NotificationState {
        dnd,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popover_uses_available_notification_icon_names() {
        assert_eq!(
            notification_popover_icon_name(false),
            "preferences-system-notifications-symbolic"
        );
        assert_eq!(
            notification_popover_icon_name(true),
            "notifications-disabled-symbolic"
        );
    }
}
