use gtk4::{CompositeTemplate, TemplateChild, accessible, glib, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;

use crate::{TEXT_MAX_CHARS, fill_slot, set_css_class, set_text, truncate};

const TONE_WARNING: &str = "row--warning";
const TONE_ERROR: &str = "row--error";
const TONE_DESTRUCTIVE: &str = "row--destructive";
const NUMERIC: &str = "row--numeric";
const MONOSPACE: &str = "row--monospace";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "RowTone")]
pub enum RowTone {
    #[default]
    Normal,
    Warning,
    Error,
    Destructive,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "RowTypography")]
pub enum RowTypography {
    #[default]
    Default,
    Numeric,
    Monospace,
}

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::Row)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/row.ui")]
pub struct Row {
    #[template_child]
    pub content: TemplateChild<gtk4::Box>,
    #[template_child]
    pub check: TemplateChild<gtk4::Image>,
    #[template_child]
    pub icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub subtitle: TemplateChild<gtk4::Label>,
    #[template_child]
    pub value: TemplateChild<gtk4::Label>,
    #[template_child]
    pub spinner: TemplateChild<gtk4::Spinner>,
    #[template_child]
    pub end: TemplateChild<gtk4::Box>,
    #[template_child]
    pub chevron: TemplateChild<gtk4::Image>,
    #[property(name = "icon-name", get = Self::icon_name, set = Self::set_icon_name, nullable, explicit_notify)]
    icon_name: PhantomData<Option<String>>,
    #[property(name = "title", get = Self::title, set = Self::set_title, nullable, explicit_notify)]
    title_prop: PhantomData<Option<String>>,
    #[property(name = "subtitle", get = Self::subtitle, set = Self::set_subtitle, nullable, explicit_notify)]
    subtitle_prop: PhantomData<Option<String>>,
    #[property(name = "value", get = Self::value, set = Self::set_value, nullable, explicit_notify)]
    value_prop: PhantomData<Option<String>>,
    #[property(name = "tone", get, set = Self::set_tone, builder(RowTone::Normal).explicit_notify())]
    tone: Cell<RowTone>,
    #[property(name = "inactive", get, set = Self::set_inactive, explicit_notify)]
    inactive: Cell<bool>,
    inspection: Cell<bool>,
    #[property(name = "tooltip", get = Self::tooltip, set = Self::set_tooltip, nullable, explicit_notify)]
    tooltip_prop: PhantomData<Option<String>>,
    #[property(
        name = "typography",
        get,
        set = Self::set_typography,
        builder(RowTypography::Default).explicit_notify()
    )]
    typography: Cell<RowTypography>,
    #[property(name = "busy", get, set = Self::set_busy, explicit_notify)]
    busy: Cell<bool>,
    announced_label: RefCell<String>,
    announced_description: RefCell<String>,
}

