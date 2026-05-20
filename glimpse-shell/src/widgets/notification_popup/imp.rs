use glib::subclass::Signal;
use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::sync::OnceLock;

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/notification_popup.ui")]
pub struct NotificationPopup {
    #[template_child] pub icon:       TemplateChild<gtk4::Image>,
    #[template_child] pub app_name:   TemplateChild<gtk4::Label>,
    #[template_child] pub time_label: TemplateChild<gtk4::Label>,
    #[template_child] pub dismiss:    TemplateChild<gtk4::Button>,
    #[template_child] pub summary:    TemplateChild<gtk4::Label>,
    #[template_child] pub body_label: TemplateChild<gtk4::Label>,
    #[template_child] pub actions:    TemplateChild<gtk4::Box>,
}

#[glib::object_subclass]
impl ObjectSubclass for NotificationPopup {
    const NAME: &'static str = "NotificationPopup";
    type Type = super::NotificationPopup;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for NotificationPopup {
    fn constructed(&self) {
        self.parent_constructed();

        let obj = self.obj().downgrade();
        self.dismiss.connect_clicked(move |_| {
            if let Some(popup) = obj.upgrade() {
                popup.emit_by_name::<()>("closed", &[]);
            }
        });
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("closed").build(),
                Signal::builder("action")
                    .param_types([String::static_type()])
                    .build(),
            ]
        })
    }
}

impl WidgetImpl for NotificationPopup {}
impl BoxImpl for NotificationPopup {}
