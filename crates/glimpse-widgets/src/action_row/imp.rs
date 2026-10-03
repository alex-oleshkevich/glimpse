use gtk4::{CompositeTemplate, gdk, glib, prelude::*, subclass::prelude::*};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/action_row.ui")]
pub struct ActionRow;

#[glib::object_subclass]
impl ObjectSubclass for ActionRow {
    const NAME: &'static str = "ActionRow";
    type Type = super::ActionRow;
    type ParentType = crate::Row;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(gtk4::AccessibleRole::Button);
        klass.set_activate_signal_from_name("activated");
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ActionRow {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: std::sync::OnceLock<Vec<glib::subclass::Signal>> =
            std::sync::OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("activated")
                    .action()
                    .run_first()
                    .class_handler(|values| {
                        let inactive = values[0]
                            .get::<super::ActionRow>()
                            .ok()
                            .filter(|row| !row.is_sensitive());
                        if let Some(row) = inactive {
                            row.stop_signal_emission_by_name("activated");
                        }
                        None
                    })
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let row = self.obj();
        let click = gtk4::GestureClick::new();
        click.set_button(gdk::BUTTON_PRIMARY);
        click.connect_released(glib::clone!(
            #[weak]
            row,
            move |gesture, _n_press, x, y| {
                if row.contains(x, y) {
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                    row.activate();
                }
            }
        ));
        let keys = gtk4::EventControllerKey::new();
        let pressed = Rc::new(RefCell::new(Vec::new()));
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
                        row.activate();
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

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for ActionRow {}

impl crate::row::RowImpl for ActionRow {}
