use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use super::{
    ACTION_INVOKED, ACTIVATED, CLEAR_ALL, CLEAR_GROUP, DISMISSED, DND_TOGGLED, FOOTER_ACTIVATED,
    Group,
};
use crate::{Hero, Notice, Placeholder, Row, Section};

const ATTENTIVE: &str = "preferences-system-notifications-symbolic";
const SILENCED: &str = "notifications-disabled-symbolic";

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/notifications_popover.ui")]
pub struct NotificationsPopover {
    #[template_child]
    pub shell: TemplateChild<crate::PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub scroller: TemplateChild<gtk4::ScrolledWindow>,
    #[template_child]
    pub column: TemplateChild<gtk4::Box>,
    #[template_child]
    pub trouble: TemplateChild<Notice>,
    #[template_child]
    pub groups: TemplateChild<gtk4::Box>,
    #[template_child]
    pub empty: TemplateChild<Placeholder>,
    #[template_child]
    pub clear: TemplateChild<Row>,
    #[template_child]
    pub footer: TemplateChild<Row>,

    pub held: RefCell<Vec<Group>>,
    pub sections: RefCell<Vec<(String, Section)>>,
    pub quiet: gtk4::Switch,

    /// Set while `set_dnd` drives the switch, so the notify handler can tell a value the caller
    /// pushed in from one the viewer flipped. Without it, showing the current state would report
    /// itself back as a change and the two ends would chase each other.
    pub echoing: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for NotificationsPopover {
    const NAME: &'static str = "NotificationsPopover";
    type Type = super::NotificationsPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for NotificationsPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder(ACTIVATED)
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder(DISMISSED)
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder(ACTION_INVOKED)
                    .param_types([String::static_type(), String::static_type()])
                    .build(),
                glib::subclass::Signal::builder(DND_TOGGLED)
                    .param_types([bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder(CLEAR_GROUP)
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder(CLEAR_ALL).build(),
                glib::subclass::Signal::builder(FOOTER_ACTIVATED).build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();

        self.quiet.set_valign(gtk4::Align::Center);
        self.quiet.set_tooltip_text(Some(&gettextrs::gettext(
            "Silence notifications until you turn this off",
        )));
        self.hero.set_slot(&self.quiet);

        self.quiet.connect_active_notify(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |quiet| {
                popover.hero.set_icon_name(Some(match quiet.is_active() {
                    true => SILENCED,
                    false => ATTENTIVE,
                }));
                if popover.echoing.get() {
                    return;
                }
                popover
                    .obj()
                    .emit_by_name::<()>(DND_TOGGLED, &[&quiet.is_active()]);
            }
        ));

        self.clear.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.obj().emit_by_name::<()>(CLEAR_ALL, &[])
        ));
        self.footer.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.obj().emit_by_name::<()>(FOOTER_ACTIVATED, &[])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for NotificationsPopover {}
