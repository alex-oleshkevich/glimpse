use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};
use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/indicator.ui")]
pub struct Indicator {
    #[template_child]
    pub icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub badge: TemplateChild<gtk4::Label>,
    pub gicon: RefCell<Option<gio::Icon>>,
    pub accessible_name: RefCell<String>,
    pub attention: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for Indicator {
    const NAME: &'static str = "GlimpseIndicator";
    type Type = super::Indicator;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Button);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for Indicator {
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
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for Indicator {}
