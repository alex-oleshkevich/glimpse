use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};

use crate::{ActionRow, ExpanderRow, ToggleRow};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/me/aresa/GlimpseShell/widgets/network_row.ui")]
    pub struct NetworkRow {
        #[template_child]
        pub expander: TemplateChild<ExpanderRow>,
        #[template_child]
        pub status: TemplateChild<gtk4::Image>,
        #[template_child]
        pub connection: TemplateChild<ActionRow>,
        #[template_child]
        pub facts: TemplateChild<gtk4::Box>,
        #[template_child]
        pub autoconnect: TemplateChild<ToggleRow>,
        #[template_child]
        pub forget: TemplateChild<ActionRow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NetworkRow {
        const NAME: &'static str = "NetworkRow";
        type Type = super::NetworkRow;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            ExpanderRow::static_type();
            ActionRow::static_type();
            ToggleRow::static_type();
            klass.bind_template();
            klass.set_accessible_role(gtk4::AccessibleRole::Group);
        }

        fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
            object.init_template();
        }
    }

    impl ObjectImpl for NetworkRow {
        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for NetworkRow {}
}

glib::wrapper! {
    pub struct NetworkRow(ObjectSubclass<imp::NetworkRow>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}
