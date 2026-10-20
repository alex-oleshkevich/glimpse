use gtk4::{CompositeTemplate, glib, prelude::*, subclass::prelude::*};

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
        crate::row::wire_activation(
            row.upcast_ref::<gtk4::Widget>(),
            glib::clone!(
                #[weak]
                row,
                move || {
                    row.activate();
                }
            ),
        );
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for ActionRow {}

impl crate::row::RowImpl for ActionRow {}
