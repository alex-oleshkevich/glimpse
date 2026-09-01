use gtk4::{AccessibleRole, CompositeTemplate, TemplateChild, glib, subclass::prelude::*};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/popover_shell.ui")]
pub struct PopoverShell {
    #[template_child]
    pub hero_box: TemplateChild<gtk4::Box>,
    #[template_child]
    pub hero_rule: TemplateChild<gtk4::Separator>,
    #[template_child]
    pub content_box: TemplateChild<gtk4::Box>,
    #[template_child]
    pub footer_rule: TemplateChild<gtk4::Separator>,
    #[template_child]
    pub footer_box: TemplateChild<gtk4::Box>,
}

#[glib::object_subclass]
impl ObjectSubclass for PopoverShell {
    const NAME: &'static str = "GlimpsePopoverShell";
    type Type = super::PopoverShell;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for PopoverShell {
    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for PopoverShell {}
