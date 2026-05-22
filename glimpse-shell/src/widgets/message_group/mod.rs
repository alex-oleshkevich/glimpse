mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use super::message::Message;

glib::wrapper! {
    pub struct MessageGroup(ObjectSubclass<imp::MessageGroup>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MessageGroupState {
    #[default]
    Collapsed,
    Expanded,
}

impl MessageGroup {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_app_icon(&self, icon: Option<&str>) {
        let imp = self.imp();
        imp.icon.set_icon_name(icon);
        imp.icon.set_visible(icon.is_some());
    }

    pub fn set_app_name(&self, name: &str) {
        let imp = self.imp();
        imp.app_name.set_text(name);
        imp.app_name.set_visible(!name.is_empty());
    }

    pub fn state(&self) -> MessageGroupState {
        self.imp().state.get()
    }

    pub fn set_state(&self, state: MessageGroupState) {
        let imp = self.imp();
        if imp.state.get() == state {
            return;
        }
        imp.state.set(state);
        self.rebuild();
    }

    /// Install the messages owned by this group. msgs[0] is treated as the
    /// lead (shown in the lead slot when collapsed, and first when expanded).
    pub fn set_messages(&self, messages: Vec<Message>) {
        self.imp().messages.replace(messages);
        self.rebuild();
    }

    pub(super) fn toggle_state(&self) {
        let next = match self.state() {
            MessageGroupState::Collapsed => MessageGroupState::Expanded,
            MessageGroupState::Expanded => MessageGroupState::Collapsed,
        };
        self.set_state(next);
    }

    /// Re-populate the lead slot and the messages container based on state.
    /// Collapsed: lead in lead_slot, container empty + hidden, header hidden.
    /// Expanded: all messages in container, lead_slot empty, header visible.
    /// The "card edges behind the lead" visual is purely CSS box-shadow on
    /// the lead Message when it lives inside `.message-group__lead`.
    fn rebuild(&self) {
        let imp = self.imp();

        while let Some(child) = imp.lead_slot.first_child() {
            imp.lead_slot.remove(&child);
        }
        while let Some(child) = imp.messages_container.first_child() {
            imp.messages_container.remove(&child);
        }

        let messages = imp.messages.borrow();
        let expanded = imp.state.get() == MessageGroupState::Expanded;
        imp.header.set_visible(expanded);
        imp.messages_container.set_visible(expanded);
        // Reserve room for the lead's box-shadow deck (10px below) when
        // collapsed so the peeks don't overlap into the next group.
        imp.lead_slot
            .set_margin_bottom(if expanded { 0 } else { 12 });

        if messages.is_empty() {
            return;
        }

        for msg in messages.iter() {
            if let Some(parent) = msg.parent()
                && let Some(parent_box) = parent.downcast_ref::<gtk4::Box>()
            {
                parent_box.remove(msg);
            }
        }

        if expanded {
            for msg in messages.iter() {
                imp.messages_container.append(msg);
            }
        } else {
            imp.lead_slot.append(&messages[0]);
        }
    }
}

impl Default for MessageGroup {
    fn default() -> Self {
        Self::new()
    }
}
