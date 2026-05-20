mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct Text(ObjectSubclass<imp::Text>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

#[derive(Clone, Copy, Default)]
pub enum FontSize {
    Xxs,
    Xs,
    Sm,
    #[default]
    Md,
    Lg,
    Xl,
}

impl FontSize {
    fn css_class(self) -> &'static str {
        match self {
            Self::Xxs => "text-xxs",
            Self::Xs  => "text-xs",
            Self::Sm  => "text-sm",
            Self::Md  => "text-md",
            Self::Lg  => "text-lg",
            Self::Xl  => "text-xl",
        }
    }
}

#[derive(Clone, Copy, Default)]
pub enum FontWeight {
    #[default]
    Normal,
    Medium,
    Semibold,
    Bold,
}

impl FontWeight {
    fn css_class(self) -> &'static str {
        match self {
            Self::Normal   => "text-normal",
            Self::Medium   => "text-medium",
            Self::Semibold => "text-semibold",
            Self::Bold     => "text-bold",
        }
    }
}

#[derive(Clone, Copy, Default)]
pub enum TextColor {
    #[default]
    Default,
    Muted,
    Accent,
    Success,
    Warning,
    Error,
}

impl TextColor {
    fn css_class(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::Muted   => Some("dim-label"),
            Self::Accent  => Some("text-accent"),
            Self::Success => Some("text-success"),
            Self::Warning => Some("text-warning"),
            Self::Error   => Some("text-error"),
        }
    }
}

impl Text {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_text(&self, text: &str) {
        self.imp().label.set_text(text);
    }

    pub fn set_size(&self, size: FontSize) {
        let label = &self.imp().label;
        for class in ["text-xxs", "text-xs", "text-sm", "text-md", "text-lg", "text-xl"] {
            label.remove_css_class(class);
        }
        label.add_css_class(size.css_class());
    }

    pub fn set_weight(&self, weight: FontWeight) {
        let label = &self.imp().label;
        for class in ["text-normal", "text-medium", "text-semibold", "text-bold"] {
            label.remove_css_class(class);
        }
        label.add_css_class(weight.css_class());
    }

    pub fn set_color(&self, color: TextColor) {
        let label = &self.imp().label;
        for class in ["dim-label", "text-accent", "text-success", "text-warning", "text-error"] {
            label.remove_css_class(class);
        }
        if let Some(class) = color.css_class() {
            label.add_css_class(class);
        }
    }

    pub fn set_xalign(&self, xalign: f32) {
        self.imp().label.set_xalign(xalign);
    }

    pub fn set_wrap(&self, wrap: bool) {
        self.imp().label.set_wrap(wrap);
    }
}

impl Default for Text {
    fn default() -> Self {
        Self::new()
    }
}
