mod imp;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

pub use imp::{RowTone, RowTypography};

glib::wrapper! {
    pub struct Row(ObjectSubclass<imp::Row>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Row {
    fn default() -> Self {
        Self::new()
    }
}

impl Row {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_start(&self, widget: &impl IsA<gtk4::Widget>) {
        self.imp().set_start(widget);
    }

    pub fn clear_start(&self) {
        self.imp().clear_start();
    }

    pub fn set_end(&self, widget: &impl IsA<gtk4::Widget>) {
        self.imp().set_end(widget);
    }

    pub fn clear_end(&self) {
        crate::clear_children(&self.imp().end());
        self.imp().sync_slots();
        self.imp().sync_accessible();
    }

    pub(crate) fn set_chevron(&self, visible: bool) {
        if self.imp().chevron.get_visible() != visible {
            self.imp().chevron.set_visible(visible);
        }
    }
}

pub(crate) fn wire_activation(row: &gtk4::Widget, activate: impl Fn() + 'static) {
    let row = row.clone();
    let activate = std::rc::Rc::new(activate);
    let click = gtk4::GestureClick::new();
    click.set_button(gtk4::gdk::BUTTON_PRIMARY);
    click.connect_released(glib::clone!(
        #[weak]
        row,
        #[strong]
        activate,
        move |gesture, _n_press, x, y| {
            if row.contains(x, y) {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                activate();
            }
        }
    ));
    let keys = gtk4::EventControllerKey::new();
    let pressed = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    keys.connect_key_pressed(glib::clone!(
        #[strong]
        pressed,
        #[strong]
        activate,
        move |_controller, key, _code, _mods| {
            if matches!(
                key,
                gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter | gtk4::gdk::Key::space
            ) {
                let mut held = pressed.borrow_mut();
                if !held.contains(&key) {
                    held.push(key);
                    drop(held);
                    activate();
                }
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    ));
    keys.connect_key_released(move |_, key, _, _| {
        pressed.borrow_mut().retain(|held| *held != key);
    });
    row.add_controller(click);
    row.add_controller(keys);
}

pub trait RowImpl: WidgetImpl {}

unsafe impl<T: RowImpl> IsSubclassable<T> for Row {}
