use gtk4::{CompositeTemplate, TemplateChild, glib, subclass::prelude::*};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/world_clock_row.ui")]
pub struct WorldClockRow {
    #[template_child]
    pub(super) name: TemplateChild<gtk4::Label>,
    #[template_child]
    pub(super) day: TemplateChild<gtk4::Label>,
    #[template_child]
    pub(super) time: TemplateChild<gtk4::Label>,
    #[template_child]
    pub(super) offset: TemplateChild<gtk4::Label>,
}

#[glib::object_subclass]
impl ObjectSubclass for WorldClockRow {
    const NAME: &'static str = "WorldClockRow";
    type Type = super::WorldClockRow;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for WorldClockRow {}
impl WidgetImpl for WorldClockRow {}
impl BoxImpl for WorldClockRow {}
