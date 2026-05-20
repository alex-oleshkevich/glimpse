mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct Badge(ObjectSubclass<imp::Badge>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

#[derive(Clone, Copy)]
pub enum BadgeKind {
    Default,
    Success,
    Warning,
    Error,
    Accent,
}

impl BadgeKind {
    fn css_class(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error   => "error",
            Self::Accent  => "accent",
        }
    }
}

impl Badge {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_label(&self, text: &str) {
        self.imp().label.set_text(text);
    }

    pub fn set_kind(&self, kind: BadgeKind) {
        for class in ["default", "success", "warning", "error", "accent"] {
            self.remove_css_class(class);
        }
        self.add_css_class(kind.css_class());
    }
}

impl Default for Badge {
    fn default() -> Self {
        Self::new()
    }
}
