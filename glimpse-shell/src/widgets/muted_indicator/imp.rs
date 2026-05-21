use gtk4::{CompositeTemplate, TemplateChild, glib, subclass::prelude::*};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/muted_indicator.ui")]
pub struct MutedIndicator {
    #[template_child]
    pub icon: TemplateChild<gtk4::Image>,
}

#[glib::object_subclass]
impl ObjectSubclass for MutedIndicator {
    const NAME: &'static str = "MutedIndicator";
    type Type = super::MutedIndicator;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for MutedIndicator {}
impl WidgetImpl for MutedIndicator {}
impl BoxImpl for MutedIndicator {}
