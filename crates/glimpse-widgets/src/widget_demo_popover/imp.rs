use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{
    ActionRow, ExpanderRow, MeterRow, PopoverShell, PopoverSize, Row, SliderRow, ToggleRow,
};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/widget_demo_popover.ui")]
pub struct WidgetDemoPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub volume: TemplateChild<SliderRow>,
    #[template_child]
    pub mute: TemplateChild<gtk4::Button>,
}

#[glib::object_subclass]
impl ObjectSubclass for WidgetDemoPopover {
    const NAME: &'static str = "WidgetDemoPopover";
    type Type = super::WidgetDemoPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        PopoverShell::static_type();
        crate::Container::static_type();
        Row::static_type();
        ActionRow::static_type();
        ToggleRow::static_type();
        MeterRow::static_type();
        ExpanderRow::static_type();
        SliderRow::static_type();
        crate::Section::static_type();
        klass.bind_template();
        klass.bind_template_callbacks();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

#[gtk4::template_callbacks]
impl WidgetDemoPopover {
    #[template_callback]
    fn on_wide_toggled(&self, _: &glib::ParamSpec, switch: &gtk4::Switch) {
        self.shell.set_size(match switch.is_active() {
            true => PopoverSize::Wide,
            false => PopoverSize::Regular,
        });
    }

    #[template_callback]
    fn on_mute(&self, _: &gtk4::Button) {
        let muted = !self.volume.inactive();
        self.volume.set_inactive(muted);
        self.volume.set_subtitle(if muted {
            Some(gettextrs::gettext("Muted"))
        } else {
            None
        });
        self.mute.set_tooltip_text(Some(&if muted {
            gettextrs::gettext("Unmute")
        } else {
            gettextrs::gettext("Mute")
        }));
        self.mute.set_icon_name(if muted {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-medium-symbolic"
        });
    }
}

impl ObjectImpl for WidgetDemoPopover {
    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for WidgetDemoPopover {}
