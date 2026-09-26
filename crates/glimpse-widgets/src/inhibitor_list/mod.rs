mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Expandable, Fact, FactList, Row, reconcile};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum InhibitorSource {
    #[default]
    ScreenSaver,
    Portal,
    Login1,
}

impl InhibitorSource {
    fn icon_name(self) -> &'static str {
        match self {
            Self::ScreenSaver => "preferences-desktop-screensaver-symbolic",
            Self::Portal => "package-x-generic-symbolic",
            Self::Login1 => "system-run-symbolic",
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
        reconcile::by_key(
            self,
            &mut imp.items.borrow_mut(),
            &entries,
            |entry| entry.id,
            |entry| self.build_item(entry.id),
            dress,
        );
    }

    /// An app's whole row opens a card of what it prevents and, when it may be ended, a
    /// destructive Release: it ends another app's hold, which is not what "Cancel" reads as.
    fn build_item(&self, id: u64) -> Expandable {
        let row = Row::new();
        let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
        chevron.set_accessible_role(gtk4::AccessibleRole::Presentation);
        chevron.add_css_class("drawer-chevron");
        row.set_trail(&chevron);

        let release = Row::new();
        release.set_title(Some(gettext("Release").as_str()));
        release.set_activatable(true);
        release.add_css_class(crate::DESTRUCTIVE);
        release.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("release-requested", &[&id])
        ));

        let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        card.append(&FactList::new());
        card.append(&release);

        let holder = Expandable::new(&row);
        holder.set_details(Some(&card));
        holder
    }
}

/// The card repeats nothing the row says: the reason is the subtitle and the source is the icon,
/// so it holds what the hold prevents and Release. One that prevents only idle and cannot be
/// released has nothing left to show, and its row is plain text.
fn dress(holder: &Expandable, entry: &InhibitorEntry) {
    let Some(row) = holder.head::<Row>() else {
        return;
    };
    row.set_title(crate::none_if_empty(&entry.label));
    row.set_subtitle(crate::none_if_empty(&entry.status));
    row.set_lead_icon(Some(entry.source.icon_name()));

    let Some(card) = holder.details::<gtk4::Box>() else {
        return;
    };
    if let Some(facts) = card.first_child().and_downcast::<FactList>() {
        facts.set_facts(&[Fact::new(gettext("Prevents"), targets(&entry.targets))]);
    }
    if let Some(release) = card.last_child()
        && release.get_visible() != entry.can_release
    {
        release.set_visible(entry.can_release);
    }

    let only_idle = entry.targets
        == InhibitorTargets {
            idle: true,
            ..InhibitorTargets::default()
        };
    let carded = entry.can_release || !only_idle;
    row.set_activatable(carded);
    if let Some(chevron) = row.trail()
        && chevron.get_visible() != carded
    {
        chevron.set_visible(carded);
    }
    if !carded {
        holder.set_expanded(false);
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
