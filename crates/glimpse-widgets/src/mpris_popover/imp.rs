use std::{cell::RefCell, sync::OnceLock};

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{AppletPopover, LegacyRow, NowPlaying, PlayerList, Section};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/mpris_popover.ui")]
pub struct MprisPopover {
    #[template_child]
    pub card: TemplateChild<AppletPopover>,
    #[template_child]
    pub scroller: TemplateChild<gtk4::ScrolledWindow>,
    #[template_child]
    pub player: TemplateChild<NowPlaying>,
    #[template_child]
    pub others: TemplateChild<Section>,
    #[template_child]
    pub list: TemplateChild<PlayerList>,
    #[template_child]
    pub footer: TemplateChild<LegacyRow>,
    entered: RefCell<Option<(gtk4::gdk::Surface, glib::SignalHandlerId)>>,
}

#[glib::object_subclass]
impl ObjectSubclass for MprisPopover {
    const NAME: &'static str = "MprisPopover";
    type Type = super::MprisPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for MprisPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("footer-activated").build(),
                glib::subclass::Signal::builder("raise-requested")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("toggle-requested")
                    .param_types([String::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();

        self.footer.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("footer-activated", &[])
        ));

        self.list.connect_activated(glib::clone!(
            #[weak]
            popover,
            move |_, key| popover.emit_by_name::<()>("raise-requested", &[&key])
        ));
        self.list.connect_toggled(glib::clone!(
            #[weak]
            popover,
            move |_, key| popover.emit_by_name::<()>("toggle-requested", &[&key])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl MprisPopover {
    fn apply_cap(&self, entered: Option<&gtk4::gdk::Monitor>) {
        let Some(surface) = self.obj().native().and_then(|native| native.surface()) else {
            return;
        };
        let Some(monitor) = entered
            .cloned()
            .or_else(|| surface.display().monitor_at_surface(&surface))
        else {
            return;
        };
        let (_, card, _, _) = self.card.measure(gtk4::Orientation::Vertical, -1);
        let (_, scroller, _, _) = self.scroller.measure(gtk4::Orientation::Vertical, -1);
        let chrome = card.saturating_sub(scroller);
        let cap = crate::popover_shell::body_cap(monitor.geometry().height(), chrome);
        if self.scroller.max_content_height() != cap {
            self.scroller.set_max_content_height(cap);
        }
    }
}

impl WidgetImpl for MprisPopover {
    fn map(&self) {
        self.parent_map();
        self.apply_cap(None);
        let object = self.obj();
        if let Some(surface) = object.native().and_then(|native| native.surface()) {
            let handler = surface.connect_enter_monitor(glib::clone!(
                #[weak]
                object,
                move |_, monitor| object.imp().apply_cap(Some(monitor))
            ));
            if let Some((old, handler)) = self.entered.replace(Some((surface, handler))) {
                old.disconnect(handler);
            }
        }
    }

    fn unmap(&self) {
        if let Some((surface, handler)) = self.entered.take() {
            surface.disconnect(handler);
        }
        self.parent_unmap();
    }
}
