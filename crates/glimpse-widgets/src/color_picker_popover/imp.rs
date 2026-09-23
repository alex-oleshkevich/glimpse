use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{AccessibleRole, CompositeTemplate, glib, prelude::*, subclass::prelude::*};

use crate::{ColorList, Hero, Placeholder, PopoverShell, Row, Section, Swatch};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/color_picker_popover.ui")]
pub struct ColorPickerPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub latest: TemplateChild<Swatch>,
    #[template_child]
    pub palette_section: TemplateChild<Section>,
    #[template_child]
    pub palette: TemplateChild<ColorList>,
    #[template_child]
    pub nothing: TemplateChild<Placeholder>,
    #[template_child]
    pub footer: TemplateChild<Row>,
    pub resting: RefCell<Option<String>>,
}

#[glib::object_subclass]
impl ObjectSubclass for ColorPickerPopover {
    const NAME: &'static str = "ColorPickerPopover";
    type Type = super::ColorPickerPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        PopoverShell::static_type();
        Hero::static_type();
        Section::static_type();
        Placeholder::static_type();
        ColorList::static_type();
        Swatch::static_type();
        Row::static_type();
        klass.set_layout_manager_type::<gtk4::BinLayout>();
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ColorPickerPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("activated")
                    .param_types([u64::static_type()])
                    .build(),
                glib::subclass::Signal::builder("copied")
                    .param_types([u64::static_type(), String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("footer-activated").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        self.resting.replace(self.hero.title());
        let obj = self.obj();
        self.palette.connect_activated(glib::clone!(
            #[weak]
            obj,
            move |_, id| obj.emit_by_name::<()>("activated", &[&id])
        ));
        self.palette.connect_copied(glib::clone!(
            #[weak]
            obj,
            move |_, id, key| obj.emit_by_name::<()>("copied", &[&id, &key])
        ));
        self.footer.connect_clicked(glib::clone!(
            #[weak]
            obj,
            move |_| obj.emit_by_name::<()>("footer-activated", &[])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for ColorPickerPopover {}
