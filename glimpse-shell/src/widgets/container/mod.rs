mod imp;

use gtk4::{glib, prelude::*};
use relm4::{ContainerChild, RelmContainerExt};

glib::wrapper! {
    pub struct Container(ObjectSubclass<imp::Container>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

#[derive(Clone, Copy)]
pub enum Space {
    S1,
    S2,
    S3,
    S4,
    S5,
    S6,
    S7,
    S8,
    S9,
    S10,
}

impl Space {
    fn n(self) -> u8 {
        match self {
            Self::S1 => 1,
            Self::S2 => 2,
            Self::S3 => 3,
            Self::S4 => 4,
            Self::S5 => 5,
            Self::S6 => 6,
            Self::S7 => 7,
            Self::S8 => 8,
            Self::S9 => 9,
            Self::S10 => 10,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub enum Radius {
    #[default]
    None,
    Sm,
    Md,
    Lg,
    Pill,
}

impl Radius {
    fn css_class(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Sm => Some("rounded-sm"),
            Self::Md => Some("rounded-md"),
            Self::Lg => Some("rounded-lg"),
            Self::Pill => Some("rounded-pill"),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub enum ContainerBg {
    #[default]
    None,
    Surface,
    Raised,
}

impl ContainerBg {
    fn css_class(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Surface => Some("bg-surface"),
            Self::Raised => Some("bg-raised"),
        }
    }
}

impl Container {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_padding(&self, space: Space) {
        for n in 1u8..=10 {
            self.remove_css_class(&format!("p-{n}"));
        }
        self.add_css_class(&format!("p-{}", space.n()));
    }

    pub fn set_padding_x(&self, space: Space) {
        for n in 1u8..=10 {
            self.remove_css_class(&format!("px-{n}"));
        }
        self.add_css_class(&format!("px-{}", space.n()));
    }

    pub fn set_padding_y(&self, space: Space) {
        for n in 1u8..=10 {
            self.remove_css_class(&format!("py-{n}"));
        }
        self.add_css_class(&format!("py-{}", space.n()));
    }

    pub fn set_margin(&self, space: Space) {
        for n in 1u8..=10 {
            self.remove_css_class(&format!("m-{n}"));
        }
        self.add_css_class(&format!("m-{}", space.n()));
    }

    pub fn set_margin_x(&self, space: Space) {
        for n in 1u8..=10 {
            self.remove_css_class(&format!("mx-{n}"));
        }
        self.add_css_class(&format!("mx-{}", space.n()));
    }

    pub fn set_margin_y(&self, space: Space) {
        for n in 1u8..=10 {
            self.remove_css_class(&format!("my-{n}"));
        }
        self.add_css_class(&format!("my-{}", space.n()));
    }

    pub fn set_radius(&self, radius: Radius) {
        for class in ["rounded-sm", "rounded-md", "rounded-lg", "rounded-pill"] {
            self.remove_css_class(class);
        }
        if let Some(class) = radius.css_class() {
            self.add_css_class(class);
        }
    }

    pub fn set_bg(&self, bg: ContainerBg) {
        for class in ["bg-surface", "bg-raised"] {
            self.remove_css_class(class);
        }
        if let Some(class) = bg.css_class() {
            self.add_css_class(class);
        }
    }

    pub fn set_border_width(&self, width: u32) {
        for n in 1u32..=4 {
            self.remove_css_class(&format!("border-{n}"));
        }
        if width > 0 {
            self.add_css_class(&format!("border-{width}"));
        }
    }

    pub fn set_min_width(&self, space: Space) {
        for n in 1u8..=10 {
            self.remove_css_class(&format!("min-w-{n}"));
        }
        self.add_css_class(&format!("min-w-{}", space.n()));
    }

    pub fn set_min_height(&self, space: Space) {
        for n in 1u8..=10 {
            self.remove_css_class(&format!("min-h-{n}"));
        }
        self.add_css_class(&format!("min-h-{}", space.n()));
    }
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerChild for Container {
    type Child = gtk4::Widget;
}

impl RelmContainerExt for Container {
    fn container_add(&self, widget: &impl AsRef<gtk4::Widget>) {
        self.append(widget.as_ref());
    }
}
