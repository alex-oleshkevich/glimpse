use gtk4::{CompositeTemplate, TemplateChild, accessible, glib, prelude::*, subclass::prelude::*};
use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::OnceLock;

use super::ENTRY_MAX_CHARS;
use crate::{RowTone, TEXT_MAX_CHARS, set_css_class, set_text, truncate};

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::EntryRow)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/entry_row.ui")]
pub struct EntryRow {
    #[template_child]
    pub content: TemplateChild<gtk4::Box>,
    #[template_child]
    pub title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub entry: TemplateChild<gtk4::Entry>,
    #[template_child]
    pub end: TemplateChild<gtk4::Box>,
    quiet: Cell<bool>,
    #[property(name = "title", get = Self::title, set = Self::set_title, nullable, explicit_notify)]
    title_prop: PhantomData<Option<String>>,
    #[property(name = "placeholder", get = Self::placeholder, set = Self::set_placeholder, nullable, explicit_notify)]
    placeholder_prop: PhantomData<Option<String>>,
    #[property(name = "text", get = Self::text, set = Self::set_text, explicit_notify)]
    text_prop: PhantomData<String>,
    #[property(name = "tone", get, set = Self::set_tone, builder(RowTone::Normal).explicit_notify())]
    tone: Cell<RowTone>,
    #[property(name = "inactive", get, set = Self::set_inactive, explicit_notify)]
    inactive: Cell<bool>,
    #[property(name = "tooltip", get = Self::tooltip, set = Self::set_tooltip, nullable, explicit_notify)]
    tooltip_prop: PhantomData<Option<String>>,
}

impl EntryRow {
    fn title(&self) -> Option<String> {
        let text = self.title.text();
        (!text.is_empty()).then(|| text.to_string())
    }
    fn set_title(&self, value: Option<String>) {
        let before = self.title.text();
        set_text(&self.title, value.as_deref());
        if self.title.text() == before {
            return;
        }
        self.sync_accessible();
        self.obj().notify_title();
    }
    fn placeholder(&self) -> Option<String> {
        self.entry.placeholder_text().map(|text| text.to_string())
    }
    fn set_placeholder(&self, value: Option<String>) {
        let value = value
            .map(|text| truncate(&text, TEXT_MAX_CHARS))
            .filter(|text| !text.is_empty());
        if self.placeholder() == value {
            return;
        }
        self.entry.set_placeholder_text(value.as_deref());
        self.sync_accessible();
        self.obj().notify_placeholder();
    }
    fn text(&self) -> String {
        self.entry.text().to_string()
    }
    fn set_text(&self, value: String) {
        let value = truncate(&value, ENTRY_MAX_CHARS);
        if self.entry.text().as_str() == value {
            return;
        }
        let position = self.entry.position();
        self.quiet.set(true);
        self.entry.set_text(&value);
        self.quiet.set(false);
        self.entry
            .set_position(position.min(value.chars().count() as i32));
        self.obj().notify_text();
    }
    fn set_tone(&self, value: RowTone) {
        if self.tone.replace(value) == value {
            return;
        }
        let row = self.obj();
        set_css_class(&*row, "row--warning", value == RowTone::Warning);
        set_css_class(&*row, "row--error", value == RowTone::Error);
        row.notify_tone();
    }
    fn set_inactive(&self, value: bool) {
        if self.inactive.replace(value) == value {
            return;
        }
        set_css_class(&*self.obj(), "row--inactive", value);
        self.content.set_sensitive(!value);
        self.obj().notify_inactive();
    }
    fn tooltip(&self) -> Option<String> {
        self.obj().tooltip_text().map(|value| value.to_string())
    }
    fn set_tooltip(&self, value: Option<String>) {
        let value = value
            .map(|text| truncate(&text, TEXT_MAX_CHARS))
            .filter(|text| !text.is_empty());
        if self.tooltip() == value {
            return;
        }
        self.obj().set_tooltip_text(value.as_deref());
        self.obj().notify_tooltip();
    }
    fn sync_accessible(&self) {
        let label = self
            .title()
            .or_else(|| self.placeholder())
            .unwrap_or_default();
        self.entry
            .update_property(&[accessible::Property::Label(&label)]);
    }
}

#[glib::object_subclass]
impl ObjectSubclass for EntryRow {
    const NAME: &'static str = "EntryRow";
    type Type = super::EntryRow;
    type ParentType = gtk4::Widget;
    type Interfaces = (gtk4::Buildable,);

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_css_name("listrow");
        klass.set_accessible_role(gtk4::AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for EntryRow {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("changed").build(),
                glib::subclass::Signal::builder("activated").build(),
            ]
        })
    }
    fn constructed(&self) {
        self.parent_constructed();
        let row = self.obj().clone();
        self.entry.connect_changed(glib::clone!(
            #[weak]
            row,
            move |_| {
                if row.imp().quiet.get() {
                    return;
                }
                row.notify_text();
                row.emit_by_name::<()>("changed", &[]);
            }
        ));
        self.entry.connect_activate(glib::clone!(
            #[weak]
            row,
            move |_| row.emit_by_name::<()>("activated", &[])
        ));
    }
    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for EntryRow {
    fn grab_focus(&self) -> bool {
        self.entry.grab_focus()
    }
}

impl BuildableImpl for EntryRow {
    fn add_child(&self, builder: &gtk4::Builder, child: &glib::Object, type_: Option<&str>) {
        let own_template = self.end.try_get().is_none();
        match (type_, child.downcast_ref::<gtk4::Widget>()) {
            _ if own_template => self.parent_add_child(builder, child, type_),
            (Some("end"), Some(widget)) => self.obj().set_end(widget),
            _ => self.parent_add_child(builder, child, type_),
        }
    }
}
