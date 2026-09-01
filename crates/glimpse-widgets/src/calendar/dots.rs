use gtk4::{gdk, glib, graphene, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};

pub const MAX: usize = 3;
const SIZE: f32 = 4.0;
const GAP: f32 = 2.0;

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Dots {
        pub colors: RefCell<Vec<gdk::RGBA>>,
        pub uniform: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Dots {
        const NAME: &'static str = "CalendarDots";
        type Type = super::Dots;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Dots {}

    impl WidgetImpl for Dots {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let width = (SIZE * MAX as f32 + GAP * (MAX - 1) as f32).ceil() as i32;
            let size = match orientation {
                gtk4::Orientation::Horizontal => width,
                _ => SIZE.ceil() as i32,
            };
            (size, size, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let colors = self.colors.borrow();
            let count = colors.len().min(MAX);
            if count == 0 {
                return;
            }

            let uniform = self.uniform.get().then(|| {
                let mut color = self.obj().color();
                color.set_alpha(color.alpha() * 0.75);
                color
            });

            let width = SIZE * count as f32 + GAP * (count - 1) as f32;
            let mut x = (self.obj().width() as f32 - width) / 2.0;
            let y = (self.obj().height() as f32 - SIZE) / 2.0;

            for color in colors.iter().take(count) {
                let color = uniform.as_ref().unwrap_or(color);
                let bounds = graphene::Rect::new(x, y, SIZE, SIZE);
                let rounded = gtk4::gsk::RoundedRect::from_rect(bounds, SIZE / 2.0);
                snapshot.push_rounded_clip(&rounded);
                snapshot.append_color(color, &bounds);
                snapshot.pop();
                x += SIZE + GAP;
            }
        }
    }
}

glib::wrapper! {
    pub struct Dots(ObjectSubclass<imp::Dots>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Dots {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_colors(&self, colors: &[gdk::RGBA]) {
        let imp = self.imp();
        if imp.colors.borrow().as_slice() == colors {
            return;
        }
        imp.colors.replace(colors.to_vec());
        self.queue_draw();
    }

    pub fn set_uniform(&self, uniform: bool) {
        if self.imp().uniform.replace(uniform) != uniform {
            self.queue_draw();
        }
    }
}
