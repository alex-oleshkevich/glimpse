use std::collections::{HashMap, HashSet};

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

use crate::{
    services::notifications::model::NotificationEntry,
    utils::popover_scroll,
    widgets::{
        animated_popover::AnimatedPopover,
        hero::Hero,
        message::Message,
        message_group::{MessageGroup, MessageGroupState},
        popover_shell::PopoverShell,
    },
};
use glimpse_core::services::notifications::model::State as NotificationState;

use super::{
    components::{NotificationListItem, notification_items},
    format,
};

pub struct Popover {
    popover: AnimatedPopover,
    refresh_timer: Option<glib::SourceId>,
    notifications: Vec<NotificationEntry>,
    dnd: bool,
    subtitle: String,
    rows: HashMap<u32, Message>,
    groups: HashMap<String, MessageGroup>,
    list: gtk::Box,
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
        root = AnimatedPopover {
            add_css_class: "popover-size-xlarge",

            #[name = "shell"]
            PopoverShell {
                Hero {
                        set_title: "Notifications",
                        set_trailing_visible: true,
                        #[watch]
                        set_icon: Some(notification_popover_icon_name(model.dnd)),
                        #[watch]
                        set_subtitle: &model.subtitle,
                        #[watch]
                        set_toggle_active: !model.dnd,
                        connect_toggled[sender] => move |_, state| {
                            sender.input(PopoverInput::SetDnd(!state));
                        },
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 12,
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        set_vexpand: true,
                        set_hexpand: true,
                        add_css_class: "empty-state",
                        add_css_class: "empty-state--notifications",
                        #[watch]
                        set_visible: model.notifications.is_empty(),

                        gtk::Image {
                            add_css_class: "empty-state__icon",
                            #[watch]
                            set_icon_name: Some(notification_popover_icon_name(model.dnd)),
                            set_pixel_size: 64,
                        },

                        gtk::Label {
                            add_css_class: "empty-state__title",
                            #[watch]
                            set_label: if model.dnd { "Do Not Disturb" } else { "No notifications" },
                        },

                        gtk::Label {
                            add_css_class: "empty-state__subtitle",
                            #[watch]
                            set_label: if model.dnd {
                                "Notifications are silenced."
                            } else {
                                "You're caught up."
                            },
                        },
                    },

                #[name = "scroller"]
                gtk::ScrolledWindow {
                    set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                    set_vexpand: true,
                    set_propagate_natural_height: true,
                    #[watch]
                    set_visible: !model.notifications.is_empty(),

                    #[name = "list"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 12,
                        add_css_class: "notification-list",
                    }
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = Popover {
            popover: AnimatedPopover::new(),
            refresh_timer: None,
            notifications: Vec::new(),
            dnd: false,
            subtitle: format::count_label(0),
            rows: HashMap::new(),
            groups: HashMap::new(),
            list: gtk::Box::new(gtk::Orientation::Vertical, 0),
        };

        let widgets = view_output!();
        model.popover = widgets.root.clone();
        model.list = widgets.list.clone();

        let clear_all = gtk::Button::new();
        clear_all.add_css_class("flat");
        clear_all.add_css_class("footer-action");
        clear_all.set_label("Clear All");
        {
            let input = sender.input_sender().clone();
            clear_all.connect_clicked(move |_| {
                let _ = input.send(PopoverInput::DismissAll);
            });
        }
        widgets.shell.footer().append(&clear_all);
        widgets.shell.set_footer_visible(true);

        widgets.root.set_parent(&init.parent);
        popover_scroll::install_half_monitor_limit(
            widgets.root.upcast_ref::<gtk::Popover>(),
            &widgets.scroller,
            &init.parent,
        );

        model.refresh_timer = Some(glib::timeout_add_seconds_local(60, {
            let input_sender = sender.input_sender().clone();
            let root = widgets.root.clone();
            move || {
                if root.is_visible() {
                    let _ = input_sender.send(PopoverInput::RefreshTimes);
                }
                glib::ControlFlow::Continue
            }
        }));

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => self.popover.toggle(),
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
                if let Some(group) = self.collapsed_group_with_lead(id) {
                    group.set_state(MessageGroupState::Expanded);
                } else {
                    let _ = sender.output(PopoverOutput::FocusAndDismiss(id));
                }
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
        self.subtitle = if self.dnd {
            "Do Not Disturb".into()
        } else {
            format::count_label(self.notifications.len())
        };

        let items = notification_items(&self.notifications);
        let mut seen_rows: HashSet<u32> = HashSet::new();
        let mut seen_groups: HashSet<String> = HashSet::new();
        let mut previous: Option<gtk::Widget> = None;

        for item in &items {
            match item {
                NotificationListItem::Notification(notification) => {
                    let id = notification.id;
                    seen_rows.insert(id);
                    let msg = self.ensure_row(id, sender);
                    update_row(&msg, notification, now);
                    reparent_into(&msg, &self.list);
                    self.list.reorder_child_after(&msg, previous.as_ref());
                    previous = Some(msg.upcast());
                }
                NotificationListItem::Group(group_model) => {
                    seen_groups.insert(group_model.key.clone());
                    let mut member_msgs: Vec<Message> =
                        Vec::with_capacity(group_model.notifications.len());
                    for notification in &group_model.notifications {
                        seen_rows.insert(notification.id);
                        let msg = self.ensure_row(notification.id, sender);
                        update_row(&msg, notification, now);
                        member_msgs.push(msg);
                    }

                    let group_widget = self.ensure_group(&group_model.key).clone();
                    group_widget.set_app_icon(Some(&group_model.icon));
                    group_widget.set_app_name(&group_model.app_name);
                    group_widget.set_messages(member_msgs);

                    reparent_into(&group_widget, &self.list);
                    self.list
                        .reorder_child_after(&group_widget, previous.as_ref());
                    previous = Some(group_widget.upcast());
                }
            }
        }

        let list = &self.list;
        self.rows.retain(|id, msg| {
            let keep = seen_rows.contains(id);
            if !keep
                && let Some(parent) = msg.parent()
                && let Some(parent_box) = parent.downcast_ref::<gtk::Box>()
            {
                parent_box.remove(msg);
            }
            keep
        });
        self.groups.retain(|key, widget| {
            let keep = seen_groups.contains(key);
            if !keep {
                list.remove(widget);
            }
            keep
        });
    }

    fn ensure_row(&mut self, id: u32, sender: &ComponentSender<Self>) -> Message {
        if let Some(msg) = self.rows.get(&id) {
            return msg.clone();
        }
        let msg = Message::new();
        wire_signals(&msg, id, sender);
        self.rows.insert(id, msg.clone());
        msg
    }

    fn ensure_group(&mut self, key: &str) -> &MessageGroup {
        if !self.groups.contains_key(key) {
            let widget = MessageGroup::new();
            self.list.append(&widget);
            self.groups.insert(key.to_owned(), widget);
        }
        self.groups.get(key).expect("just inserted")
    }

    fn refresh_times(&self) {
        let now = format::now_ms();
        for notification in &self.notifications {
            if let Some(msg) = self.rows.get(&notification.id) {
                msg.set_time(&format::relative_time(now, notification.timestamp));
            }
        }
    }

    /// Find a collapsed MessageGroup whose lead notification id matches `id`.
    fn collapsed_group_with_lead(&self, id: u32) -> Option<MessageGroup> {
        for item in notification_items(&self.notifications) {
            if let NotificationListItem::Group(group_model) = item
                && group_model.lead.id == id
                && let Some(widget) = self.groups.get(&group_model.key)
                && widget.state() == MessageGroupState::Collapsed
            {
                return Some(widget.clone());
            }
        }
        None
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

/// Re-parent a widget into `target`. If it already lives in `target`, no-op.
/// If it lives in another Box, detach first.
fn reparent_into(widget: &impl IsA<gtk::Widget>, target: &gtk::Box) {
    let widget_ref = widget.as_ref();
    if let Some(parent) = widget_ref.parent() {
        if parent == *target.upcast_ref::<gtk::Widget>() {
            return;
        }
        if let Some(parent_box) = parent.downcast_ref::<gtk::Box>() {
            parent_box.remove(widget_ref);
        }
    }
    target.append(widget_ref);
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
