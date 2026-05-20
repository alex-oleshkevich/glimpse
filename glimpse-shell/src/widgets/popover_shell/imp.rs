use gtk4::{CompositeTemplate, TemplateChild, glib, subclass::prelude::*};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/popover_shell.ui")]
pub struct PopoverShell {
    #[template_child]
    pub content: TemplateChild<gtk4::Box>,
    #[template_child]
    pub footer: TemplateChild<gtk4::Box>,
}

#[glib::object_subclass]
impl ObjectSubclass for PopoverShell {
    const NAME: &'static str = "PopoverShell";
    type Type = super::PopoverShell;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for PopoverShell {}
impl WidgetImpl for PopoverShell {}
impl BoxImpl for PopoverShell {}
