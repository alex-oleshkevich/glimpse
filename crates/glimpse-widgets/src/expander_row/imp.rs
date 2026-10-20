use gtk4::{CompositeTemplate, TemplateChild, accessible, glib, prelude::*, subclass::prelude::*};
use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::OnceLock;

use crate::{
    ActionRow, Expandable, Row, RowTone, RowTypography, TEXT_MAX_CHARS, set_css_class, truncate,
};

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::ExpanderRow)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/expander_row.ui")]
pub struct ExpanderRow {
    #[template_child]
    pub expandable: TemplateChild<Expandable>,
    #[template_child]
    pub head: TemplateChild<ActionRow>,
    #[template_child]
    pub details: TemplateChild<gtk4::Box>,
    #[property(name = "icon-name", get = Self::icon_name, set = Self::set_icon_name, nullable, explicit_notify)]
    icon_name_prop: PhantomData<Option<String>>,
    #[property(name = "title", get = Self::title, set = Self::set_title, nullable, explicit_notify)]
    title_prop: PhantomData<Option<String>>,
    #[property(name = "subtitle", get = Self::subtitle, set = Self::set_subtitle, nullable, explicit_notify)]
    subtitle_prop: PhantomData<Option<String>>,
    #[property(name = "value", get = Self::value, set = Self::set_value, nullable, explicit_notify)]
    value_prop: PhantomData<Option<String>>,
    #[property(name = "tone", get = Self::tone, set = Self::set_tone, builder(RowTone::Normal).explicit_notify())]
    tone_prop: PhantomData<RowTone>,
    #[property(name = "inactive", get = Self::inactive, set = Self::set_inactive, explicit_notify)]
    inactive_prop: PhantomData<bool>,
    #[property(name = "tint", get, set = Self::set_tint, explicit_notify)]
    tint: Cell<bool>,
    #[property(name = "title-lines", get = Self::title_lines, set = Self::set_title_lines, default = 1, explicit_notify, lax_validation)]
    title_lines_prop: PhantomData<i32>,
    #[property(name = "tooltip", get = Self::tooltip, set = Self::set_tooltip, nullable, explicit_notify)]
    tooltip_prop: PhantomData<Option<String>>,
    #[property(name = "typography", get = Self::typography, set = Self::set_typography, builder(RowTypography::Default).explicit_notify())]
    typography_prop: PhantomData<RowTypography>,
    #[property(name = "busy", get = Self::busy, set = Self::set_busy, explicit_notify)]
    busy_prop: PhantomData<bool>,
    #[property(name = "open", get = Self::open, set = Self::set_open, explicit_notify)]
    open_prop: PhantomData<bool>,
}

