use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::marker::PhantomData;
use std::sync::OnceLock;

use crate::Row;

const DETAIL_ICON: &str = "go-next-symbolic";

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::SplitRow)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/split_row.ui")]
pub struct SplitRow {
    #[template_child]
    pub row: TemplateChild<Row>,
    #[template_child]
    pub rule: TemplateChild<gtk4::Separator>,
    #[template_child]
    pub detail: TemplateChild<gtk4::Button>,

    #[property(name = "title", get = Self::title, set = Self::set_title, nullable)]
    title: PhantomData<Option<String>>,
    #[property(name = "subtitle", get = Self::subtitle, set = Self::set_subtitle, nullable)]
    subtitle: PhantomData<Option<String>>,
    #[property(name = "lead-icon", get = Self::lead_icon, set = Self::set_lead_icon, nullable)]
    lead_icon: PhantomData<Option<String>>,
    #[property(name = "value", get = Self::value, set = Self::set_value, nullable)]
    value: PhantomData<Option<String>>,
    #[property(name = "selectable", get = Self::selectable, set = Self::set_selectable)]
    selectable: PhantomData<bool>,
    #[property(name = "selected", get = Self::selected, set = Self::set_selected)]
    selected: PhantomData<bool>,
    #[property(
        name = "detail-icon",
        get = Self::detail_icon,
        set = Self::set_detail_icon,
        default = DETAIL_ICON
    )]
    detail_icon: PhantomData<String>,
    #[property(name = "detail-tooltip", get = Self::detail_tooltip, set = Self::set_detail_tooltip, nullable)]
    detail_tooltip: PhantomData<Option<String>>,
}

impl SplitRow {
    fn title(&self) -> Option<String> {
        self.row.title()
    }

    fn set_title(&self, title: Option<String>) {
        self.row.set_title(title.as_deref());
    }

    fn subtitle(&self) -> Option<String> {
        self.row.subtitle()
    }

    fn set_subtitle(&self, subtitle: Option<String>) {
        self.row.set_subtitle(subtitle.as_deref());
    }

    fn lead_icon(&self) -> Option<String> {
        self.row.lead_icon()
    }

    fn set_lead_icon(&self, icon: Option<String>) {
        self.row.set_lead_icon(icon.as_deref());
    }

    fn value(&self) -> Option<String> {
        self.row.value()
    }

    fn set_value(&self, value: Option<String>) {
        self.row.set_value(value.as_deref());
    }

    fn selectable(&self) -> bool {
        self.row.selectable()
    }

    fn set_selectable(&self, selectable: bool) {
        self.row.set_selectable(selectable);
    }

    fn selected(&self) -> bool {
        self.row.selected()
    }

    fn set_selected(&self, selected: bool) {
        self.row.set_selected(selected);
    }

    fn detail_icon(&self) -> String {
        self.detail.icon_name().unwrap_or_default().to_string()
    }

    fn set_detail_icon(&self, icon: String) {
        if self.detail_icon() == icon {
            return;
        }
        self.detail.set_icon_name(&icon);
    }

    fn detail_tooltip(&self) -> Option<String> {
        self.detail.tooltip_text().map(|text| text.to_string())
    }

    fn set_detail_tooltip(&self, tooltip: Option<String>) {
        if self.detail_tooltip() == tooltip {
            return;
        }
        self.detail.set_tooltip_text(tooltip.as_deref());
    }
}

#[glib::object_subclass]
impl ObjectSubclass for SplitRow {
    const NAME: &'static str = "SplitRow";
    type Type = super::SplitRow;
    type ParentType = gtk4::Widget;
    type Interfaces = (gtk4::Buildable,);

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for SplitRow {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("activated").build(),
                glib::subclass::Signal::builder("details").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let split = self.obj();

        self.row.connect_clicked(glib::clone!(
            #[weak]
            split,
            move |_| split.emit_by_name::<()>("activated", &[])
        ));
        self.detail.connect_clicked(glib::clone!(
            #[weak]
            split,
            move |_| split.emit_by_name::<()>("details", &[])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for SplitRow {}

impl BuildableImpl for SplitRow {
    fn add_child(&self, builder: &gtk4::Builder, child: &glib::Object, kind: Option<&str>) {
        match (kind, child.downcast_ref::<gtk4::Widget>()) {
            (Some("lead"), Some(widget)) => self.row.set_lead(widget),
            (Some("trail"), Some(widget)) => self.row.set_trail(widget),
            _ => self.parent_add_child(builder, child, kind),
        }
    }
}
