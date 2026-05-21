use glib::subclass::Signal;
use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::{cell::RefCell, sync::OnceLock};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/hero.ui")]
pub struct Hero {
    #[template_child]
    pub icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub subtitle: TemplateChild<gtk4::Label>,
    #[template_child]
    pub trailing: TemplateChild<gtk4::Box>,
    #[template_child]
    pub toggle: TemplateChild<gtk4::Switch>,
    #[template_child]
    pub separator: TemplateChild<gtk4::Separator>,
    pub state_set_handler: RefCell<Option<glib::SignalHandlerId>>,
}

#[glib::object_subclass]
impl ObjectSubclass for Hero {
    const NAME: &'static str = "Hero";
    type Type = super::Hero;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for Hero {
    fn constructed(&self) {
        self.parent_constructed();

        let obj = self.obj().downgrade();
        let handler = self.toggle.connect_state_set(move |_, state| {
            if let Some(hero) = obj.upgrade() {
                hero.emit_by_name::<()>("toggled", &[&state]);
            }
            glib::Propagation::Proceed
        });
        self.state_set_handler.replace(Some(handler));
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("toggled")
                    .param_types([bool::static_type()])
                    .build(),
            ]
        })
    }
}

impl WidgetImpl for Hero {}
impl BoxImpl for Hero {}
