use gtk4::{CompositeTemplate, TemplateChild, glib, subclass::prelude::*};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/scrubber_times.ui")]
pub struct ScrubberTimes {
    #[template_child]
    pub position: TemplateChild<gtk4::Label>,
    #[template_child]
    pub length: TemplateChild<gtk4::Label>,
}

#[glib::object_subclass]
impl ObjectSubclass for ScrubberTimes {
    const NAME: &'static str = "ScrubberTimes";
    type Type = super::ScrubberTimes;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ScrubberTimes {}
impl WidgetImpl for ScrubberTimes {}
impl BoxImpl for ScrubberTimes {}
