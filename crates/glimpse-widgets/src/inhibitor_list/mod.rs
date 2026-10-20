mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{ActionRow, ExpanderRow, Fact, Row, RowTone, reconcile};

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
    fn build_item(&self, id: u64) -> ExpanderRow {
        let release = ActionRow::new();
        release
            .upcast_ref::<Row>()
            .set_title(Some(gettext("Release").as_str()));
        release.upcast_ref::<Row>().set_tone(RowTone::Destructive);
        release.connect_activated(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("release-requested", &[&id])
        ));

        let item = ExpanderRow::new();
        let details = item.details();
        details.append(&gtk4::Box::new(gtk4::Orientation::Vertical, 0));
        details.append(&release);
        item
    }
}

/// The row is one line and the source is its icon, so the card holds the reason, what the hold
/// prevents and Release.
fn dress(item: &ExpanderRow, entry: &InhibitorEntry) {
    item.set_title(crate::none_if_empty(&entry.label));
    item.set_icon_name(Some(entry.source.icon_name()));

    let details = item.details();
    if let Some(facts) = details.first_child().and_downcast::<gtk4::Box>() {
        let reason =
            (!entry.status.is_empty()).then(|| Fact::new(gettext("Reason"), entry.status.clone()));
        let prevents = Fact::new(gettext("Prevents"), targets(&entry.targets));
        crate::set_facts(
            &facts,
            &reason.into_iter().chain([prevents]).collect::<Vec<_>>(),
        );
    }
    if let Some(release) = details.last_child()
        && release.get_visible() != entry.can_release
    {
        release.set_visible(entry.can_release);
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
