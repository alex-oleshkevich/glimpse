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
        // Common defaults shared by every animated popover. Size is NOT
        // defaulted — every consumer must apply one of
        // `popover-size-{small|medium|large|xlarge|xxlarge}` explicitly.
        obj.set_hexpand(false);
        obj.set_autohide(true);

        let closed_weak = obj.downgrade();
        obj.connect_closed(move |_| {
            if let Some(w) = closed_weak.upgrade() {
                let imp = w.imp();
                tracing::debug!(state = ?imp.state.get(), "AnimatedPopover: GTK closed signal fired");
                imp.state.set(AnimationState::Closed);
                w.set_can_target(true);
                w.remove_css_class("animated-popover--open");
                w.remove_css_class("animated-popover--closing");
            }
        });

        // Drive the open transition off `show` rather than tick/timeout.
        // GTK fires `show` synchronously inside `popup()`, but the closure
        // we register with `idle_add_local_once` runs AFTER the current main
        // loop iteration completes — by which point GTK has done at least
        // one layout pass at the initial (opacity:0) state. Adding
        // `OPEN_CLASS` then triggers the CSS transition reliably on every
        // popover size, including `popover-size-small`.
        let show_weak = obj.downgrade();
        obj.connect_show(move |_| {
            let Some(w) = show_weak.upgrade() else { return };
            let imp = w.imp();
            if imp.state.get() != AnimationState::Opening {
                return;
            }
            let epoch = imp.generation.get();
            let idle_weak = w.downgrade();
            glib::idle_add_local_once(move || {
                let Some(w) = idle_weak.upgrade() else { return };
                let imp = w.imp();
                if imp.generation.get() != epoch || imp.state.get() != AnimationState::Opening {
                    return;
                }
                w.add_css_class("animated-popover--open");
                imp.state.set(AnimationState::Open);
                tracing::debug!("AnimatedPopover: open class applied (via show+idle)");
            });
        });
    }
}

impl WidgetImpl for AnimatedPopover {}
impl PopoverImpl for AnimatedPopover {}
