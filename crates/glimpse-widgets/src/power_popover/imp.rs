use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use super::{ChargeLimit, Device, History};
use crate::{
    ActionRow, ChoiceList, ExpanderRow, Notice, PopoverShell, Readout, Row, RowTone, Section,
    Sparkline, ToggleRow, clear_children, none_if_empty,
};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/power_popover.ui")]
pub struct PowerPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub readout: TemplateChild<Readout>,
    #[template_child]
    pub notice: TemplateChild<Notice>,
    #[template_child]
    pub remaining: TemplateChild<Row>,
    #[template_child]
    pub battery: TemplateChild<ExpanderRow>,
    #[template_child]
    pub facts: TemplateChild<gtk4::Box>,
    #[template_child]
    pub charge_limit: TemplateChild<ToggleRow>,
    #[template_child]
    pub history: TemplateChild<ExpanderRow>,
    #[template_child]
    pub sparkline: TemplateChild<Sparkline>,
    #[template_child]
    pub profiles_section: TemplateChild<Section>,
    #[template_child]
    pub profiles: TemplateChild<ChoiceList>,
    #[template_child]
    pub keep: TemplateChild<Section>,
    #[template_child]
    pub hold: TemplateChild<ToggleRow>,
    #[template_child]
    pub hold_card: TemplateChild<ExpanderRow>,
    #[template_child]
    pub preset_15m: TemplateChild<ToggleRow>,
    #[template_child]
    pub preset_30m: TemplateChild<ToggleRow>,
    #[template_child]
    pub preset_1h: TemplateChild<ToggleRow>,
    #[template_child]
    pub preset_2h: TemplateChild<ToggleRow>,
    #[template_child]
    pub preset_4h: TemplateChild<ToggleRow>,
    #[template_child]
    pub devices_section: TemplateChild<Section>,
    #[template_child]
    pub devices_box: TemplateChild<gtk4::Box>,

    pub(super) devices: RefCell<Vec<Device>>,
    pub(super) history_data: RefCell<Option<History>>,
    pub(super) facts_empty: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for PowerPopover {
    const NAME: &'static str = "PowerPopover";
    type Type = super::PowerPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        PopoverShell::static_type();
        Readout::static_type();
        Notice::static_type();
        Row::static_type();
        ExpanderRow::static_type();
        ToggleRow::static_type();
        Sparkline::static_type();
        Section::static_type();
        ChoiceList::static_type();
        ActionRow::static_type();
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for PowerPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("profile-activated")
                    .param_types([u32::static_type()])
                    .build(),
                glib::subclass::Signal::builder("charge-limit-toggled")
                    .param_types([bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder("hold-toggled")
                    .param_types([bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder("hold-requested")
                    .param_types([u32::static_type()])
                    .build(),
                glib::subclass::Signal::builder("history-opened").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        self.facts_empty.set(true);
        self.sparkline.set_domain(Some((0.0, 100.0)));
        self.sparkline.set_fill(true);
        let popover = self.obj();

        self.profiles.connect_activated(glib::clone!(
            #[weak]
            popover,
            move |_, index| popover.emit_by_name::<()>("profile-activated", &[&index])
        ));
        self.charge_limit.connect_toggled(glib::clone!(
            #[weak]
            popover,
            move |_, on| popover.emit_by_name::<()>("charge-limit-toggled", &[&on])
        ));
        self.hold.connect_toggled(glib::clone!(
            #[weak]
            popover,
            move |_, on| popover.emit_by_name::<()>("hold-toggled", &[&on])
        ));
        self.history.connect_toggled(glib::clone!(
            #[weak]
            popover,
            move |_, open| {
                if open {
                    popover.emit_by_name::<()>("history-opened", &[]);
                }
            }
        ));

        crate::wire_hold_presets(
            self.presets().map(|(row, _)| row),
            glib::clone!(
                #[weak]
                popover,
                move |seconds| {
                    popover.set_hold_preset(Some(seconds));
                    popover.emit_by_name::<()>("hold-requested", &[&seconds]);
                }
            ),
        );
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for PowerPopover {}

impl PowerPopover {
    pub(crate) fn presets(&self) -> [(ToggleRow, u32); 5] {
        let rows = [
            self.preset_15m.get(),
            self.preset_30m.get(),
            self.preset_1h.get(),
            self.preset_2h.get(),
            self.preset_4h.get(),
        ];
        std::array::from_fn(|i| (rows[i].clone(), crate::HOLD_PRESETS[i]))
    }

    pub(super) fn show_widget(&self, widget: &impl gtk4::prelude::IsA<gtk4::Widget>, show: bool) {
        let widget = widget.upcast_ref::<gtk4::Widget>();
        if widget.get_visible() != show {
            widget.set_visible(show);
        }
    }

    pub(super) fn sync_battery(&self) {
        let show = !self.facts_empty.get() || self.charge_limit.get_visible();
        if !show && self.battery.open() {
            self.battery.set_open(false);
        }
        self.show_widget(&*self.battery, show);
    }

    pub(super) fn place_devices(&self, devices: &[Device]) {
        clear_children(&self.devices_box);
        for device in devices {
            let row = Row::new();
            row.set_icon_name(none_if_empty(&device.icon_name));
            row.set_title(none_if_empty(&device.name));
            row.set_value(none_if_empty(&device.value));
            if device.warning {
                row.set_tone(RowTone::Warning);
            }
            self.devices_box.append(&row);
        }
    }

    pub(super) fn apply_limit(&self, limit: Option<&ChargeLimit>) {
        self.show_widget(&*self.charge_limit, limit.is_some());
        if let Some(limit) = limit {
            self.charge_limit
                .upcast_ref::<Row>()
                .set_title(none_if_empty(&limit.title));
            if self.charge_limit.active() != limit.enabled {
                self.charge_limit.set_active(limit.enabled);
            }
        }
        self.sync_battery();
    }
}
