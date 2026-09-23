use gtk4::{gdk, glib, graphene, prelude::*, subclass::prelude::*};
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::Swatch)]
    pub struct Swatch {
        #[property(get, set = Self::set_color, nullable, explicit_notify)]
        pub color: RefCell<Option<gdk::RGBA>>,
    }

    impl Swatch {
        fn set_color(&self, color: Option<gdk::RGBA>) {
            if *self.color.borrow() == color {
                return;
            }
            self.color.replace(color);
            self.obj().queue_draw();
            self.obj().notify_color();
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Swatch {
        const NAME: &'static str = "Swatch";
        type Type = super::Swatch;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("swatch");
            klass.set_accessible_role(gtk4::AccessibleRole::Presentation);
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for Swatch {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_overflow(gtk4::Overflow::Hidden);
        }
    }

    impl WidgetImpl for Swatch {
        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let Some(color) = *self.color.borrow() else {
                return;
            };
            let widget = self.obj();
            let bounds =
                graphene::Rect::new(0.0, 0.0, widget.width() as f32, widget.height() as f32);
            snapshot.append_color(&color, &bounds);
        }
    }
}

glib::wrapper! {
    pub struct Swatch(ObjectSubclass<imp::Swatch>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Swatch {
    fn default() -> Self {
        glib::Object::new()
    }
}

pub fn rgba([red, green, blue]: [u8; 3]) -> gdk::RGBA {
    gdk::RGBA::new(
        f32::from(red) / 255.0,
        f32::from(green) / 255.0,
        f32::from(blue) / 255.0,
        1.0,
    )
}
