use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use super::{
    ACTION_INVOKED, ACTIVATED, CLEAR_ALL, CLEAR_GROUP, DISMISSED, DND_TOGGLED, FOOTER_ACTIVATED,
    Group,
};
use crate::{Hero, Notice, Placeholder, Row};

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
    pub sections: RefCell<Vec<(String, gtk4::Revealer)>>,
    pub notifications: gtk4::Switch,
    pub echoing: Cell<bool>,
    pub fading: Cell<bool>,
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

        self.notifications.set_active(true);
        self.notifications.set_valign(gtk4::Align::Center);
        self.notifications
            .set_tooltip_text(Some(&gettextrs::gettext(
                "Show notification popups while this is on",
            )));
        self.hero.set_slot(&self.notifications);

        self.notifications.connect_active_notify(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |notifications| {
                popover
                    .hero
                    .set_icon_name(Some(match notifications.is_active() {
                        true => ATTENTIVE,
                        false => SILENCED,
                    }));
                if popover.echoing.get() {
                    return;
                }
                let silenced = !notifications.is_active();
                popover.obj().emit_by_name::<()>(DND_TOGGLED, &[&silenced]);
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
