mod imp;

use gtk4::{
    CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION, glib, prelude::*,
    subclass::prelude::ObjectSubclassIsExt,
};

use super::css_color::sanitize_css_color;

glib::wrapper! {
    pub struct StatusDot(ObjectSubclass<imp::StatusDot>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusDotStatus {
    Neutral,
    Success,
    Warning,
    Error,
    Accent,
}

impl StatusDotStatus {
    pub fn css_class(self) -> &'static str {
        match self {
            Self::Neutral => "is-neutral",
            Self::Success => "is-success",
            Self::Warning => "is-warning",
            Self::Error => "is-danger",
            Self::Accent => "is-accent",
        }
    }

    const ALL_CLASSES: &'static [&'static str] = &[
        "is-neutral",
        "is-success",
        "is-warning",
        "is-danger",
        "is-accent",
    ];
}

impl StatusDot {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_status(&self, status: StatusDotStatus) {
        self.clear_color_provider();
        for class in StatusDotStatus::ALL_CLASSES {
            self.remove_css_class(class);
        }
        self.add_css_class(status.css_class());
    }

    pub fn set_color(&self, color: Option<&str>) -> bool {
        self.clear_color_provider();
        let Some(value) = color.and_then(sanitize_css_color) else {
            return false;
        };

        let provider = CssProvider::new();
        provider.load_from_string(&format!(".status-dot {{ color: {value}; }}"));
        #[allow(deprecated)]
        self.style_context()
            .add_provider(&provider, STYLE_PROVIDER_PRIORITY_APPLICATION);
        *self.imp().provider.borrow_mut() = Some(provider);
        true
    }

    fn clear_color_provider(&self) {
        let Some(provider) = self.imp().provider.borrow_mut().take() else {
            return;
        };
        #[allow(deprecated)]
        self.style_context().remove_provider(&provider);
    }
}

impl Default for StatusDot {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::gtk_available_on_this_thread;

    #[test]
    fn set_color_installs_custom_color_provider() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let dot = StatusDot::new();

        dot.set_color(Some("#4285f4"));

        assert!(dot.imp().provider.borrow().is_some());
        assert!(dot.has_css_class("status-dot"));
    }

    #[test]
    fn set_status_clears_custom_color_provider() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let dot = StatusDot::new();

        dot.set_color(Some("#4285f4"));
        dot.set_status(StatusDotStatus::Warning);

        assert!(dot.imp().provider.borrow().is_none());
        assert!(dot.has_css_class("is-warning"));
    }

    #[test]
    fn set_color_rejects_css_injection() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let dot = StatusDot::new();

        assert!(!dot.set_color(Some("red; background: blue")));
        assert!(dot.imp().provider.borrow().is_none());
    }
}
