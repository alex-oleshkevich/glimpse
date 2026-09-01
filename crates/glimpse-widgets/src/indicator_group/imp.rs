use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};
use std::cell::RefCell;
use std::sync::OnceLock;

use crate::indicator::Indicator;

const SPACING: u32 = 4;
const PRIMARY_BUTTON: u32 = 1;

#[derive(Debug, Default)]
pub struct IndicatorGroup {
    pub items: RefCell<Vec<Indicator>>,
    pub accessible_name: RefCell<String>,
}

#[glib::object_subclass]
impl ObjectSubclass for IndicatorGroup {
    const NAME: &'static str = "IndicatorGroup";
    type Type = super::IndicatorGroup;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::Button);
    }
}

impl ObjectImpl for IndicatorGroup {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("pressed")
                    .param_types([u32::static_type()])
                    .build(),
                glib::subclass::Signal::builder("scrolled")
                    .param_types([f64::static_type(), f64::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.add_css_class("indicator-group");
        obj.set_visible(false);
        obj.set_focusable(true);
        if let Some(layout) = obj.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_spacing(SPACING);
        }

        let click = gtk4::GestureClick::new();
        click.set_button(0);
        click.connect_pressed(glib::clone!(
            #[weak]
            obj,
            move |gesture, _, _, _| {
                obj.emit_by_name::<()>("pressed", &[&gesture.current_button()]);
            }
        ));
        obj.add_controller(click);

        let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::BOTH_AXES);
        scroll.connect_scroll(glib::clone!(
            #[weak]
            obj,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, dx, dy| {
                if dx == 0.0 && dy == 0.0 {
                    return glib::Propagation::Proceed;
                }
                obj.emit_by_name::<()>("scrolled", &[&dx, &dy]);
                glib::Propagation::Stop
            }
        ));
        obj.add_controller(scroll);

        let keys = gtk4::EventControllerKey::new();
        keys.connect_key_pressed(glib::clone!(
            #[weak]
            obj,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| {
                if !matches!(
                    key,
                    gtk4::gdk::Key::Return
                        | gtk4::gdk::Key::KP_Enter
                        | gtk4::gdk::Key::space
                        | gtk4::gdk::Key::KP_Space
                ) {
                    return glib::Propagation::Proceed;
                }
                obj.emit_by_name::<()>("pressed", &[&PRIMARY_BUTTON]);
                glib::Propagation::Stop
            }
        ));
        obj.add_controller(keys);
    }

    fn dispose(&self) {
        for indicator in self.items.borrow_mut().drain(..) {
            indicator.unparent();
        }
    }
}

impl WidgetImpl for IndicatorGroup {}
