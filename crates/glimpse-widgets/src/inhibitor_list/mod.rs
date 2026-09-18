mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Fact, FactList, Row, drawer};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum InhibitorSource {
    #[default]
    ScreenSaver,
    Portal,
    Login1,
    ManualHold,
}

impl InhibitorSource {
    fn icon_name(self) -> &'static str {
        match self {
            Self::ScreenSaver => "preferences-desktop-screensaver-symbolic",
            Self::Portal => "package-x-generic-symbolic",
            Self::Login1 => "system-run-symbolic",
            Self::ManualHold => "alarm-symbolic",
        }
    }

    fn label(self) -> String {
        match self {
            Self::ScreenSaver => gettext("Screen saver"),
            Self::Portal => gettext("Portal"),
            Self::Login1 => gettext("System"),
            Self::ManualHold => gettext("Manual hold"),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct InhibitorTargets {
    pub idle: bool,
    pub suspend: bool,
    pub shutdown: bool,
    pub lid_switch: bool,
    pub power_key: bool,
    pub suspend_key: bool,
    pub hibernate_key: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct InhibitorEntry {
    pub id: u64,
    pub source: InhibitorSource,
    pub label: String,
    pub status: String,
    pub targets: InhibitorTargets,
    pub can_release: bool,
}

glib::wrapper! {
    pub struct InhibitorList(ObjectSubclass<imp::InhibitorList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for InhibitorList {
    fn default() -> Self {
        Self::new()
    }
}

impl InhibitorList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_inhibitors(&self, entries: &[InhibitorEntry]) {
        let imp = self.imp();
        if imp.entries.borrow().as_slice() == entries {
            return;
        }
        imp.entries.replace(entries.to_vec());
        self.render();
        if imp
            .opened
            .get()
            .is_some_and(|id| !entries.iter().any(|entry| entry.id == id))
        {
            self.set_open(None);
        } else {
            self.reveal();
        }
    }

    pub fn is_open(&self) -> bool {
        self.imp().opened.get().is_some()
    }

    pub fn close_detail(&self) {
        self.set_open(None);
    }

    pub fn connect_detail_toggled<F: Fn(&Self, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "detail-toggled",
            false,
            glib::closure_local!(move |list: Self, open: bool| f(&list, open)),
        )
    }

    pub fn connect_release_requested<F: Fn(&Self, u64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "release-requested",
            false,
            glib::closure_local!(move |list: Self, id: u64| f(&list, id)),
        )
    }

    fn render(&self) {
        let imp = self.imp();
        let entries = imp.entries.borrow().clone();
        let mut items = imp.items.borrow_mut();

        for (index, entry) in entries.iter().enumerate() {
            if items.len() == index {
                let item = self.build_item(index);
                item.holder
                    .insert_after(self, items.last().map(|item| &item.holder));
                items.push(item);
            }
            let item = &items[index];
            item.row.set_title(crate::none_if_empty(&entry.label));
            item.row.set_subtitle(crate::none_if_empty(&entry.status));
            item.row.set_lead_icon(Some(entry.source.icon_name()));
            crate::set_text(&item.description, Some(&entry.status));
            item.facts.set_facts(&[
                Fact::new(gettext("Source"), entry.source.label()),
                Fact::new(gettext("Prevents"), targets(&entry.targets)),
            ]);
            item.cancel.set_visible(entry.can_release);
        }

        for item in items.split_off(entries.len()) {
            item.holder.unparent();
        }
    }

    fn build_item(&self, index: usize) -> imp::Item {
        let row = Row::new();
        let arrow = gtk4::Image::from_icon_name("go-next-symbolic");
        arrow.set_accessible_role(gtk4::AccessibleRole::Presentation);
        arrow.add_css_class("drawer-chevron");
        row.set_trail(&arrow);
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| {
                let id = list.imp().entries.borrow().get(index).map(|entry| entry.id);
                if let Some(id) = id {
                    list.toggle_detail(id);
                }
            }
        ));

        let description = gtk4::Label::new(None);
        description.set_wrap(true);
        description.set_xalign(0.0);
        description.add_css_class("detail-card__description");
        let facts = FactList::new();
        let cancel = Row::new();
        cancel.set_title(Some(gettext("Cancel")));
        cancel.set_activatable(true);
        cancel.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| {
                let target = list
                    .imp()
                    .entries
                    .borrow()
                    .get(index)
                    .filter(|entry| entry.can_release)
                    .map(|entry| entry.id);
                if let Some(id) = target {
                    list.emit_by_name::<()>("release-requested", &[&id]);
                }
            }
        ));

        let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        card.add_css_class("detail-card");
        card.append(&description);
        card.append(&facts);
        card.append(&cancel);

        let panel = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .build();
        panel.set_child(Some(&card));
        let holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        holder.append(&row);
        holder.append(&panel);

        imp::Item {
            holder,
            row,
            panel,
            description,
            facts,
            cancel,
        }
    }

    fn toggle_detail(&self, id: u64) {
        let open = (self.imp().opened.get() != Some(id)).then_some(id);
        self.set_open(open);
    }

    fn set_open(&self, open: Option<u64>) {
        let imp = self.imp();
        if imp.opened.replace(open) == open {
            return;
        }
        self.reveal();
        self.emit_by_name::<()>("detail-toggled", &[&open.is_some()]);
    }

    fn reveal(&self) {
        let imp = self.imp();
        let open = imp.opened.get();
        for (entry, item) in imp.entries.borrow().iter().zip(imp.items.borrow().iter()) {
            let selected = open == Some(entry.id);
            drawer::set(&item.panel, selected);
            crate::set_css_class(&item.row, drawer::OPEN, selected);
            crate::set_css_class(&item.row, drawer::RECEDED, open.is_some() && !selected);
        }
    }
}

fn targets(targets: &InhibitorTargets) -> String {
    [
        (targets.idle, gettext("Idle")),
        (targets.suspend, gettext("Suspend")),
        (targets.shutdown, gettext("Shutdown")),
        (targets.lid_switch, gettext("Lid")),
        (targets.power_key, gettext("Power key")),
        (targets.suspend_key, gettext("Suspend key")),
        (targets.hibernate_key, gettext("Hibernate key")),
    ]
    .into_iter()
    .filter_map(|(active, label)| active.then_some(label))
    .collect::<Vec<_>>()
    .join(" · ")
}
