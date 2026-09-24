mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::icons_equal;

glib::wrapper! {
    pub struct Indicator(ObjectSubclass<imp::Indicator>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

pub(crate) const LABEL_MAX_CHARS: usize = 64;
pub(crate) const TOOLTIP_MAX_CHARS: usize = 256;
const ATTENTION_CLASS: &str = "indicator--attention";
const NOTICE_CLASS: &str = "indicator--notice";
const WARNING_CLASS: &str = "indicator--warning";
const ERROR_CLASS: &str = "indicator--error";
const TEXT_CLASS: &str = "indicator--text";
pub(crate) const DOT_SIZE: f32 = 7.0;

#[derive(Debug, Default, Clone)]
pub struct IndicatorSpec {
    pub icon: Option<gio::Icon>,
    /// An emblem on the icon's trailing corner, for a state the icon itself does not carry.
    pub overlay: Option<gio::Icon>,
    pub dot: Option<gtk4::gdk::RGBA>,
    pub extension: Option<gtk4::Widget>,
    pub label: Option<String>,
    pub tooltip: Option<String>,
    pub badge: Option<String>,
    pub attention: bool,
    /// The calm counterpart to `attention`: something worth noticing, not something demanding it.
    /// Both can be true, and attention wins.
    pub notice: bool,
    /// What the chip is reporting, when it is reporting a condition rather than a reading.
    /// `None` leaves it in the bar's own colour; `Info` is a state worth an icon and no colour.
    pub severity: Option<crate::Severity>,
    pub class: Option<String>,
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
        self.set_overlay(spec.overlay.as_ref());
        self.set_dot(spec.dot);
        self.set_extension(spec.extension.as_ref());
        self.set_label(spec.label.as_deref());
        self.set_badge(spec.badge.as_deref());
        self.set_attention(spec.attention);
        self.set_notice(spec.notice);
        self.set_severity(spec.severity);
        self.set_class(spec.class.as_deref());
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
        imp.icon_slot.set_visible(icon.is_some());
        self.sync_text_only();
    }

    pub fn set_overlay(&self, overlay: Option<&gio::Icon>) {
        let imp = self.imp();
        if icons_equal(imp.overlay_icon.borrow().as_ref(), overlay) {
            return;
        }
        imp.overlay_icon.replace(overlay.cloned());
        match overlay {
            Some(overlay) => imp.overlay.set_from_gicon(overlay),
            None => imp.overlay.clear(),
        }
        imp.overlay.set_visible(overlay.is_some());
    }

    pub fn set_dot(&self, color: Option<gtk4::gdk::RGBA>) {
        let imp = self.imp();
        if imp.color.replace(color) == color {
            return;
        }
        match color {
            Some(color) => imp.dot.set_colors(&[color]),
            None => imp.dot.set_colors(&[]),
        }
        imp.dot.set_visible(color.is_some());
    }

    pub fn set_extension(&self, widget: Option<&gtk4::Widget>) {
        let slot = &self.imp().extension;
        if slot.first_child().as_ref() == widget {
            return;
        }
        while let Some(child) = slot.first_child() {
            slot.remove(&child);
        }
        if let Some(widget) = widget {
            if let Some(parent) = widget.parent().and_downcast::<gtk4::Box>() {
                parent.remove(widget);
            }
            slot.append(widget);
        }
        slot.set_visible(widget.is_some());
    }

    pub fn set_label(&self, label: Option<&str>) {
        set_text(&self.imp().label, label);
        self.sync_text_only();
    }

    fn sync_text_only(&self) {
        let imp = self.imp();
        let text_only = imp.label.get_visible() && !imp.icon_slot.get_visible();
        if self.has_css_class(TEXT_CLASS) != text_only {
            crate::set_css_class(self, TEXT_CLASS, text_only);
        }
    }

    pub fn set_badge(&self, badge: Option<&str>) {
        set_text(&self.imp().badge, badge);
        self.sync_attention_dot();
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
        self.sync_attention_dot();
    }

    pub fn set_notice(&self, notice: bool) {
        if self.imp().notice.replace(notice) == notice {
            return;
        }
        crate::set_css_class(self, NOTICE_CLASS, notice);
    }

    fn sync_attention_dot(&self) {
        let imp = self.imp();
        let shown = imp.attention.get() && !imp.badge.get_visible();
        if imp.attention_dot.get_visible() != shown {
            imp.attention_dot.set_visible(shown);
        }
    }
}

impl Indicator {
    pub fn set_severity(&self, severity: Option<crate::Severity>) {
        if self.imp().severity.replace(severity) == severity {
            return;
        }
        self.remove_css_class(WARNING_CLASS);
        self.remove_css_class(ERROR_CLASS);
        match severity {
            Some(crate::Severity::Warning) => self.add_css_class(WARNING_CLASS),
            Some(crate::Severity::Error) => self.add_css_class(ERROR_CLASS),
            Some(crate::Severity::Info) | None => {}
        }
    }

    pub fn set_class(&self, class: Option<&str>) {
        let previous = self.imp().class.replace(class.map(str::to_owned));
        if previous.as_deref() == class {
            return;
        }
        if let Some(previous) = previous {
            self.remove_css_class(&previous);
        }
        if let Some(class) = class {
            self.add_css_class(class);
        }
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

pub(crate) fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}
