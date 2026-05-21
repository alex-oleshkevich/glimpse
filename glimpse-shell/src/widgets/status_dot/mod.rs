mod imp;

use gtk4::{glib, prelude::*};

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
        for class in StatusDotStatus::ALL_CLASSES {
            self.remove_css_class(class);
        }
        self.add_css_class(status.css_class());
    }
}

impl Default for StatusDot {
    fn default() -> Self {
        Self::new()
    }
}
