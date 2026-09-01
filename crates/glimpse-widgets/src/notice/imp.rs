use gtk4::{CompositeTemplate, TemplateChild, accessible, glib, prelude::*, subclass::prelude::*};
use std::cell::Cell;
use std::marker::PhantomData;

use crate::{set_css_class, set_text};

const WARNING: &str = "notice--warning";
const ERROR: &str = "notice--error";

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "NoticeSeverity")]
pub enum Severity {
    #[default]
    Info,
    Warning,
    Error,
}

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::Notice)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/notice.ui")]
pub struct Notice {
    #[template_child]
    pub icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub subtitle: TemplateChild<gtk4::Label>,
    #[template_child]
    pub chevron: TemplateChild<gtk4::Image>,

    #[property(name = "icon-name", get = Self::icon_name, set = Self::set_icon_name, nullable)]
    icon_name: PhantomData<Option<String>>,
    #[property(name = "title", get = Self::title, set = Self::set_title, nullable)]
    title_text: PhantomData<Option<String>>,
    #[property(name = "subtitle", get = Self::subtitle, set = Self::set_subtitle, nullable)]
    subtitle_text: PhantomData<Option<String>>,
    #[property(name = "activatable", get = Self::activatable, set = Self::set_activatable)]
    activatable: PhantomData<bool>,
    #[property(name = "severity", get = Self::severity, set = Self::set_severity, builder(Severity::Info))]
    severity: Cell<Severity>,
}

impl Notice {
    fn icon_name(&self) -> Option<String> {
        self.icon.icon_name().map(|name| name.to_string())
    }

    fn set_icon_name(&self, name: Option<String>) {
        if self.icon_name() == name {
            return;
        }
        self.icon.set_icon_name(name.as_deref());
        self.icon.set_visible(name.is_some());
    }

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
    }

    fn activatable(&self) -> bool {
        self.obj().can_target()
    }

    fn set_activatable(&self, activatable: bool) {
        if self.activatable() == activatable {
            return;
        }
        let notice = self.obj();
        notice.set_can_target(activatable);
        notice.set_can_focus(activatable);
        self.chevron.set_visible(activatable);
    }

    fn severity(&self) -> Severity {
        self.severity.get()
    }

    fn set_severity(&self, severity: Severity) {
        if self.severity.replace(severity) == severity {
            return;
        }
        let notice = self.obj();
        set_css_class(&*notice, WARNING, severity == Severity::Warning);
        set_css_class(&*notice, ERROR, severity == Severity::Error);
    }
}

#[glib::object_subclass]
impl ObjectSubclass for Notice {
    const NAME: &'static str = "Notice";
    type Type = super::Notice;
    type ParentType = gtk4::Button;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for Notice {
    fn constructed(&self) {
        self.parent_constructed();
        let notice = self.obj();
        notice.set_can_target(false);
        notice.set_can_focus(false);
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for Notice {}
impl ButtonImpl for Notice {}
