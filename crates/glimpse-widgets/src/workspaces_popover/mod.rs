mod imp;

use gettextrs::{gettext, ngettext};
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::reconcile::by_key;
use crate::{Row, SplitRow, Workspace, WorkspaceWindow, drawer};

const DETAIL: &str = "detail-card";

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
        let windows = workspace.windows.clone();
        drop(workspaces);

        imp.opened.set(Some(id));
        self.fill(id, &windows);
        self.recede();
    }

    /// The panel is built the first time its workspace is opened: a list of a dozen workspaces
    /// would otherwise carry a dozen row boxes nothing has asked to see.
    fn fill(&self, id: u64, windows: &[WorkspaceWindow]) {
        let Some((_, holder)) = self
            .imp()
            .list
            .holders()
            .into_iter()
            .find(|(held, _)| *held == id)
        else {
            return;
        };
        let Some(panel) = drawer::panel(&holder) else {
            return;
        };
        let rows = match panel.child().and_downcast::<gtk4::Box>() {
            Some(rows) => rows,
            None => {
                let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                rows.add_css_class(DETAIL);
                panel.set_child(Some(&rows));
                rows
            }
        };
        by_key(
            &rows,
            &mut self.imp().rows.borrow_mut(),
            windows,
            |window| window.id,
            |window| self.row_for(window.id),
            |row, window| {
                row.set_title(Some(window.title.as_str()));
                row.set_subtitle((!window.app_id.is_empty()).then_some(window.app_id.as_str()));
                row.set_selectable(true);
                row.set_selected(window.focused);
            },
        );
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

    /// Every row but the open one recedes, and every panel but the open one closes — derived from
    /// `opened` rather than remembered per row, so two holders can never disagree about which one
    /// the card belongs to.
    fn recede(&self) {
        let imp = self.imp();
        let opened = imp.opened.get();
        for (id, holder) in imp.list.holders() {
            let wanted = opened == Some(id);
            if let Some(split) = drawer::head::<SplitRow>(&holder) {
                crate::set_css_class(&split, drawer::OPEN, wanted);
                crate::set_css_class(&split, drawer::RECEDED, opened.is_some() && !wanted);
            }
            if let Some(panel) = drawer::panel(&holder) {
                drawer::set(&panel, wanted);
            }
        }
        crate::set_css_class(&*imp.hero, drawer::RECEDED, opened.is_some());
    }

    fn close_drawer(&self) {
        self.imp().opened.set(None);
        self.recede();
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
        1 => gettext("{workspaces} on {output}")
            .replace("{workspaces}", &plural(workspaces.len()))
            .replace("{output}", outputs[0]),
        displays => ngettext(
            "{workspaces} across {displays} display",
            "{workspaces} across {displays} displays",
            displays as u32,
        )
        .replace("{workspaces}", &plural(workspaces.len()))
        .replace("{displays}", &displays.to_string()),
    })
}

fn plural(count: usize) -> String {
    ngettext("{count} workspace", "{count} workspaces", count as u32)
        .replace("{count}", &count.to_string())
}
