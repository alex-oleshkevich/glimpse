use gtk4::{glib, subclass::prelude::*, CompositeTemplate, TemplateChild};

#[derive(CompositeTemplate, Default)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/battery_hero.ui")]
pub struct BatteryHero {
    #[template_child]
    pub(super) icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub(super) percentage: TemplateChild<gtk4::Label>,
    #[template_child]
    pub(super) progress: TemplateChild<gtk4::ProgressBar>,
    #[template_child]
    pub(super) state: TemplateChild<gtk4::Label>,
}

#[glib::object_subclass]
impl ObjectSubclass for BatteryHero {
    const NAME: &'static str = "BatteryHero";
    type Type = super::BatteryHero;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for BatteryHero {}
impl WidgetImpl for BatteryHero {}
impl BoxImpl for BatteryHero {}