#[glib::object_subclass]
impl ObjectSubclass for Row {
    const NAME: &'static str = "Row";
    type Type = super::Row;
    type ParentType = gtk4::Widget;
    type Interfaces = (gtk4::Buildable,);

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_css_name("listrow");
        klass.set_accessible_role(gtk4::AccessibleRole::ListItem);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for Row {
    fn constructed(&self) {
        self.parent_constructed();
        self.sync_accessible();
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for Row {}

impl BuildableImpl for Row {
    fn add_child(&self, builder: &gtk4::Builder, child: &glib::Object, type_: Option<&str>) {
        let own_template = self.end.try_get().is_none();
        match (type_, child.downcast_ref::<gtk4::Widget>()) {
            _ if own_template => self.parent_add_child(builder, child, type_),
            (Some("end"), Some(widget)) => self.set_end(widget),
            _ => self.parent_add_child(builder, child, type_),
        }
    }
}

impl Row {
    pub(crate) fn check(&self) -> gtk4::Image {
        self.check.get()
    }

    pub(crate) fn end(&self) -> gtk4::Box {
        self.end.get()
    }

    #[cfg(test)]
    pub(crate) fn chevron(&self) -> gtk4::Image {
        self.chevron.get()
    }

    pub(crate) fn set_end(&self, widget: &impl IsA<gtk4::Widget>) {
        fill_slot(&self.end, widget);
        self.sync_slots();
        self.sync_accessible();
    }

    pub(crate) fn sync_slots(&self) {
        let busy = self.busy.get();
        let occupied = self.end.first_child().is_some();
        let show_value = !busy && !self.value.text().is_empty();
        reveal(&*self.spinner, busy);
        if self.spinner.is_spinning() != busy {
            self.spinner.set_spinning(busy);
        }
        reveal(&*self.end, !busy && occupied);
        reveal(&*self.value, show_value);
    }

    fn icon_name(&self) -> Option<String> {
        self.icon.icon_name().map(|name| name.to_string())
    }

    fn set_icon_name(&self, icon: Option<String>) {
        let icon = icon.filter(|name| !name.is_empty());
        if self.icon.icon_name().as_deref() == icon.as_deref() {
            return;
        }
        self.icon.set_icon_name(icon.as_deref());
        reveal(&*self.icon, icon.is_some());
        self.obj().notify_icon_name();
    }

    fn title(&self) -> Option<String> {
        present(&self.title)
    }

    fn set_title(&self, title: Option<String>) {
        if !write_label(&self.title, title.as_deref()) {
            return;
        }
        self.sync_accessible();
        self.obj().notify_title();
    }

    fn subtitle(&self) -> Option<String> {
        present(&self.subtitle)
    }

    fn set_subtitle(&self, subtitle: Option<String>) {
        if !write_label(&self.subtitle, subtitle.as_deref()) {
            return;
        }
        self.sync_accessible();
        self.obj().notify_subtitle();
    }

    fn value(&self) -> Option<String> {
        present(&self.value)
    }

    fn set_value(&self, value: Option<String>) {
        if !write_label(&self.value, value.as_deref()) {
            return;
        }
        self.sync_slots();
        self.sync_accessible();
        self.obj().notify_value();
    }

    fn set_tone(&self, tone: RowTone) {
        if self.tone.replace(tone) == tone {
            return;
        }
        let row = self.obj();
        set_css_class(&*row, TONE_WARNING, tone == RowTone::Warning);
        set_css_class(&*row, TONE_ERROR, tone == RowTone::Error);
        set_css_class(&*row, TONE_DESTRUCTIVE, tone == RowTone::Destructive);
        row.notify_tone();
    }

    fn set_inactive(&self, inactive: bool) {
        if self.inactive.replace(inactive) == inactive {
            return;
        }
        self.sync_inactive();
        self.obj().notify_inactive();
    }

    pub(crate) fn set_inspection(&self) {
        if self.inspection.replace(true) {
            return;
        }
        self.sync_inactive();
    }

    fn sync_inactive(&self) {
        let row = self.obj();
        set_css_class(&*row, "row--inactive", self.inactive.get());
        let sensitive = self.inspection.get() || !self.inactive.get();
        if row.is_sensitive() != sensitive {
            row.set_sensitive(sensitive);
        }
    }

    fn tooltip(&self) -> Option<String> {
        self.obj()
            .tooltip_text()
            .map(|text| text.to_string())
            .filter(|text| !text.is_empty())
    }

    fn set_tooltip(&self, tooltip: Option<String>) {
        let text = tooltip.and_then(|value| {
            let capped = truncate(&value, TEXT_MAX_CHARS);
            (!capped.is_empty()).then_some(capped)
        });
        if self.obj().tooltip_text().as_deref() == text.as_deref() {
            return;
        }
        self.obj().set_tooltip_text(text.as_deref());
        self.obj().notify_tooltip();
    }

    fn set_typography(&self, typography: RowTypography) {
        if self.typography.replace(typography) == typography {
            return;
        }
        let row = self.obj();
        set_css_class(&*row, NUMERIC, typography == RowTypography::Numeric);
        set_css_class(&*row, MONOSPACE, typography == RowTypography::Monospace);
        row.notify_typography();
    }

    fn set_busy(&self, busy: bool) {
        if self.busy.replace(busy) == busy {
            return;
        }
        self.sync_slots();
        self.sync_accessible();
        self.obj().notify_busy();
    }

    pub(crate) fn sync_accessible(&self) {
        let label = self.title.text().to_string();
        let mut parts = Vec::new();
        if self.subtitle.get_visible() && !self.subtitle.text().is_empty() {
            parts.push(self.subtitle.text().to_string());
        }
        if self.value.get_visible() && !self.value.text().is_empty() {
            parts.push(self.value.text().to_string());
        }
        let description = parts.join(", ");
        if *self.announced_label.borrow() == label
            && *self.announced_description.borrow() == description
        {
            return;
        }
        self.announced_label.replace(label.clone());
        self.announced_description.replace(description.clone());
        self.obj().update_property(&[
            accessible::Property::Label(&label),
            accessible::Property::Description(&description),
        ]);
    }
}

fn present(label: &gtk4::Label) -> Option<String> {
    let text = label.text();
    (!text.is_empty()).then(|| text.to_string())
}

fn write_label(label: &gtk4::Label, text: Option<&str>) -> bool {
    let next = truncate(text.unwrap_or(""), TEXT_MAX_CHARS);
    if label.text().as_str() == next {
        return false;
    }
    set_text(label, text);
    true
}

fn reveal(widget: &impl IsA<gtk4::Widget>, visible: bool) {
    let widget = widget.upcast_ref::<gtk4::Widget>();
    if widget.get_visible() != visible {
        widget.set_visible(visible);
    }
}
