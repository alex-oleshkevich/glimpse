mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Indicator, IndicatorSpec, reconcile::by_key};

pub(crate) const SPACING: i32 = 4;
const CHEVRON_OPEN: &str = "tray-strip__chevron--open";
const CHEVRON_MARKED: &str = "tray-strip__chevron--marked";

/// Which end of the strip the overflow chevron is pinned to. The hidden chips always grow *away*
/// from it, into the bar rather than off the edge of the screen, so a right-zone applet on a
/// horizontal panel wants `Start` and a left-zone one wants `End`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Edge {
    #[default]
    Start,
    End,
}

/// One tray item as the strip renders it. The key routes a press or a scroll back to whatever owns
/// the item; the strip itself never interprets it.
#[derive(Debug, Default, Clone)]
pub struct TrayChip {
    pub key: String,
    pub spec: IndicatorSpec,
}

glib::wrapper! {
    pub struct TrayStrip(ObjectSubclass<imp::TrayStrip>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for TrayStrip {
    fn default() -> Self {
        Self::new()
    }
}

impl TrayStrip {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// `0` keeps every chip on the strip and hides the chevron.
    pub fn set_max_visible(&self, max: u32) {
        if self.imp().max_visible.replace(max) == max {
            return;
        }
        self.render();
    }

    pub fn set_overflow_edge(&self, edge: Edge) {
        if self.imp().edge.replace(edge) == edge {
            return;
        }
        self.arrange();
    }

    pub fn set_orientation(&self, orientation: gtk4::Orientation) {
        let imp = self.imp();
        imp.vertical.set(orientation == gtk4::Orientation::Vertical);
        for layout in [self.layout_manager().and_downcast::<gtk4::BoxLayout>()]
            .into_iter()
            .flatten()
        {
            layout.set_orientation(orientation);
        }
        for boxed in [
            imp.visible_box.borrow().clone(),
            imp.hidden_box.borrow().clone(),
        ]
        .into_iter()
        .flatten()
        {
            boxed.set_orientation(orientation);
        }
        self.arrange();
    }

    /// The chevron leads and the drawer opens behind it, so the hidden chips appear between the
    /// chevron and the chips that were already on the bar rather than beyond the screen edge. Its
    /// icon points back over the closed drawer and the `--open` rotation turns it the way the
    /// chips travelled.
    fn arrange(&self) {
        let imp = self.imp();
        let (Some(visible_box), Some(chevron), Some(drawer)) = (
            imp.visible_box.borrow().clone(),
            imp.chevron.borrow().clone(),
            imp.drawer.borrow().clone(),
        ) else {
            return;
        };

        let order: [gtk4::Widget; 3] = match imp.edge.get() {
            Edge::Start => [
                chevron.clone().upcast(),
                drawer.clone().upcast(),
                visible_box.clone().upcast(),
            ],
            Edge::End => [
                visible_box.clone().upcast(),
                drawer.clone().upcast(),
                chevron.clone().upcast(),
            ],
        };

        let mut previous: Option<gtk4::Widget> = None;
        for child in order {
            if child.prev_sibling() != previous {
                child.insert_after(self, previous.as_ref());
            }
            previous = Some(child);
        }

        let (transition, icon) = match (imp.vertical.get(), imp.edge.get()) {
            (true, Edge::Start) => (gtk4::RevealerTransitionType::SlideDown, "pan-up-symbolic"),
            (true, Edge::End) => (gtk4::RevealerTransitionType::SlideUp, "pan-down-symbolic"),
            (false, Edge::Start) => (
                gtk4::RevealerTransitionType::SlideRight,
                "pan-start-symbolic",
            ),
            (false, Edge::End) => (gtk4::RevealerTransitionType::SlideLeft, "pan-end-symbolic"),
        };
        drawer.set_transition_type(transition);
        if chevron.icon_name().as_deref() != Some(icon) {
            chevron.set_icon_name(icon);
        }
    }

    /// Wording for the overflow chevron, supplied by the caller: this crate takes values and does
    /// not author text a person reads.
    pub fn set_overflow_tooltip(&self, tooltip: Option<&str>) {
        let imp = self.imp();
        if imp.overflow_tooltip.borrow().as_deref() == tooltip {
            return;
        }
        imp.overflow_tooltip.replace(tooltip.map(str::to_owned));
        self.render();
    }

    pub fn set_items(&self, items: &[TrayChip]) {
        self.imp().chips.replace(items.to_vec());
        self.render();
    }

    /// The chip the last press landed on, for a popover to point at.
    pub fn anchor(&self) -> Option<gtk4::Widget> {
        self.imp().pressed.upgrade().map(Cast::upcast)
    }

    pub fn connect_activated<F: Fn(&Self, String, u32) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |strip: Self, key: String, button: u32| f(
                &strip, key, button
            )),
        )
    }

    pub fn connect_scrolled<F: Fn(&Self, String, f64, f64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "scrolled",
            false,
            glib::closure_local!(move |strip: Self, key: String, dx: f64, dy: f64| f(
                &strip, key, dx, dy
            )),
        )
    }

    fn render(&self) {
        let imp = self.imp();
        let chips = imp.chips.borrow().clone();
        let (shown, hidden) = chips.split_at(visible_count(chips.len(), imp.max_visible.get()));

        let Some(visible_box) = imp.visible_box.borrow().clone() else {
            return;
        };
        let Some(hidden_box) = imp.hidden_box.borrow().clone() else {
            return;
        };

        by_key(
            &visible_box,
            &mut imp.shown.borrow_mut(),
            shown,
            |chip| chip.key.clone(),
            |chip| self.chip(&chip.key),
            |indicator, chip| indicator.apply(&chip.spec),
        );
        by_key(
            &hidden_box,
            &mut imp.hidden.borrow_mut(),
            hidden,
            |chip| chip.key.clone(),
            |chip| self.chip(&chip.key),
            |indicator, chip| indicator.apply(&chip.spec),
        );

        if let Some(chevron) = imp.chevron.borrow().as_ref() {
            chevron.set_visible(!hidden.is_empty());
            crate::set_css_class(
                chevron,
                CHEVRON_MARKED,
                hidden.iter().any(|chip| chip.spec.badge.is_some()),
            );
            chevron.set_tooltip_text(imp.overflow_tooltip.borrow().as_deref());
            if hidden.is_empty() && chevron.is_active() {
                chevron.set_active(false);
            }
        }

        self.set_visible(!chips.is_empty());
        self.sync_accessible_label(shown, hidden.len());
    }

    fn chip(&self, key: &str) -> Indicator {
        let indicator = Indicator::new();
        let key = key.to_owned();

        let click = gtk4::GestureClick::new();
        click.set_button(0);
        click.connect_released(glib::clone!(
            #[weak(rename_to = strip)]
            self,
            #[weak]
            indicator,
            #[strong]
            key,
            move |gesture, _, _, _| {
                strip.imp().pressed.set(Some(&indicator));
                strip.emit_by_name::<()>("activated", &[&key, &gesture.current_button()]);
            }
        ));
        indicator.add_controller(click);

        let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::BOTH_AXES);
        scroll.connect_scroll(glib::clone!(
            #[weak(rename_to = strip)]
            self,
            #[strong]
            key,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, dx, dy| {
                strip.emit_by_name::<()>("scrolled", &[&key, &dx, &dy]);
                glib::Propagation::Stop
            }
        ));
        indicator.add_controller(scroll);

        indicator
    }

    fn sync_accessible_label(&self, shown: &[TrayChip], hidden: usize) {
        let mut parts: Vec<String> = shown
            .iter()
            .filter_map(|chip| {
                chip.spec
                    .label
                    .clone()
                    .or_else(|| chip.spec.tooltip.clone())
            })
            .collect();
        if hidden > 0
            && let Some(overflow) = self.imp().overflow_tooltip.borrow().as_deref()
        {
            parts.push(overflow.to_owned());
        }
        let name = parts.join(" ");
        if *self.imp().accessible_name.borrow() == name {
            return;
        }
        self.update_property(&[gtk4::accessible::Property::Label(&name)]);
        self.imp().accessible_name.replace(name);
    }
}

/// How many chips stay on the strip. `0` means no overflow at all, and a cap at or above the count
/// leaves nothing to hide — both must yield an empty tail, or the chevron appears over nothing.
pub(crate) fn visible_count(total: usize, max: u32) -> usize {
    match max {
        0 => total,
        max => (max as usize).min(total),
    }
}

#[cfg(test)]
mod tests {
    use super::visible_count;

    #[test]
    fn a_cap_of_zero_keeps_every_chip_on_the_strip() {
        assert_eq!(visible_count(5, 0), 5);
        assert_eq!(visible_count(0, 0), 0);
    }

    #[test]
    fn a_cap_at_or_above_the_count_hides_nothing() {
        assert_eq!(visible_count(3, 3), 3);
        assert_eq!(visible_count(3, 9), 3);
    }

    #[test]
    fn a_cap_below_the_count_leaves_the_rest_for_the_drawer() {
        assert_eq!(visible_count(5, 2), 2);
        assert_eq!(visible_count(5, 1), 1);
    }
}
