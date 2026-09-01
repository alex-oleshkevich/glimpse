use gtk4::{CompositeTemplate, TemplateChild, accessible, glib, prelude::*, subclass::prelude::*};
use std::marker::PhantomData;

use crate::{set_css_class, set_text};

const CHECK_ICON: &str = "object-select-symbolic";
const SELECTED: &str = "row--on";
const TWO_LINE: &str = "row--two";

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::Row)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/row.ui")]
pub struct Row {
    #[template_child]
    pub check: TemplateChild<gtk4::Image>,
    #[template_child]
    pub lead: TemplateChild<gtk4::Box>,
    #[template_child]
    pub title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub subtitle: TemplateChild<gtk4::Label>,
    #[template_child]
    pub trail: TemplateChild<gtk4::Box>,

    #[property(name = "title", get = Self::title, set = Self::set_title, nullable)]
    title_text: PhantomData<Option<String>>,
    #[property(name = "subtitle", get = Self::subtitle, set = Self::set_subtitle, nullable)]
    subtitle_text: PhantomData<Option<String>>,
    #[property(name = "selectable", get = Self::selectable, set = Self::set_selectable)]
    selectable: PhantomData<bool>,
    #[property(name = "selected", get = Self::selected, set = Self::set_selected)]
    selected: PhantomData<bool>,
    #[property(name = "activatable", get = Self::activatable, set = Self::set_activatable)]
    activatable: PhantomData<bool>,
}

impl Row {
    fn title(&self) -> Option<String> {
        self.title
            .get_visible()
            .then(|| self.title.text().to_string())
    }

    fn set_title(&self, title: Option<String>) {
        set_text(&self.title, title.as_deref());
        self.obj()
            .update_property(&[accessible::Property::Label(self.title.text().as_str())]);
    }

    fn subtitle(&self) -> Option<String> {
        self.subtitle
            .get_visible()
            .then(|| self.subtitle.text().to_string())
    }

    fn set_subtitle(&self, subtitle: Option<String>) {
        set_text(&self.subtitle, subtitle.as_deref());
        set_css_class(&*self.obj(), TWO_LINE, self.subtitle.get_visible());
    }

    fn selectable(&self) -> bool {
        self.check.get_visible()
    }

    fn set_selectable(&self, selectable: bool) {
        if self.selectable() == selectable {
            return;
        }
        self.check.set_visible(selectable);
    }

    fn selected(&self) -> bool {
        self.obj().has_css_class(SELECTED)
    }

    fn set_selected(&self, selected: bool) {
        if self.selected() == selected {
            return;
        }
        match selected {
            true => self.check.set_icon_name(Some(CHECK_ICON)),
            false => self.check.clear(),
        }
        set_css_class(&*self.obj(), SELECTED, selected);
    }

    fn activatable(&self) -> bool {
        self.obj().can_target()
    }

    fn set_activatable(&self, activatable: bool) {
        if self.activatable() == activatable {
            return;
        }
        let row = self.obj();
        row.set_can_target(activatable);
        row.set_can_focus(activatable);
    }
}

#[glib::object_subclass]
impl ObjectSubclass for Row {
    const NAME: &'static str = "Row";
    type Type = super::Row;
    type ParentType = gtk4::Button;
    type Interfaces = (gtk4::Buildable,);

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for Row {
    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for Row {}
impl ButtonImpl for Row {}

impl BuildableImpl for Row {
    fn add_child(&self, builder: &gtk4::Builder, child: &glib::Object, kind: Option<&str>) {
        let own_template = self.trail.try_get().is_none();
        let row = self.obj();
        match (kind, child.downcast_ref::<gtk4::Widget>()) {
            _ if own_template => self.parent_add_child(builder, child, kind),
            (Some("lead"), Some(widget)) => row.set_lead(widget),
            (Some("trail"), Some(widget)) => row.set_trail(widget),
            _ => self.parent_add_child(builder, child, kind),
        }
    }
}
