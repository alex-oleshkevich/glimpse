use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};

use crate::widgets::message::Message;

use super::MessageGroupState;

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/message_group.ui")]
pub struct MessageGroup {
    #[template_child]
    pub header: TemplateChild<gtk4::Box>,
    #[template_child]
    pub icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub app_name: TemplateChild<gtk4::Label>,
    #[template_child]
    pub expand: TemplateChild<gtk4::Button>,
    #[template_child]
    pub lead_slot: TemplateChild<gtk4::Box>,
    #[template_child]
    pub messages_container: TemplateChild<gtk4::Box>,

    pub state: Cell<MessageGroupState>,
    pub messages: RefCell<Vec<Message>>,
}

#[glib::object_subclass]
impl ObjectSubclass for MessageGroup {
    const NAME: &'static str = "MessageGroup";
    type Type = super::MessageGroup;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for MessageGroup {
    fn constructed(&self) {
        self.parent_constructed();
        let weak = self.obj().downgrade();
        self.expand.connect_clicked(move |_| {
            if let Some(w) = weak.upgrade() {
                w.toggle_state();
            }
        });
    }
}

impl WidgetImpl for MessageGroup {}
impl BoxImpl for MessageGroup {}
