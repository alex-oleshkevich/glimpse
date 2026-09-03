mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::reconcile::by_key;
use crate::{SplitRow, Workspace};

const URGENT: &str = "workspace-row--urgent";

glib::wrapper! {
    pub struct WorkspaceSection(ObjectSubclass<imp::WorkspaceSection>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for WorkspaceSection {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceSection {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_output(&self, output: &str) {
        self.imp().section.set_title(Some(output));
    }

    pub fn set_workspaces(&self, workspaces: &[Workspace]) {
        let imp = self.imp();
        let mut held = imp.held.borrow_mut();
        by_key(
            &*imp.rows,
            &mut held,
            workspaces,
            |workspace| workspace.id,
            |workspace| self.row_for(workspace.id),
            apply,
        );
    }

    pub fn connect_activated<F: Fn(&Self, u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |section: Self, id: u64| f(&section, id)),
        )
    }

    pub fn connect_details<F: Fn(&Self, u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "details",
            false,
            glib::closure_local!(move |section: Self, id: u64| f(&section, id)),
        )
    }

    fn row_for(&self, id: u64) -> SplitRow {
        let split = SplitRow::new();
        split.connect_activated(glib::clone!(
            #[weak(rename_to = section)]
            self,
            move |_| section.emit_by_name::<()>("activated", &[&id])
        ));
        split.connect_details(glib::clone!(
            #[weak(rename_to = section)]
            self,
            move |_| section.emit_by_name::<()>("details", &[&id])
        ));
        split
    }
}

fn apply(split: &SplitRow, workspace: &Workspace) {
    let row = split.row();
    row.set_title(Some(workspace.label.as_str()));
    row.set_subtitle((!workspace.detail.is_empty()).then_some(workspace.detail.as_str()));
    row.set_selectable(true);
    row.set_selected(workspace.focused);
    crate::set_css_class(split, URGENT, workspace.urgent);
}
