mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::reconcile::by_key;
use crate::{ActionRow, ExpanderRow, Row, RowTone, Workspace};

const FOCUSED_ICON: &str = "object-select-symbolic";

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

    /// Every workspace row, `(id, row)`, for a caller that must reach across every section's rows
    /// at once — reveal and recede are a property of the whole popover, not of one output's
    /// section.
    pub(crate) fn holders(&self) -> Vec<(u64, ExpanderRow)> {
        self.imp().held.borrow().clone()
    }

    fn row_for(&self, id: u64) -> ExpanderRow {
        let row = ExpanderRow::new();
        let focus = ActionRow::new();
        focus
            .upcast_ref::<Row>()
            .set_title(Some(gettext("Focus workspace").as_str()));
        focus.connect_activated(glib::clone!(
            #[weak(rename_to = section)]
            self,
            move |_| section.emit_by_name::<()>("activated", &[&id])
        ));
        row.details().append(&focus);
        row.connect_toggled(glib::clone!(
            #[weak(rename_to = section)]
            self,
            move |_, open| {
                if open {
                    section.emit_by_name::<()>("details", &[&id]);
                }
            }
        ));
        row
    }
}

pub(crate) fn mark_focused(row: &Row, focused: bool) {
    let shown = row.imp().end().first_child().is_some();
    if shown == focused {
        return;
    }
    match focused {
        true => row.set_end(&gtk4::Image::from_icon_name(FOCUSED_ICON)),
        false => row.clear_end(),
    }
}

fn apply(row: &ExpanderRow, workspace: &Workspace) {
    row.set_title(Some(workspace.label.as_str()));
    row.set_value((!workspace.detail.is_empty()).then_some(workspace.detail.as_str()));
    row.set_tone(match workspace.urgent {
        true => RowTone::Warning,
        false => RowTone::Normal,
    });
    mark_focused(row.imp().head.upcast_ref(), workspace.focused);
}
