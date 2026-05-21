use glib::subclass::Signal;
use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::sync::OnceLock;

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/calendar_controls.ui")]
pub struct CalendarControls {
    #[template_child]
    pub(super) prev: TemplateChild<gtk4::Button>,
    #[template_child]
    pub(super) title: TemplateChild<gtk4::Button>,
    #[template_child]
    pub(super) today: TemplateChild<gtk4::Button>,
    #[template_child]
    pub(super) next: TemplateChild<gtk4::Button>,
}

#[glib::object_subclass]
impl ObjectSubclass for CalendarControls {
    const NAME: &'static str = "CalendarControls";
    type Type = super::CalendarControls;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for CalendarControls {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();

        let weak = obj.downgrade();
        self.prev.connect_clicked(move |_| {
            if let Some(o) = weak.upgrade() {
                o.emit_by_name::<()>("prev-clicked", &[]);
            }
        });
        let weak = obj.downgrade();
        self.next.connect_clicked(move |_| {
            if let Some(o) = weak.upgrade() {
                o.emit_by_name::<()>("next-clicked", &[]);
            }
        });
        let weak = obj.downgrade();
        self.today.connect_clicked(move |_| {
            if let Some(o) = weak.upgrade() {
                o.emit_by_name::<()>("today-clicked", &[]);
            }
        });
        let weak = obj.downgrade();
        self.title.connect_clicked(move |_| {
            if let Some(o) = weak.upgrade() {
                o.emit_by_name::<()>("title-clicked", &[]);
            }
        });
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("prev-clicked").build(),
                Signal::builder("next-clicked").build(),
                Signal::builder("today-clicked").build(),
                Signal::builder("title-clicked").build(),
            ]
        })
    }
}

impl WidgetImpl for CalendarControls {}
impl BoxImpl for CalendarControls {}
