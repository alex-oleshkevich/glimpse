mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct Indicator(ObjectSubclass<imp::Indicator>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

pub(crate) const LABEL_MAX_CHARS: usize = 64;
pub(crate) const TOOLTIP_MAX_CHARS: usize = 256;
const ATTENTION_CLASS: &str = "indicator--attention";

#[derive(Debug, Default, Clone)]
pub struct IndicatorSpec {
    pub id: String,
    pub icon: Option<gio::Icon>,
    pub label: Option<String>,
    pub tooltip: Option<String>,
    pub badge: Option<String>,
    pub attention: bool,
}

impl Default for Indicator {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn apply(&self, spec: &IndicatorSpec) {
        let tooltip = spec
            .tooltip
            .as_deref()
            .map(|tooltip| truncate(tooltip, TOOLTIP_MAX_CHARS));
        if self.tooltip_text().as_deref() != tooltip.as_deref() {
            self.set_tooltip_text(tooltip.as_deref());
        }
        self.set_icon(spec.icon.as_ref());
        self.set_label(spec.label.as_deref());
        self.set_badge(spec.badge.as_deref());
        self.set_attention(spec.attention);
    }

    pub fn set_icon(&self, icon: Option<&gio::Icon>) {
        let imp = self.imp();
        if icons_equal(imp.gicon.borrow().as_ref(), icon) {
            return;
        }
        imp.gicon.replace(icon.cloned());
        match icon {
            Some(icon) => imp.icon.set_from_gicon(icon),
            None => imp.icon.clear(),
        }
        imp.icon.set_visible(icon.is_some());
    }

    pub fn set_label(&self, label: Option<&str>) {
        set_text(&self.imp().label, label);
        self.sync_accessible_label();
    }

    pub fn set_badge(&self, badge: Option<&str>) {
        set_text(&self.imp().badge, badge);
    }

    pub fn set_attention(&self, attention: bool) {
        if self.imp().attention.replace(attention) == attention {
            return;
        }
        if attention {
            self.add_css_class(ATTENTION_CLASS);
        } else {
            self.remove_css_class(ATTENTION_CLASS);
        }
    }

    pub fn connect_pressed<F: Fn(&Self, u32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "pressed",
            false,
            glib::closure_local!(move |indicator: Self, button: u32| f(&indicator, button)),
        )
    }

    pub fn connect_scrolled<F: Fn(&Self, f64, f64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "scrolled",
            false,
            glib::closure_local!(move |indicator: Self, dx: f64, dy: f64| f(&indicator, dx, dy)),
        )
    }

    fn sync_accessible_label(&self) {
        let label = self.imp().label.text();
        let name = if label.is_empty() {
            self.tooltip_text().unwrap_or_default().to_string()
        } else {
            label.to_string()
        };
        if *self.imp().accessible_name.borrow() == name {
            return;
        }
        self.imp().accessible_name.replace(name.clone());
        if name.is_empty() {
            self.reset_property(gtk4::AccessibleProperty::Label);
        } else {
            self.update_property(&[gtk4::accessible::Property::Label(&name)]);
        }
    }
}

fn icons_equal(current: Option<&gio::Icon>, next: Option<&gio::Icon>) -> bool {
    match (current, next) {
        (None, None) => true,
        (Some(current), Some(next)) => current.equal(Some(next)),
        _ => false,
    }
}

fn set_text(label: &gtk4::Label, value: Option<&str>) {
    let text = truncate(value.unwrap_or_default(), LABEL_MAX_CHARS);
    if label.text().as_str() == text {
        return;
    }
    label.set_text(&text);
    label.set_visible(!text.is_empty());
}

fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}
