use std::{
    cell::{Cell, RefCell},
    sync::OnceLock,
};

use glib::subclass::Signal;
use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

pub struct PanelIndicator {
    pub(super) icon: gtk4::Image,
    pub(super) label: gtk4::Label,
    pub(super) extra_slot: gtk4::Box,
    pub(super) extra_visible: Cell<bool>,
    /// CSS classes installed by `PanelIndicator::set_extra_classes`. Tracked
    /// separately so the diff-based updater can remove only the classes it
    /// owns without touching foundational classes (`applet`, `panel-indicator`)
    /// or runtime state classes (`is-active`, `is-checked`, `needs-attention`).
    pub(super) extra_classes: RefCell<Vec<String>>,
}

impl Default for PanelIndicator {
    fn default() -> Self {
        Self {
            icon: gtk4::Image::new(),
            label: gtk4::Label::new(None),
            extra_slot: gtk4::Box::new(gtk4::Orientation::Horizontal, 4),
            extra_visible: Cell::new(true),
            extra_classes: RefCell::new(Vec::new()),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for PanelIndicator {
    const NAME: &'static str = "GlimpsePanelIndicator";
    type Type = super::PanelIndicator;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for PanelIndicator {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();

        obj.add_css_class("applet");
        obj.add_css_class("panel-indicator");
        obj.set_orientation(gtk4::Orientation::Horizontal);
        obj.set_spacing(4);
        obj.set_valign(gtk4::Align::Center);
        obj.set_halign(gtk4::Align::Center);
        obj.set_hexpand(false);

        self.icon.add_css_class("panel-indicator__icon");
        self.icon.set_pixel_size(16);
        self.icon.set_valign(gtk4::Align::Center);
        self.icon.set_visible(false);
        obj.append(&self.icon);

        self.label.add_css_class("panel-indicator__label");
        self.label.set_hexpand(true);
        self.label.set_halign(gtk4::Align::Fill);
        self.label.set_valign(gtk4::Align::Center);
        self.label.set_xalign(0.5);
        self.label.set_visible(false);
        obj.append(&self.label);

        self.extra_slot.add_css_class("panel-indicator__extra");
        self.extra_slot.set_valign(gtk4::Align::Center);
        self.extra_slot.set_visible(false);
        obj.append(&self.extra_slot);

        install_click_signal(&obj, gdk::BUTTON_PRIMARY, "activated", "activated-at");
        install_click_signal(
            &obj,
            gdk::BUTTON_MIDDLE,
            "middle-clicked",
            "middle-clicked-at",
        );
        install_click_signal(
            &obj,
            gdk::BUTTON_SECONDARY,
            "secondary-clicked",
            "secondary-clicked-at",
        );

        let scroll = gtk4::EventControllerScroll::new(
            gtk4::EventControllerScrollFlags::BOTH_AXES
                | gtk4::EventControllerScrollFlags::DISCRETE,
        );
        let weak = obj.downgrade();
        scroll.connect_scroll(move |_, dx, dy| {
            if dx == 0.0 && dy == 0.0 {
                return glib::Propagation::Proceed;
            }

            if let Some(indicator) = weak.upgrade() {
                indicator.emit_by_name::<()>("scrolled", &[&dx, &dy]);
            }
            glib::Propagation::Stop
        });
        obj.add_controller(scroll);
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("activated").build(),
                Signal::builder("activated-at")
                    .param_types([f64::static_type(), f64::static_type()])
                    .build(),
                Signal::builder("middle-clicked").build(),
                Signal::builder("middle-clicked-at")
                    .param_types([f64::static_type(), f64::static_type()])
                    .build(),
                Signal::builder("secondary-clicked").build(),
                Signal::builder("secondary-clicked-at")
                    .param_types([f64::static_type(), f64::static_type()])
                    .build(),
                Signal::builder("scrolled")
                    .param_types([f64::static_type(), f64::static_type()])
                    .build(),
            ]
        })
    }
}

impl WidgetImpl for PanelIndicator {}
impl BoxImpl for PanelIndicator {}

fn install_click_signal(
    obj: &super::PanelIndicator,
    button: u32,
    signal_name: &'static str,
    position_signal_name: &'static str,
) {
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(button);
    let weak = obj.downgrade();
    gesture.connect_pressed(move |gesture, _, x, y| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        if let Some(indicator) = weak.upgrade() {
            indicator.emit_by_name::<()>(signal_name, &[]);
            indicator.emit_by_name::<()>(position_signal_name, &[&x, &y]);
        }
    });
    obj.add_controller(gesture);
}
