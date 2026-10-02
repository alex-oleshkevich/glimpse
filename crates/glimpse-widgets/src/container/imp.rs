use std::cell::{Cell, RefCell};

use gtk4::{AccessibleRole, CompositeTemplate, glib, prelude::*, subclass::prelude::*};

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "ContainerPaint")]
pub enum Paint {
    #[default]
    None,
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Light,
    Dark,
}

#[derive(Debug, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::Container)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/container.ui")]
pub struct Container {
    #[property(get, set = Self::set_width, default = -1, explicit_notify)]
    width: Cell<i32>,
    #[property(get, set = Self::set_height, default = -1, explicit_notify)]
    height: Cell<i32>,
    #[property(get, set = Self::set_background, builder(Paint::None).explicit_notify())]
    background: Cell<Paint>,
    #[property(get, set = Self::set_color, builder(Paint::None).explicit_notify())]
    color: Cell<Paint>,
    #[property(get, set = Self::set_align, builder(gtk4::Align::Fill).explicit_notify())]
    align: Cell<gtk4::Align>,
    pub child: RefCell<Option<gtk4::Widget>>,
}

impl Default for Container {
    fn default() -> Self {
        Self {
            width: Cell::new(-1),
            height: Cell::new(-1),
            background: Cell::new(Paint::None),
            color: Cell::new(Paint::None),
            align: Cell::new(gtk4::Align::Fill),
            child: RefCell::new(None),
        }
    }
}

impl Container {
    fn set_width(&self, width: i32) {
        let width = width.clamp(-1, 1024);
        if self.width.get() == width {
            return;
        }
        self.width.set(width);
        self.apply_size();
        self.obj().notify_width();
    }

    fn set_height(&self, height: i32) {
        let height = height.clamp(-1, 1024);
        if self.height.get() == height {
            return;
        }
        self.height.set(height);
        self.apply_size();
        self.obj().notify_height();
    }

    fn apply_size(&self) {
        self.obj()
            .set_size_request(self.width.get(), self.height.get());
    }

    fn set_background(&self, paint: Paint) {
        if self.background.get() == paint {
            return;
        }
        let previous = self.background.replace(paint);
        let container = self.obj();
        if let Some(class) = paint_class("bg", previous) {
            container.remove_css_class(class);
        }
        if let Some(class) = paint_class("bg", paint) {
            container.add_css_class(class);
        }
        container.notify_background();
    }

    fn set_color(&self, paint: Paint) {
        if self.color.get() == paint {
            return;
        }
        let previous = self.color.replace(paint);
        let container = self.obj();
        if let Some(class) = paint_class("fg", previous) {
            container.remove_css_class(class);
        }
        if let Some(class) = paint_class("fg", paint) {
            container.add_css_class(class);
        }
        container.notify_color();
    }

    fn set_align(&self, align: gtk4::Align) {
        if self.align.get() == align {
            return;
        }
        self.align.set(align);
        if let Some(child) = self.child.borrow().clone() {
            self.place(&child);
        }
        self.obj().notify_align();
    }

    pub(super) fn place(&self, widget: &gtk4::Widget) {
        let align = self.align.get();
        if widget.halign() != align {
            widget.set_halign(align);
        }
        if widget.valign() != align {
            widget.set_valign(align);
        }
    }
}

fn paint_class(kind: &str, paint: Paint) -> Option<&'static str> {
    Some(match (kind, paint) {
        ("bg", Paint::Red) => "container--bg-red",
        ("bg", Paint::Orange) => "container--bg-orange",
        ("bg", Paint::Yellow) => "container--bg-yellow",
        ("bg", Paint::Green) => "container--bg-green",
        ("bg", Paint::Blue) => "container--bg-blue",
        ("bg", Paint::Purple) => "container--bg-purple",
        ("bg", Paint::Light) => "container--bg-light",
        ("bg", Paint::Dark) => "container--bg-dark",
        ("fg", Paint::Red) => "container--fg-red",
        ("fg", Paint::Orange) => "container--fg-orange",
        ("fg", Paint::Yellow) => "container--fg-yellow",
        ("fg", Paint::Green) => "container--fg-green",
        ("fg", Paint::Blue) => "container--fg-blue",
        ("fg", Paint::Purple) => "container--fg-purple",
        ("fg", Paint::Light) => "container--fg-light",
        ("fg", Paint::Dark) => "container--fg-dark",
        _ => return None,
    })
}

#[glib::object_subclass]
impl ObjectSubclass for Container {
    const NAME: &'static str = "Container";
    type Type = super::Container;
    type ParentType = gtk4::Widget;
    type Interfaces = (gtk4::Buildable,);

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for Container {
    fn dispose(&self) {
        if let Some(child) = self.child.borrow_mut().take() {
            child.unparent();
        }
        self.dispose_template();
    }
}

impl WidgetImpl for Container {}

impl BuildableImpl for Container {
    fn add_child(&self, builder: &gtk4::Builder, child: &glib::Object, kind: Option<&str>) {
        match (kind, child.downcast_ref::<gtk4::Widget>()) {
            (None, Some(widget)) => self.obj().set_child(Some(widget)),
            _ => self.parent_add_child(builder, child, kind),
        }
    }
}
