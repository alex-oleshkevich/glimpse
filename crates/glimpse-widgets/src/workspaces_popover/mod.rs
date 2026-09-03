mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::reconcile::by_key;
use crate::{Row, Workspace};

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
        imp.hero.set_subtitle(summary(workspaces).as_deref());

        if let Some(opened) = imp.opened.get() {
            self.reveal(opened);
        }
    }

    fn reveal(&self, id: u64) {
        let imp = self.imp();
        let workspaces = imp.workspaces.borrow();
        let Some(workspace) = workspaces.iter().find(|workspace| workspace.id == id) else {
            drop(workspaces);
            return self.close_drawer();
        };

        imp.detail.set_empty(workspace.windows.is_empty());

        let mut rows = imp.rows.borrow_mut();
        by_key(
            &*imp.page,
            &mut rows,
            &workspace.windows,
            |window| window.id,
            |window| self.row_for(window.id),
            |row, window| {
                row.set_title(Some(window.title.as_str()));
                row.set_subtitle((!window.app_id.is_empty()).then_some(window.app_id.as_str()));
                row.set_selectable(true);
                row.set_selected(window.focused);
            },
        );

        drop(rows);
        drop(workspaces);
        imp.opened.set(Some(id));
        imp.drawer.set_reveal_child(true);
    }

    fn row_for(&self, id: u64) -> Row {
        let row = Row::new();
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("window-activated", &[&id])
        ));
        row
    }

    fn close_drawer(&self) {
        let imp = self.imp();
        imp.opened.set(None);
        imp.drawer.set_reveal_child(false);
    }

    pub fn toggle_detail(&self, id: u64) {
        match self.imp().opened.get() {
            Some(open) if open == id => self.close_drawer(),
            _ => self.reveal(id),
        }
    }

    pub fn connect_activated<F: Fn(u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.imp().list.connect_activated(move |_, id| f(id))
    }

    pub fn connect_details<F: Fn(u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.imp().list.connect_details(move |_, id| f(id))
    }

    pub fn connect_window_activated<F: Fn(u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "window-activated",
            false,
            glib::closure_local!(move |_: Self, id: u64| f(id)),
        )
    }
}

fn summary(workspaces: &[Workspace]) -> Option<String> {
    if workspaces.is_empty() {
        return None;
    }

    let mut outputs: Vec<&str> = workspaces
        .iter()
        .map(|workspace| workspace.output.as_str())
        .collect();
    outputs.sort_unstable();
    outputs.dedup();

    Some(match outputs.len() {
        1 => format!("{} on {}", plural(workspaces.len()), outputs[0]),
        displays => format!("{} across {displays} displays", plural(workspaces.len())),
    })
}

fn plural(count: usize) -> String {
    match count {
        1 => "1 workspace".to_owned(),
        many => format!("{many} workspaces"),
    }
}
