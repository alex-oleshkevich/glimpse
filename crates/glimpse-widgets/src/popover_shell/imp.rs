use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

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
    const NAME: &'static str = "PopoverShell";
    type Type = super::PopoverShell;
    type ParentType = gtk4::Widget;
    type Interfaces = (gtk4::Buildable,);

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

impl BuildableImpl for PopoverShell {
    fn add_child(&self, builder: &gtk4::Builder, child: &glib::Object, kind: Option<&str>) {
        let own_template = self.content_box.try_get().is_none();
        let shell = self.obj();
        match (kind, child.downcast_ref::<gtk4::Widget>()) {
            _ if own_template => self.parent_add_child(builder, child, kind),
            (Some("hero"), Some(widget)) => shell.set_hero(widget),
            (Some("footer"), Some(widget)) => shell.append_to_footer(widget),
            (None, Some(widget)) => shell.set_content(widget),
            _ => self.parent_add_child(builder, child, kind),
        }
    }
}