impl ExpanderRow {
    fn icon_name(&self) -> Option<String> {
        self.head.upcast_ref::<Row>().icon_name()
    }
    fn set_icon_name(&self, value: Option<String>) {
        let value = value.filter(|name| !name.is_empty());
        if self.icon_name() == value {
            return;
        }
        self.head
            .upcast_ref::<Row>()
            .set_icon_name(value.as_deref());
        self.obj().notify_icon_name();
    }
    fn title(&self) -> Option<String> {
        self.head.upcast_ref::<Row>().title()
    }
    fn set_title(&self, value: Option<String>) {
        let value = value
            .map(|text| truncate(&text, TEXT_MAX_CHARS))
            .filter(|text| !text.is_empty());
        if self.title() == value {
            return;
        }
        self.head.upcast_ref::<Row>().set_title(value.as_deref());
        self.obj().notify_title();
    }
    fn subtitle(&self) -> Option<String> {
        self.head.upcast_ref::<Row>().subtitle()
    }
    fn set_subtitle(&self, value: Option<String>) {
        let value = value
            .map(|text| truncate(&text, TEXT_MAX_CHARS))
            .filter(|text| !text.is_empty());
        if self.subtitle() == value {
            return;
        }
        self.head.upcast_ref::<Row>().set_subtitle(value.as_deref());
        self.obj().notify_subtitle();
    }
    fn value(&self) -> Option<String> {
        self.head.upcast_ref::<Row>().value()
    }
    fn set_value(&self, value: Option<String>) {
        let value = value
            .map(|text| truncate(&text, TEXT_MAX_CHARS))
            .filter(|text| !text.is_empty());
        if self.value() == value {
            return;
        }
        self.head.upcast_ref::<Row>().set_value(value.as_deref());
        self.obj().notify_value();
    }
    fn tone(&self) -> RowTone {
        self.head.upcast_ref::<Row>().tone()
    }
    fn set_tone(&self, value: RowTone) {
        if self.tone() == value {
            return;
        }
        self.head.upcast_ref::<Row>().set_tone(value);
        self.sync_tint();
        self.obj().notify_tone();
    }
    fn inactive(&self) -> bool {
        self.head.upcast_ref::<Row>().inactive()
    }
    fn set_inactive(&self, value: bool) {
        if self.inactive() == value {
            return;
        }
        self.head.upcast_ref::<Row>().set_inactive(value);
        self.obj().notify_inactive();
    }
    fn set_tint(&self, value: bool) {
        if self.tint.replace(value) == value {
            return;
        }
        self.sync_tint();
        self.obj().notify_tint();
    }
    fn sync_tint(&self) {
        let row = self.obj();
        set_css_class(&*row, "expander-row--tint", self.tint.get());
        set_css_class(&*row, "row--warning", self.tone() == RowTone::Warning);
        set_css_class(&*row, "row--error", self.tone() == RowTone::Error);
        set_css_class(
            &*row,
            "row--destructive",
            self.tone() == RowTone::Destructive,
        );
    }
    fn title_lines(&self) -> i32 {
        self.head.upcast_ref::<Row>().imp().title.lines().max(1)
    }
    fn set_title_lines(&self, value: i32) {
        let value = value.clamp(1, 4);
        if self.title_lines() == value {
            return;
        }
        let title = &self.head.upcast_ref::<Row>().imp().title;
        title.set_lines(value);
        if title.wraps() != (value > 1) {
            title.set_wrap(value > 1);
        }
        if title.is_single_line_mode() != (value == 1) {
            title.set_single_line_mode(value == 1);
        }
        self.obj().notify_title_lines();
    }
    fn tooltip(&self) -> Option<String> {
        self.head.upcast_ref::<Row>().tooltip()
    }
    fn set_tooltip(&self, value: Option<String>) {
        let value = value
            .map(|text| truncate(&text, TEXT_MAX_CHARS))
            .filter(|text| !text.is_empty());
        if self.tooltip() == value {
            return;
        }
        self.head.upcast_ref::<Row>().set_tooltip(value.as_deref());
        self.obj().notify_tooltip();
    }
    fn typography(&self) -> RowTypography {
        self.head.upcast_ref::<Row>().typography()
    }
    fn set_typography(&self, value: RowTypography) {
        if self.typography() == value {
            return;
        }
        self.head.upcast_ref::<Row>().set_typography(value);
        self.obj().notify_typography();
    }
    fn busy(&self) -> bool {
        self.head.upcast_ref::<Row>().busy()
    }
    fn set_busy(&self, value: bool) {
        if self.busy() == value {
            return;
        }
        self.head.upcast_ref::<Row>().set_busy(value);
        self.obj().notify_busy();
    }
    fn open(&self) -> bool {
        self.expandable.expanded()
    }
    fn set_open(&self, value: bool) {
        self.expandable.set_expanded(value);
    }
}

#[glib::object_subclass]
impl ObjectSubclass for ExpanderRow {
    const NAME: &'static str = "ExpanderRow";
    type Type = super::ExpanderRow;
    type ParentType = gtk4::Widget;
    type Interfaces = (gtk4::Buildable,);

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(gtk4::AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for ExpanderRow {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("toggled")
                    .param_types([bool::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let row = self.obj().clone();
        self.head.upcast_ref::<Row>().imp().set_inspection();
        self.head.upcast_ref::<Row>().set_chevron(true);
        self.head
            .update_state(&[accessible::State::Expanded(Some(false))]);
        self.expandable.connect_expanded_notify(glib::clone!(
            #[weak]
            row,
            move |expandable| {
                let open = expandable.expanded();
                set_css_class(&*row.imp().head, "row--open", open);
                row.imp()
                    .head
                    .update_state(&[accessible::State::Expanded(Some(open))]);
                row.notify_open();
                row.emit_by_name::<()>("toggled", &[&open]);
            }
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for ExpanderRow {}

impl BuildableImpl for ExpanderRow {
    fn add_child(&self, builder: &gtk4::Builder, child: &glib::Object, kind: Option<&str>) {
        if self.details.try_get().is_none() {
            self.parent_add_child(builder, child, kind);
        } else if let Some(widget) = child.downcast_ref::<gtk4::Widget>() {
            match kind {
                Some("end") => self.head.upcast_ref::<Row>().set_end(widget),
                Some("details") | None => self.details.append(widget),
                _ => self.parent_add_child(builder, child, kind),
            }
        } else {
            self.parent_add_child(builder, child, kind);
        }
    }
}
