use std::cell::Cell;

use gtk4::{glib, prelude::*, subclass::prelude::*};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum AnimationState {
    #[default]
    Closed,
    Opening,
    Open,
    Closing,
}

#[derive(Default)]
pub struct AnimatedPopover {
    pub(super) state: Cell<AnimationState>,
    pub(super) generation: Cell<u64>,
}

#[glib::object_subclass]
impl ObjectSubclass for AnimatedPopover {
    const NAME: &'static str = "GlimpseAnimatedPopover";
    type Type = super::AnimatedPopover;
    type ParentType = gtk4::Popover;
}

impl ObjectImpl for AnimatedPopover {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.add_css_class("animated-popover");

        let weak = obj.downgrade();
        obj.connect_closed(move |_| {
            if let Some(w) = weak.upgrade() {
                let imp = w.imp();
                tracing::debug!(state = ?imp.state.get(), "AnimatedPopover: GTK closed signal fired");
                imp.state.set(AnimationState::Closed);
                w.set_can_target(true);
                w.remove_css_class("animated-popover--open");
                w.remove_css_class("animated-popover--closing");
            }
        });
    }
}

impl WidgetImpl for AnimatedPopover {}
impl PopoverImpl for AnimatedPopover {}
