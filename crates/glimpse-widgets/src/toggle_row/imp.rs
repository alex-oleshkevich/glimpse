use gtk4::{
    CompositeTemplate, TemplateChild, accessible, gdk, glib, prelude::*, subclass::prelude::*,
};
use std::cell::Cell;

use crate::set_css_class;

const ON: &str = "row--on";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "ToggleControl")]
pub enum ToggleControl {
    Check,
    #[default]
    Switch,
}

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::ToggleRow)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/toggle_row.ui")]
pub struct ToggleRow {
    #[template_child]
    pub knob: TemplateChild<gtk4::Switch>,
    #[property(name = "control", get, set = Self::set_control, builder(ToggleControl::Switch).explicit_notify())]
    control: Cell<ToggleControl>,
    #[property(name = "active", get, set = Self::set_active, explicit_notify)]
    active: Cell<bool>,
    #[property(get, set = Self::set_locked, explicit_notify)]
    locked: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for ToggleRow {
    const NAME: &'static str = "ToggleRow";
    type Type = super::ToggleRow;
    type ParentType = crate::Row;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(gtk4::AccessibleRole::Switch);
        klass.set_activate_signal_from_name("activate");
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for ToggleRow {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: std::sync::OnceLock<Vec<glib::subclass::Signal>> =
            std::sync::OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("activate")
                    .action()
                    .run_first()
                    .class_handler(|values| {
                        if let Ok(row) = values[0].get::<super::ToggleRow>() {
                            row.imp().activate_from_user();
                        }
                        None
                    })
                    .build(),
                glib::subclass::Signal::builder("toggled")
                    .param_types([bool::static_type()])
                    .run_first()
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        if self.knob.can_target() {
            self.knob.set_can_target(false);
        }
        if self.knob.can_focus() {
            self.knob.set_can_focus(false);
        }
        if self.knob.is_focusable() {
            self.knob.set_focusable(false);
        }
        self.wire_input();
        self.sync_check();
        self.sync_role();
        self.sync_state();
        self.obj()
            .connect_sensitive_notify(|row| row.imp().sync_disabled());
        self.sync_disabled();
    }

    fn dispose(&self) {
        if self.knob.parent().is_none() {
            self.obj()
                .upcast_ref::<crate::Row>()
                .imp()
                .end()
                .append(&self.knob.get());
        }
        self.dispose_template();
    }
}

impl WidgetImpl for ToggleRow {}

impl crate::row::RowImpl for ToggleRow {}

impl ToggleRow {
    pub(crate) fn activate_from_user(&self) {
        if self.locked.get() || !self.obj().is_sensitive() {
            return;
        }
        let next = !self.active.get();
        self.apply_active(next);
        self.obj().emit_by_name::<()>("toggled", &[&next]);
    }

    fn set_control(&self, control: ToggleControl) {
        if self.control.replace(control) == control {
            return;
        }
        self.place_knob(control == ToggleControl::Switch);
        self.sync_check();
        self.sync_role();
        self.obj().notify_control();
    }

    fn set_active(&self, active: bool) {
        self.apply_active(active);
    }

    fn set_locked(&self, locked: bool) {
        if self.locked.replace(locked) == locked {
            return;
        }
        self.knob.set_sensitive(!locked);
        self.sync_disabled();
        self.obj().notify_locked();
    }

    fn sync_disabled(&self) {
        self.obj().update_state(&[accessible::State::Disabled(
            self.locked.get() || !self.obj().is_sensitive(),
        )]);
    }

    fn apply_active(&self, active: bool) {
        if self.active.replace(active) == active {
            return;
        }
        if self.knob.is_active() != active {
            self.knob.set_active(active);
        }
        self.sync_check();
        self.sync_state();
        self.obj().notify_active();
    }

    fn place_knob(&self, in_end: bool) {
        let widget = self.obj();
        let row = widget.upcast_ref::<crate::Row>();
        if in_end {
            reveal_knob(&self.knob, true);
            row.set_end(&self.knob.get());
        } else if self.knob.parent().is_some() {
            self.knob.unparent();
            reveal_knob(&self.knob, false);
            row.imp().sync_slots();
            row.imp().sync_accessible();
        }
    }

    fn sync_check(&self) {
        let on = self.control.get() == ToggleControl::Check && self.active.get();
        let check = self.obj().upcast_ref::<crate::Row>().imp().check();
        if check.get_visible() != on {
            check.set_visible(on);
        }
        set_css_class(&*self.obj(), ON, on);
    }

    fn sync_role(&self) {
        let role = match self.control.get() {
            ToggleControl::Check => gtk4::AccessibleRole::Checkbox,
            ToggleControl::Switch => gtk4::AccessibleRole::Switch,
        };
        if self.obj().accessible_role() != role {
            self.obj().set_accessible_role(role);
        }
    }

    fn sync_state(&self) {
        let checked = if self.active.get() {
            gtk4::AccessibleTristate::True
        } else {
            gtk4::AccessibleTristate::False
        };
        self.obj()
            .update_state(&[accessible::State::Checked(checked)]);
    }

    fn wire_input(&self) {
        let row = self.obj();
        let click = gtk4::GestureClick::new();
        click.set_button(gdk::BUTTON_PRIMARY);
        click.connect_released(glib::clone!(
            #[weak]
            row,
            move |gesture, _n_press, x, y| {
                if row.contains(x, y) {
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                    row.imp().activate_from_user();
                }
            }
        ));
        let keys = gtk4::EventControllerKey::new();
        let pressed = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        keys.connect_key_pressed(glib::clone!(
            #[weak]
            row,
            #[strong]
            pressed,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_controller, key, _code, _mods| {
                if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::space) {
                    let mut held = pressed.borrow_mut();
                    if !held.contains(&key) {
                        held.push(key);
                        drop(held);
                        row.imp().activate_from_user();
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
}

fn reveal_knob(knob: &gtk4::Switch, visible: bool) {
    if knob.get_visible() != visible {
        knob.set_visible(visible);
    }
}
