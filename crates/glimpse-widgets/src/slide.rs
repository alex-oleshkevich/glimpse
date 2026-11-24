use std::cell::{Cell, OnceCell, RefCell};

use adw::prelude::AnimationExt;
use gtk4::{gdk, glib, graphene, prelude::*, subclass::prelude::*};

const SLIDE_MS: u32 = 200;

mod imp {
    use super::*;

    pub struct Slide {
        pub child: RefCell<Option<gtk4::Widget>>,
        pub edge: Cell<gtk4::PositionType>,
        pub progress: Cell<f64>,
        pub animation: OnceCell<adw::TimedAnimation>,
        pub done: RefCell<Option<Box<dyn FnOnce()>>>,
    }

    impl Default for Slide {
        fn default() -> Self {
            Self {
                child: RefCell::default(),
                edge: Cell::new(gtk4::PositionType::Top),
                progress: Cell::new(1.0),
                animation: OnceCell::new(),
                done: RefCell::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Slide {
        const NAME: &'static str = "Slide";
        type Type = super::Slide;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk4::BinLayout>();
            klass.set_css_name("slide");
        }
    }

    impl ObjectImpl for Slide {
        fn constructed(&self) {
            self.parent_constructed();
            let widget = self.obj();
            let target = adw::CallbackAnimationTarget::new({
                let widget = widget.downgrade();
                move |value| {
                    if let Some(widget) = widget.upgrade() {
                        widget.set_progress(value);
                    }
                }
            });
            let animation = adw::TimedAnimation::new(&*widget, 1.0, 1.0, SLIDE_MS, target);
            animation.connect_done({
                let widget = widget.downgrade();
                move |_| {
                    let Some(widget) = widget.upgrade() else {
                        return;
                    };
                    let done = widget.imp().done.take();
                    if let Some(done) = done {
                        done();
                    }
                }
            });
            let _ = self.animation.set(animation);
        }

        fn dispose(&self) {
            if let Some(child) = self.child.take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Slide {
        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let widget = self.obj();
            let (width, height) = (widget.width() as f32, widget.height() as f32);
            let bounds = graphene::Rect::new(0.0, 0.0, width, height);
            let progress = self.progress.get();
            let child = self.child.borrow().clone();
            let Some(child) = child.filter(|_| progress > 0.0) else {
                snapshot.append_color(&gdk::RGBA::TRANSPARENT, &bounds);
                return;
            };
            let hidden = (1.0 - progress) as f32;
            let (x, y) = match self.edge.get() {
                gtk4::PositionType::Top => (0.0, -hidden * height),
                gtk4::PositionType::Bottom => (0.0, hidden * height),
                gtk4::PositionType::Left => (-hidden * width, 0.0),
                _ => (hidden * width, 0.0),
            };
            snapshot.push_clip(&bounds);
            snapshot.save();
            snapshot.translate(&graphene::Point::new(x, y));
            if progress < 1.0 {
                snapshot.push_opacity(progress);
                widget.snapshot_child(&child, snapshot);
                snapshot.pop();
            } else {
                widget.snapshot_child(&child, snapshot);
            }
            snapshot.restore();
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct Slide(ObjectSubclass<imp::Slide>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Slide {
    fn default() -> Self {
        Self::new()
    }
}

impl Slide {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_child(&self, child: Option<&impl IsA<gtk4::Widget>>) {
        let imp = self.imp();
        let child = child.map(|child| child.as_ref().clone());
        if *imp.child.borrow() == child {
            return;
        }
        if let Some(old) = imp.child.take() {
            old.unparent();
        }
        if let Some(child) = &child {
            child.set_parent(self);
            child.set_can_target(imp.progress.get() >= 1.0);
        }
        imp.child.replace(child);
    }

    pub fn set_edge(&self, edge: gtk4::PositionType) {
        let imp = self.imp();
        if imp.edge.get() == edge {
            return;
        }
        imp.edge.set(edge);
        self.queue_draw();
    }

    pub fn shown(&self) -> bool {
        self.imp().progress.get() > 0.0
    }

    pub fn duration(&self) -> u32 {
        crate::animation_ms(SLIDE_MS)
    }

    pub fn reveal(&self, shown: bool, done: impl FnOnce() + 'static) {
        let imp = self.imp();
        let Some(animation) = imp.animation.get() else {
            return;
        };
        imp.done.replace(Some(Box::new(done)));
        self.drive(animation, imp.progress.get(), shown);
    }

    pub fn drive(&self, animation: &adw::TimedAnimation, from: f64, shown: bool) {
        animation.set_value_from(from);
        animation.reset();
        animation.set_value_to(if shown { 1.0 } else { 0.0 });
        animation.set_duration(self.duration());
        animation.set_easing(match shown {
            true => adw::Easing::EaseOutCubic,
            false => adw::Easing::EaseInCubic,
        });
        animation.play();
    }

    pub fn jump(&self, shown: bool) {
        let imp = self.imp();
        imp.done.replace(None);
        let target = if shown { 1.0 } else { 0.0 };
        if let Some(animation) = imp.animation.get() {
            animation.set_value_from(target);
            animation.reset();
        }
        self.set_progress(target);
    }

    fn set_progress(&self, progress: f64) {
        let imp = self.imp();
        if imp.progress.get() == progress {
            return;
        }
        imp.progress.set(progress);
        if let Some(child) = imp.child.borrow().as_ref() {
            let target = progress >= 1.0;
            if child.can_target() != target {
                child.set_can_target(target);
            }
        }
        self.queue_draw();
    }
}
