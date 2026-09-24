mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Expandable, Row, none_if_empty, reconcile};

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
        let row = Row::new();
        let chevron = gtk4::Image::from_icon_name(CHEVRON);
        chevron.set_accessible_role(gtk4::AccessibleRole::Presentation);
        chevron.add_css_class("drawer-chevron");
        row.set_trail(&chevron);
        Expandable::new(&row)
    }

    fn apply_usage(&self, holder: &Expandable, usage: &Usage) {
        if let Some(row) = holder.head::<Row>() {
            row.set_title(none_if_empty(&usage.title));
            row.set_subtitle(usage.detail.as_deref());
            row.set_lead_icon(none_if_empty(&usage.icon));
            if row.activatable() != usage.stoppable {
                row.set_activatable(usage.stoppable);
            }
            if let Some(chevron) = row.trail()
                && chevron.get_visible() != usage.stoppable
            {
                chevron.set_visible(usage.stoppable);
            }
        }
        match (usage.stoppable, holder.details::<gtk4::Widget>().is_some()) {
            (true, false) => holder.set_details(Some(&self.stop_card(&usage.id))),
            (false, true) => holder.set_details(None::<&gtk4::Widget>),
            _ => {}
        }
    }

    fn stop_card(&self, id: &str) -> gtk4::Box {
        let stop = Row::new();
        stop.set_title(Some(gettext("Stop sharing").as_str()));
        stop.add_css_class(crate::DESTRUCTIVE);
        let key = id.to_owned();
        stop.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("stop-activated", &[&key])
        ));
        let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        card.append(&stop);
        card
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    fn usage(id: &str, title: &str) -> Usage {
        Usage {
            id: id.to_owned(),
            icon: "google-chrome".into(),
            title: title.to_owned(),
            detail: Some("Camera \u{b7} Microphone".into()),
            stoppable: false,
        }
    }

    fn rows(parent: &gtk4::Box) -> Vec<Expandable> {
        let mut rows = Vec::new();
        let mut child = parent.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(row) = widget.downcast::<Expandable>() {
                rows.push(row);
            }
        }
        rows
    }

    fn head(holder: &Expandable) -> Row {
        holder.head::<Row>().expect("a head row")
    }

    #[test]
    #[ignore = "needs a display"]
    fn privacy_popover_states() {
        if gtk4::init().is_err() {
            return;
        }
        crate::register_resources().expect("resources");

        let popover = PrivacyPopover::new();
        let imp = popover.imp();
        assert!(!imp.usages.get_visible(), "nothing in use shows no section");
        assert!(!imp.mute.get_visible());

        popover.set_usages(&[usage("app:Chrome", "Google Chrome")]);
        assert!(imp.usages.get_visible());
        let chrome = rows(&imp.usage_rows);
        assert_eq!(chrome.len(), 1);
        assert_eq!(head(&chrome[0]).title().as_deref(), Some("Google Chrome"));
        assert_eq!(
            head(&chrome[0]).subtitle().as_deref(),
            Some("Camera \u{b7} Microphone")
        );
        assert!(
            !head(&chrome[0]).activatable() && chrome[0].details::<gtk4::Widget>().is_none(),
            "a use with nothing to stop is a plain row with no card"
        );

        let renders_after_first = imp.renders.get();
        popover.set_usages(&[usage("app:Chrome", "Google Chrome")]);
        assert_eq!(
            imp.renders.get(),
            renders_after_first,
            "an unchanged usage list must never re-enter render_usages at all"
        );

        let stopped: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        popover.connect_stop_activated({
            let stopped = Rc::clone(&stopped);
            move |_, id| stopped.borrow_mut().push(id)
        });
        popover.set_usages(&[Usage {
            stoppable: true,
            ..usage("app:Chrome", "Google Chrome")
        }]);
        let sharing = rows(&imp.usage_rows);
        assert_eq!(sharing[0], chrome[0], "the same app keeps its row");
        head(&sharing[0]).emit_by_name::<()>("clicked", &[]);
        assert!(
            sharing[0].expanded(),
            "a share that can be stopped opens its card"
        );
        let stop = sharing[0]
            .details::<gtk4::Box>()
            .and_then(|card| card.first_child())
            .and_downcast::<Row>()
            .expect("a Stop sharing row");
        assert_eq!(stop.title().as_deref(), Some("Stop sharing"));
        assert!(stop.has_css_class("row--destructive"));
        stop.emit_by_name::<()>("clicked", &[]);
        assert_eq!(*stopped.borrow(), ["app:Chrome".to_owned()]);

        popover.set_usages(&[usage("app:Chrome", "Google Chrome")]);
        assert!(
            rows(&imp.usage_rows)[0].details::<gtk4::Widget>().is_none(),
            "a share that ended takes its card with it"
        );

        let toggles: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
        popover.connect_mute_toggled({
            let toggles = Rc::clone(&toggles);
            move |_, on| toggles.borrow_mut().push(on)
        });
        popover.set_microphone_muted(Some(true));
        assert!(imp.mute.get_visible() && imp.mute.active());
        assert!(
            toggles.borrow().is_empty(),
            "following the device's state is not a request to change it"
        );
        imp.mute.set_property("active", false);
        imp.mute.emit_by_name::<()>("clicked", &[]);
        assert_eq!(toggles.borrow().last(), Some(&true));
        popover.set_microphone_muted(None);
        assert!(!imp.mute.get_visible());

        popover.set_usages(&[]);
        assert!(!imp.usages.get_visible());
        assert!(imp.usage_rows.first_child().is_none());
    }
}
