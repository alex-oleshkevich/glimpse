use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use crate::{Indicator, TrayChip, tray_strip::Edge};

#[derive(Debug, Default)]
pub struct TrayStrip {
    pub visible_box: RefCell<Option<gtk4::Box>>,
    pub chevron: RefCell<Option<gtk4::ToggleButton>>,
    pub drawer: RefCell<Option<gtk4::Revealer>>,
    pub hidden_box: RefCell<Option<gtk4::Box>>,
    pub chips: RefCell<Vec<TrayChip>>,
    pub overflow_tooltip: RefCell<Option<String>>,
    pub shown: RefCell<Vec<(String, Indicator)>>,
    pub hidden: RefCell<Vec<(String, Indicator)>>,
    pub max_visible: Cell<u32>,
    pub edge: Cell<Edge>,
    pub vertical: Cell<bool>,
    pub pressed: glib::WeakRef<Indicator>,
    pub accessible_name: RefCell<String>,
}

#[glib::object_subclass]
impl ObjectSubclass for TrayStrip {
    const NAME: &'static str = "TrayStrip";
    type Type = super::TrayStrip;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::Group);
    }
}

impl ObjectImpl for TrayStrip {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("activated")
                    .param_types([String::static_type(), u32::static_type()])
                    .build(),
                glib::subclass::Signal::builder("scrolled")
                    .param_types([
                        String::static_type(),
                        f64::static_type(),
                        f64::static_type(),
                    ])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let strip = self.obj();
        strip.add_css_class("tray-strip");
        strip.set_visible(false);

        if let Some(layout) = strip.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_spacing(super::SPACING as u32);
        }

        let shown = gtk4::Box::new(gtk4::Orientation::Horizontal, super::SPACING);
        shown.set_parent(&*strip);
        self.visible_box.replace(Some(shown));

        let chevron = gtk4::ToggleButton::builder()
            .has_frame(false)
            .visible(false)
            .build();
        chevron.add_css_class("tray-strip__chevron");
        chevron.set_parent(&*strip);
        self.chevron.replace(Some(chevron.clone()));

        let hidden = gtk4::Box::new(gtk4::Orientation::Horizontal, super::SPACING);
        let drawer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideLeft)
            .reveal_child(false)
            .child(&hidden)
            .build();
        drawer.set_parent(&*strip);
        self.drawer.replace(Some(drawer.clone()));
        self.hidden_box.replace(Some(hidden));

        chevron.connect_toggled(move |chevron| {
            crate::drawer::set(&drawer, chevron.is_active());
            crate::set_css_class(chevron, super::CHEVRON_OPEN, chevron.is_active());
        });

        strip.arrange();
    }

    fn dispose(&self) {
        while let Some(child) = self.obj().first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for TrayStrip {}
