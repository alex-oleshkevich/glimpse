use glib::subclass::Signal;
use gtk4::{CompositeTemplate, TemplateChild, gdk, glib, prelude::*, subclass::prelude::*};
use std::sync::OnceLock;

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/message.ui")]
pub struct Message {
    #[template_child]
    pub icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub app_name: TemplateChild<gtk4::Label>,
    #[template_child]
    pub time_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub dismiss: TemplateChild<gtk4::Button>,
    #[template_child]
    pub content_icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub body_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub actions: TemplateChild<gtk4::Box>,
}

#[glib::object_subclass]
impl ObjectSubclass for Message {
    const NAME: &'static str = "Message";
    type Type = super::Message;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for Message {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();

        let weak = obj.downgrade();
        self.dismiss.connect_clicked(move |_| {
            if let Some(w) = weak.upgrade() {
                w.emit_by_name::<()>("closed", &[]);
            }
        });

        let click = gtk4::GestureClick::new();
        click.set_button(gdk::BUTTON_PRIMARY);
        let weak = obj.downgrade();
        click.connect_released(move |gesture, _, _, _| {
            if let Some(w) = weak.upgrade() {
                w.emit_by_name::<()>("clicked", &[&gesture.current_event_state()]);
            }
        });
        obj.add_controller(click);

        let rclick = gtk4::GestureClick::new();
        rclick.set_button(gdk::BUTTON_SECONDARY);
        let weak = obj.downgrade();
        rclick.connect_released(move |_, _, _, _| {
            if let Some(w) = weak.upgrade() {
                w.emit_by_name::<()>("secondary-clicked", &[]);
            }
        });
        obj.add_controller(rclick);
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("closed").build(),
                Signal::builder("clicked")
                    .param_types([gdk::ModifierType::static_type()])
                    .build(),
                Signal::builder("secondary-clicked").build(),
                Signal::builder("action")
                    .param_types([String::static_type()])
                    .build(),
            ]
        })
    }
}

impl WidgetImpl for Message {}
impl BoxImpl for Message {}
