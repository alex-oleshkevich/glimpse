use std::cell::Cell;

use gtk4::{glib, gsk, prelude::*, subclass::prelude::*};

const FILL_CLASS: &str = "applet-popover__arrow-fill";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tip {
    #[default]
    Up,
    Down,
    Left,
    Right,
}

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct PopoverArrow {
        pub tip: Cell<Tip>,
        pub fill: gtk4::Box,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PopoverArrow {
        const NAME: &'static str = "PopoverArrow";
        type Type = super::PopoverArrow;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk4::BinLayout>();
            klass.set_css_name("popover-arrow");
            klass.set_accessible_role(gtk4::AccessibleRole::Presentation);
        }
    }

    impl ObjectImpl for PopoverArrow {
        fn constructed(&self) {
            self.parent_constructed();
            self.fill.add_css_class(FILL_CLASS);
            self.fill.set_parent(&*self.obj());
        }

        fn dispose(&self) {
            self.fill.unparent();
        }
    }

    impl WidgetImpl for PopoverArrow {
        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let widget = self.obj();
            let path = triangle(
                self.tip.get(),
                widget.width() as f32,
                widget.height() as f32,
            );
            snapshot.push_fill(&path, gsk::FillRule::Winding);
            widget.snapshot_child(&self.fill, snapshot);
            snapshot.pop();
        }
    }

    fn triangle(tip: Tip, width: f32, height: f32) -> gsk::Path {
        let [first, second, third] = match tip {
            Tip::Up => [(width / 2.0, 0.0), (width, height), (0.0, height)],
            Tip::Down => [(width / 2.0, height), (width, 0.0), (0.0, 0.0)],
            Tip::Left => [(0.0, height / 2.0), (width, 0.0), (width, height)],
            Tip::Right => [(width, height / 2.0), (0.0, 0.0), (0.0, height)],
        };
        let builder = gsk::PathBuilder::new();
        builder.move_to(first.0, first.1);
        builder.line_to(second.0, second.1);
        builder.line_to(third.0, third.1);
        builder.close();
        builder.to_path()
    }
}

glib::wrapper! {
    pub struct PopoverArrow(ObjectSubclass<imp::PopoverArrow>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for PopoverArrow {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl PopoverArrow {
    pub fn new(tip: Tip) -> Self {
        let arrow = Self::default();
        arrow.imp().tip.set(tip);
        arrow
    }

    pub fn set_tip(&self, tip: Tip) {
        if self.imp().tip.get() == tip {
            return;
        }
        self.imp().tip.set(tip);
        self.queue_draw();
    }

    pub fn tip(&self) -> Tip {
        self.imp().tip.get()
    }

    pub fn set_fill_opacity(&self, opacity: f64) {
        self.imp().fill.set_opacity(opacity);
    }
}
