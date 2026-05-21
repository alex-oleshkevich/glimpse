mod imp;

use std::path::Path;

use glib::closure_local;
use gtk4::{glib, prelude::*, subclass::prelude::*};
use relm4::{ContainerChild, RelmContainerExt};

glib::wrapper! {
    pub struct PanelIndicator(ObjectSubclass<imp::PanelIndicator>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget,
                    gtk4::Orientable;
}

impl PanelIndicator {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_icon(&self, icon: Option<&str>) {
        let image = &self.imp().icon;
        match icon.filter(|icon| !icon.is_empty()) {
            Some(icon) if is_icon_path(icon) => {
                image.set_from_file(Some(Path::new(icon)));
                image.set_visible(true);
            }
            Some(icon) => {
                image.set_icon_name(Some(icon));
                image.set_visible(true);
            }
            None => {
                image.set_icon_name(None::<&str>);
                image.set_visible(false);
            }
        }
    }

    pub fn set_label(&self, label: Option<&str>) {
        let label_widget = &self.imp().label;
        match label.filter(|label| !label.is_empty()) {
            Some(label) => {
                label_widget.set_label(label);
                label_widget.set_visible(true);
            }
            None => {
                label_widget.set_label("");
                label_widget.set_visible(false);
            }
        }
    }

    pub fn append_extra(&self, child: &impl IsA<gtk4::Widget>) {
        self.imp().extra_slot.append(child);
        self.sync_extra_visibility();
    }

    pub fn clear_extra(&self) {
        let slot = &self.imp().extra_slot;
        while let Some(child) = slot.first_child() {
            slot.remove(&child);
        }
        self.sync_extra_visibility();
    }

    pub fn set_extra_visible(&self, visible: bool) {
        self.imp().extra_visible.set(visible);
        self.sync_extra_visibility();
    }

    pub fn set_active(&self, active: bool) {
        set_class(self, "is-active", active);
    }

    pub fn set_checked(&self, checked: bool) {
        set_class(self, "is-checked", checked);
    }

    pub fn set_needs_attention(&self, attention: bool) {
        set_class(self, "needs-attention", attention);
    }

    pub fn connect_activated(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            closure_local!(move |indicator: &Self| f(indicator)),
        )
    }

    pub fn connect_middle_clicked(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "middle-clicked",
            false,
            closure_local!(move |indicator: &Self| f(indicator)),
        )
    }

    pub fn connect_secondary_clicked(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "secondary-clicked",
            false,
            closure_local!(move |indicator: &Self| f(indicator)),
        )
    }

    pub fn connect_scrolled(&self, f: impl Fn(&Self, f64, f64) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "scrolled",
            false,
            closure_local!(move |indicator: &Self, dx: f64, dy: f64| f(indicator, dx, dy)),
        )
    }

    fn sync_extra_visibility(&self) {
        let imp = self.imp();
        imp.extra_slot
            .set_visible(imp.extra_visible.get() && imp.extra_slot.first_child().is_some());
    }
}

impl Default for PanelIndicator {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerChild for PanelIndicator {
    type Child = gtk4::Widget;
}

impl RelmContainerExt for PanelIndicator {
    fn container_add(&self, widget: &impl AsRef<gtk4::Widget>) {
        self.append_extra(widget.as_ref());
    }
}

fn is_icon_path(icon: &str) -> bool {
    icon.starts_with('/') || icon.starts_with("./") || icon.starts_with("../") || icon.contains('/')
}

fn set_class(widget: &impl WidgetExt, class: &str, active: bool) {
    if active {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn panel_indicator_has_shell_classes() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let indicator = PanelIndicator::new();

        assert!(indicator.has_css_class("applet"));
        assert!(indicator.has_css_class("panel-indicator"));
        assert_eq!(indicator.orientation(), gtk4::Orientation::Horizontal);
        assert_eq!(indicator.valign(), gtk4::Align::Center);
    }

    #[test]
    fn icon_and_label_hide_when_empty() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let indicator = PanelIndicator::new();

        indicator.set_icon(Some("system-shutdown-symbolic"));
        indicator.set_label(Some("Power"));
        assert!(indicator.imp().icon.is_visible());
        assert!(indicator.imp().label.is_visible());

        indicator.set_icon(None);
        indicator.set_label(Some(""));
        assert!(!indicator.imp().icon.is_visible());
        assert!(!indicator.imp().label.is_visible());
    }

    #[test]
    fn extra_slot_visibility_tracks_children_and_override() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let indicator = PanelIndicator::new();
        let child = gtk4::Label::new(Some("2"));

        assert!(!indicator.imp().extra_slot.is_visible());
        indicator.set_extra_visible(false);
        indicator.append_extra(&child);
        assert!(!indicator.imp().extra_slot.is_visible());

        indicator.set_extra_visible(true);
        assert!(indicator.imp().extra_slot.is_visible());

        indicator.clear_extra();
        assert!(!indicator.imp().extra_slot.is_visible());
    }

    #[test]
    fn state_helpers_toggle_css_classes() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let indicator = PanelIndicator::new();

        indicator.set_active(true);
        indicator.set_checked(true);
        indicator.set_needs_attention(true);
        assert!(indicator.has_css_class("is-active"));
        assert!(indicator.has_css_class("is-checked"));
        assert!(indicator.has_css_class("needs-attention"));

        indicator.set_active(false);
        indicator.set_checked(false);
        indicator.set_needs_attention(false);
        assert!(!indicator.has_css_class("is-active"));
        assert!(!indicator.has_css_class("is-checked"));
        assert!(!indicator.has_css_class("needs-attention"));
    }

    #[test]
    fn icon_path_detection_matches_existing_applet_behavior() {
        assert!(is_icon_path("/tmp/icon.svg"));
        assert!(is_icon_path("./icon.svg"));
        assert!(is_icon_path("../icon.svg"));
        assert!(is_icon_path("icons/icon.svg"));
        assert!(!is_icon_path("system-shutdown-symbolic"));
    }
}
