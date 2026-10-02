use std::cell::Cell;
use std::marker::PhantomData;

use glib::Properties;
use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

#[derive(Debug, Default, CompositeTemplate, Properties)]
#[properties(wrapper_type = super::PopoverShell)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/popover_shell.ui")]
pub struct PopoverShell {
    #[template_child]
    pub header: TemplateChild<crate::Hero>,
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
    pub footer_rule_suppressed: Cell<bool>,

    #[property(name = "icon-name", get = Self::icon_name, set = Self::set_icon_name, nullable)]
    icon_name: PhantomData<Option<String>>,
    #[property(name = "title", get = Self::title, set = Self::set_title, nullable)]
    title: PhantomData<Option<String>>,
    #[property(name = "subtitle", get = Self::subtitle, set = Self::set_subtitle, nullable)]
    subtitle: PhantomData<Option<String>>,
}

impl PopoverShell {
    fn icon_name(&self) -> Option<String> {
        self.header.icon_name().map(|name| name.to_string())
    }

    fn set_icon_name(&self, icon_name: Option<String>) {
        if self.header.icon_name().as_deref() != icon_name.as_deref() {
            self.header.set_icon_name(icon_name.as_deref());
        }
    }

    fn title(&self) -> Option<String> {
        self.header.title().map(|title| title.to_string())
    }

    fn set_title(&self, title: Option<String>) {
        if self.header.title().as_deref() != title.as_deref() {
            self.header.set_title(title.as_deref());
        }
        self.header.set_visible(title.is_some());
    }

    fn subtitle(&self) -> Option<String> {
        self.header.subtitle().map(|subtitle| subtitle.to_string())
    }

    fn set_subtitle(&self, subtitle: Option<String>) {
        if self.header.subtitle().as_deref() != subtitle.as_deref() {
            self.header.set_subtitle(subtitle.as_deref());
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for PopoverShell {
    const NAME: &'static str = "PopoverShell";
    type Type = super::PopoverShell;
    type ParentType = gtk4::Widget;
    type Interfaces = (gtk4::Buildable,);

    fn class_init(klass: &mut Self::Class) {
        crate::Hero::static_type();
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for PopoverShell {
    fn constructed(&self) {
        self.parent_constructed();
        let shell = self.obj();
        let press = gtk4::GestureClick::new();
        press.set_button(0);
        press.set_propagation_phase(gtk4::PropagationPhase::Capture);
        press.connect_pressed(glib::clone!(
            #[weak]
            shell,
            move |press, _, x, y| {
                if shell.dismiss(x, y) {
                    press.set_state(gtk4::EventSequenceState::Claimed);
                }
            }
        ));
        shell.add_controller(press);
    }

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
