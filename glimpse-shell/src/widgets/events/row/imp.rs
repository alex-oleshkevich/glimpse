use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/events_row.ui")]
pub struct EventRow {
    #[template_child]
    pub(super) title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub(super) time: TemplateChild<gtk4::Label>,
}

#[glib::object_subclass]
impl ObjectSubclass for EventRow {
    const NAME: &'static str = "EventRow";
    type Type = super::EventRow;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for EventRow {}
impl WidgetImpl for EventRow {}
impl BoxImpl for EventRow {}

impl EventRow {
    pub(super) fn set_title(&self, text: &str) {
        self.title.set_label(text);
    }

    pub(super) fn set_time(&self, text: &str) {
        if text.is_empty() {
            self.time.set_visible(false);
        } else {
            self.time.set_label(text);
            self.time.set_visible(true);
        }
    }
}
