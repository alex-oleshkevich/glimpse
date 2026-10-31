use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};

use crate::{set_css_class, truncate};

use super::{LABEL_MAX_CHARS, OVER_CLASS};

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::OsdView)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/osd_view.ui")]
pub struct OsdView {
    #[template_child]
    pub root: TemplateChild<gtk4::Box>,
    #[template_child]
    pub icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub meter: TemplateChild<gtk4::ProgressBar>,
    #[template_child]
    pub percent: TemplateChild<gtk4::Label>,

    #[property(name = "icon-name", get, set = Self::set_icon_name, explicit_notify)]
    icon_name: RefCell<String>,
    #[property(name = "label", get, set = Self::set_label, explicit_notify)]
    label_text: RefCell<String>,
    #[property(name = "value", get, set = Self::set_value, explicit_notify)]
    value: Cell<f64>,
    #[property(name = "has-value", get, set = Self::set_has_value, explicit_notify)]
    has_value: Cell<bool>,
    #[property(name = "show-percent", get, set = Self::set_show_percent, explicit_notify)]
    show_percent: Cell<bool>,
}

impl OsdView {
    fn set_icon_name(&self, name: String) {
        if *self.icon_name.borrow() == name {
            return;
        }
        self.icon_name.replace(name);
        self.sync();
        self.obj().notify_icon_name();
    }

    fn set_label(&self, text: String) {
        let text = truncate(&text, LABEL_MAX_CHARS);
        if *self.label_text.borrow() == text {
            return;
        }
        self.label_text.replace(text);
        self.sync();
        self.obj().notify_label();
    }

    fn set_value(&self, value: f64) {
        if self.value.get() == value {
            return;
        }
        self.value.set(value);
        self.sync();
        self.obj().notify_value();
    }

    fn set_has_value(&self, has_value: bool) {
        if self.has_value.get() == has_value {
            return;
        }
        self.has_value.set(has_value);
        self.sync();
        self.obj().notify_has_value();
    }

    fn set_show_percent(&self, show_percent: bool) {
        if self.show_percent.get() == show_percent {
            return;
        }
        self.show_percent.set(show_percent);
        self.sync();
        self.obj().notify_show_percent();
    }

    fn sync(&self) {
        let name = self.icon_name.borrow();
        let name = (!name.is_empty()).then_some(name.as_str());
        if self.icon.icon_name().as_deref() != name {
            self.icon.set_icon_name(name);
        }
        if self.icon.get_visible() != name.is_some() {
            self.icon.set_visible(name.is_some());
        }
        let label = self.label_text.borrow();
        if self.label.text().as_str() != label.as_str() {
            self.label.set_text(&label);
        }
        if self.label.get_visible() == label.is_empty() {
            self.label.set_visible(!label.is_empty());
        }
        let value = self.value.get();
        let has_value = self.has_value.get();
        if self.meter.get_visible() != has_value {
            self.meter.set_visible(has_value);
        }
        let show_percent = has_value && self.show_percent.get();
        if self.percent.get_visible() != show_percent {
            self.percent.set_visible(show_percent);
        }
        let fraction = value.clamp(0.0, 1.0);
        if self.meter.fraction() != fraction {
            self.meter.set_fraction(fraction);
        }
        let percent = format!("{}%", super::percent(value));
        if self.percent.text().as_str() != percent {
            self.percent.set_label(&percent);
        }
        let over = has_value && value > 1.0;
        if self.root.has_css_class(OVER_CLASS) != over {
            set_css_class(&*self.root, OVER_CLASS, over);
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for OsdView {
    const NAME: &'static str = "OsdView";
    type Type = super::OsdView;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(gtk4::AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for OsdView {
    fn constructed(&self) {
        self.parent_constructed();
        self.sync();
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for OsdView {}
