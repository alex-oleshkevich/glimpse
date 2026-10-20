mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{ActionRow, ExpanderRow, Fact, Row, RowTone, none_if_empty, reconcile};

pub use imp::Usage;

glib::wrapper! {
    pub struct PrivacyPopover(ObjectSubclass<imp::PrivacyPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for PrivacyPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl PrivacyPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_usages(&self, usages: &[Usage]) {
        let imp = self.imp();
        if imp.usage_data.borrow().as_slice() == usages {
            return;
        }
        imp.usage_data.replace(usages.to_vec());
        self.render_usages();
    }

    pub fn set_microphone_muted(&self, muted: Option<bool>) {
        let mute = &self.imp().mute;
        if mute.get_visible() != muted.is_some() {
            mute.set_visible(muted.is_some());
        }
        if let Some(muted) = muted
            && mute.active() != muted
        {
            mute.set_active(muted);
        }
    }

    pub fn connect_stop_activated<F: Fn(&Self, String) + 'static>(
        &self,
        handler: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "stop-activated",
            false,
            glib::closure_local!(move |popover: Self, id: String| handler(&popover, id)),
        )
    }

    pub fn connect_mute_toggled<F: Fn(&Self, bool) + 'static>(
        &self,
        handler: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "mute-toggled",
            false,
            glib::closure_local!(move |popover: Self, on: bool| handler(&popover, on)),
        )
    }

    fn render_usages(&self) {
        let imp = self.imp();
        let usages = imp.usage_data.borrow();
        reconcile::by_key(
            &*imp.usage_rows,
            &mut imp.usage_held.borrow_mut(),
            &usages,
            |usage| (usage.id.clone(), carded(usage)),
            |usage| self.build_usage(usage),
            apply_usage,
        );
        imp.usages.set_visible(!usages.is_empty());
    }

    fn build_usage(&self, usage: &Usage) -> gtk4::Widget {
        if !carded(usage) {
            return Row::new().upcast();
        }
        let stop = ActionRow::new();
        stop.upcast_ref::<Row>()
            .set_title(Some(gettext("Stop sharing").as_str()));
        stop.upcast_ref::<Row>().set_tone(RowTone::Destructive);
        let key = usage.id.clone();
        stop.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("stop-activated", &[&key])
        ));
        let item = ExpanderRow::new();
        let details = item.details();
        details.append(&gtk4::Box::new(gtk4::Orientation::Vertical, 0));
        details.append(&stop);
        item.upcast()
    }
}

fn carded(usage: &Usage) -> bool {
    usage.stoppable || usage.detail.is_some()
}

fn apply_usage(widget: &gtk4::Widget, usage: &Usage) {
    let title = none_if_empty(&usage.title);
    let icon = none_if_empty(&usage.icon);
    let Some(item) = widget.downcast_ref::<ExpanderRow>() else {
        if let Some(row) = widget.downcast_ref::<Row>() {
            row.set_title(title);
            row.set_icon_name(icon);
        }
        return;
    };
    item.set_title(title);
    item.set_icon_name(icon);
    let details = item.details();
    if let Some(facts) = details.first_child().and_downcast::<gtk4::Box>() {
        let in_use = usage
            .detail
            .as_ref()
            .map(|detail| Fact::new(gettext("In use"), detail.clone()));
        crate::set_facts(&facts, &in_use.into_iter().collect::<Vec<_>>());
    }
    if let Some(stop) = details.last_child()
        && stop.get_visible() != usage.stoppable
    {
        stop.set_visible(usage.stoppable);
    }
}

#[cfg(test)]
mod tests;
