mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::reconcile::by_key;
use crate::workspace_section::mark_focused;
use crate::{ActionRow, Row, Workspace};

glib::wrapper! {
    pub struct WorkspacesPopover(ObjectSubclass<imp::WorkspacesPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for WorkspacesPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspacesPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_workspaces(&self, workspaces: &[Workspace]) {
        let imp = self.imp();
        if imp.workspaces.borrow().as_slice() == workspaces {
            return;
        }
        imp.workspaces.replace(workspaces.to_vec());
        imp.list.set_workspaces(workspaces);

        let open = imp
            .list
            .holders()
            .into_iter()
            .find(|(_, holder)| holder.open());
        if let Some((id, _)) = open {
            self.fill(id);
        }
    }

    pub(crate) fn fill(&self, id: u64) {
        let imp = self.imp();
        let Some(windows) = imp
            .workspaces
            .borrow()
            .iter()
            .find(|workspace| workspace.id == id)
            .map(|workspace| workspace.windows.clone())
        else {
            return;
        };
        let Some((_, holder)) = imp.list.holders().into_iter().find(|(held, _)| *held == id) else {
            return;
        };
        let details = holder.details();
        let rows = details
            .last_child()
            .and_downcast::<gtk4::Box>()
            .unwrap_or_else(|| {
                let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                details.append(&rows);
                rows
            });
        by_key(
            &rows,
            &mut imp.rows.borrow_mut(),
            &windows,
            |window| window.id,
            |window| self.row_for(window.id),
            |row, window| {
                let row = row.upcast_ref::<Row>();
                row.set_title(Some(window.title.as_str()));
                row.set_subtitle((!window.app_id.is_empty()).then_some(window.app_id.as_str()));
                mark_focused(row, window.focused);
            },
        );
        let placeholder = rows
            .last_child()
            .filter(|row| !row.is::<ActionRow>())
            .and_downcast::<Row>()
            .unwrap_or_else(|| {
                let row = Row::new();
                row.set_title(Some(gettext("No windows").as_str()));
                rows.append(&row);
                row
            });
        placeholder.set_visible(windows.is_empty());
    }

    fn row_for(&self, id: u64) -> ActionRow {
        let row = ActionRow::new();
        row.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("window-activated", &[&id])
        ));
        row
    }

    pub fn connect_activated<F: Fn(u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.imp().list.connect_activated(move |_, id| f(id))
    }

    pub fn connect_window_activated<F: Fn(u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "window-activated",
            false,
            glib::closure_local!(move |_: Self, id: u64| f(id)),
        )
    }
}
