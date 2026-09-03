use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use super::{PagerItem, Shape, Slot, step};

#[derive(Debug, Default)]
pub struct Pager {
    pub slots: RefCell<Vec<Slot>>,
    pub items: RefCell<Vec<PagerItem>>,
    pub shape: Cell<Shape>,
    pub pressed: glib::WeakRef<PagerItem>,
}

#[glib::object_subclass]
impl ObjectSubclass for Pager {
    const NAME: &'static str = "Pager";
    type Type = super::Pager;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::Group);
    }
}

impl ObjectImpl for Pager {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("pressed").build(),
                glib::subclass::Signal::builder("stepped")
                    .param_types([bool::static_type(), bool::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let pager = self.obj();
        pager.add_css_class("pager");
        pager.set_orientation(gtk4::Orientation::Horizontal);
        pager.set_visible(false);
        pager.apply_shape();

        let click = gtk4::GestureClick::new();
        click.set_button(gtk4::gdk::BUTTON_PRIMARY);
        click.connect_released(glib::clone!(
            #[weak]
            pager,
            move |_, _, x, y| {
                pager.imp().pressed.set(pager.item_at(x, y).as_ref());
                pager.emit_by_name::<()>("pressed", &[]);
            }
        ));
        pager.add_controller(click);

        let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::BOTH_AXES);
        scroll.connect_scroll(glib::clone!(
            #[weak]
            pager,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, dx, dy| {
                let Some((horizontal, forward)) = step(dx, dy) else {
                    return glib::Propagation::Proceed;
                };
                pager.emit_by_name::<()>("stepped", &[&horizontal, &forward]);
                glib::Propagation::Stop
            }
        ));
        pager.add_controller(scroll);
    }

    fn dispose(&self) {
        for item in self.items.borrow_mut().drain(..) {
            item.unparent();
        }
    }
}

impl WidgetImpl for Pager {}
