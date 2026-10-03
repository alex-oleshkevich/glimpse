mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Expandable, Fact, FactList, LegacyRow, none_if_empty, reconcile};

pub use imp::Usage;

const CHEVRON: &str = "go-next-symbolic";

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
        #[cfg(test)]
        imp.renders.set(imp.renders.get() + 1);
        let usages = imp.usage_data.borrow();
        reconcile::by_key(
            &*imp.usage_rows,
            &mut imp.usage_held.borrow_mut(),
            &usages,
            |usage| usage.id.clone(),
            |_| self.build_usage(),
            |holder, usage| self.apply_usage(holder, usage),
        );
        imp.usages.set_visible(!usages.is_empty());
    }

    fn build_usage(&self) -> Expandable {
        let row = LegacyRow::new();
        let chevron = gtk4::Image::from_icon_name(CHEVRON);
        chevron.set_accessible_role(gtk4::AccessibleRole::Presentation);
        chevron.add_css_class("drawer-chevron");
        row.set_trail(&chevron);
        Expandable::new(&row)
    }

    fn apply_usage(&self, holder: &Expandable, usage: &Usage) {
        let carded = usage.stoppable || usage.detail.is_some();
        if let Some(row) = holder.head::<LegacyRow>() {
            row.set_title(none_if_empty(&usage.title));
            row.set_lead_icon(none_if_empty(&usage.icon));
            if row.activatable() != carded {
                row.set_activatable(carded);
            }
            if let Some(chevron) = row.trail()
                && chevron.get_visible() != carded
            {
                chevron.set_visible(carded);
            }
        }
        let card = match (carded, holder.details::<gtk4::Box>()) {
            (false, Some(_)) => return holder.set_details(None::<&gtk4::Widget>),
            (false, None) => return,
            (true, Some(card)) => card,
            (true, None) => {
                let card = self.card(&usage.id);
                holder.set_details(Some(&card));
                card
            }
        };
        if let Some(facts) = card.first_child().and_downcast::<FactList>() {
            let in_use = usage
                .detail
                .as_ref()
                .map(|detail| Fact::new(gettext("In use"), detail.clone()));
            facts.set_facts(&in_use.into_iter().collect::<Vec<_>>());
        }
        if let Some(stop) = card.last_child() {
            stop.set_visible(usage.stoppable);
        }
    }

    fn card(&self, id: &str) -> gtk4::Box {
        let stop = LegacyRow::new();
        stop.set_title(Some(gettext("Stop sharing").as_str()));
        stop.add_css_class(crate::DESTRUCTIVE);
        let key = id.to_owned();
        stop.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("stop-activated", &[&key])
        ));
        let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        card.append(&FactList::new());
        card.append(&stop);
        card
    }
}

#[cfg(test)]
mod tests;
