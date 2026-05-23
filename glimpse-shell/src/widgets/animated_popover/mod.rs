mod imp;

use std::time::Duration;

use gtk4::{glib, prelude::*, subclass::prelude::*};
use relm4::{ContainerChild, RelmContainerExt};

use imp::AnimationState;

const OPEN_CLASS: &str = "animated-popover--open";
const CLOSING_CLASS: &str = "animated-popover--closing";
const ANIMATION_DURATION: Duration = Duration::from_millis(160);

glib::wrapper! {
    pub struct AnimatedPopover(ObjectSubclass<imp::AnimatedPopover>)
        @extends gtk4::Popover, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget,
                    gtk4::Native, gtk4::ShortcutManager;
}

impl AnimatedPopover {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn toggle(&self) {
        let state = self.imp().state.get();
        tracing::debug!(?state, "AnimatedPopover::toggle");
        match state {
            AnimationState::Closed | AnimationState::Closing => self.open(),
            AnimationState::Opening | AnimationState::Open => self.close(),
        }
    }

    pub fn open(&self) {
        tracing::debug!("AnimatedPopover::open");
        let imp = self.imp();
        let epoch = imp.generation.get().wrapping_add(1);
        imp.generation.set(epoch);
        imp.state.set(AnimationState::Opening);
        self.set_can_target(true);
        self.remove_css_class(OPEN_CLASS);
        self.remove_css_class(CLOSING_CLASS);
        self.popup();

        if !self.is_visible() {
            tracing::warn!("AnimatedPopover: not visible after popup()");
        }
        // The actual `OPEN_CLASS` flip happens in the `connect_show` handler
        // installed in `imp::AnimatedPopover::constructed`, deferred via
        // `idle_add_local_once`. That sequence — show → idle → add class —
        // guarantees one painted frame at the initial (opacity:0) state on
        // every popover size, including the small ones used by
        // battery/session. Tick / timeout heuristics aren't reliable across
        // sizes because GTK collapses layout+paint into a single frame for
        // small popovers.
    }

    pub fn close(&self) {
        let imp = self.imp();
        if imp.state.get() == AnimationState::Closed {
            return;
        }

        tracing::debug!("AnimatedPopover::close");
        let epoch = imp.generation.get().wrapping_add(1);
        imp.generation.set(epoch);
        imp.state.set(AnimationState::Closing);
        self.set_can_target(false);
        self.remove_css_class(OPEN_CLASS);
        self.add_css_class(CLOSING_CLASS);

        let weak = self.downgrade();
        glib::timeout_add_local_once(ANIMATION_DURATION, move || {
            let Some(w) = weak.upgrade() else { return };
            let imp = w.imp();
            if imp.generation.get() != epoch || imp.state.get() != AnimationState::Closing {
                return;
            }
            w.popdown();
            w.set_can_target(true);
            w.remove_css_class(CLOSING_CLASS);
            imp.state.set(AnimationState::Closed);
            tracing::debug!("AnimatedPopover: popdown complete");
        });
    }
}

impl Default for AnimatedPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerChild for AnimatedPopover {
    type Child = gtk4::Widget;
}

impl RelmContainerExt for AnimatedPopover {
    fn container_add(&self, widget: &impl AsRef<gtk4::Widget>) {
        self.set_child(Some(widget.as_ref()));
    }
}
