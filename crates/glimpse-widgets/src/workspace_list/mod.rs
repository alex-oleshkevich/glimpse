mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Section, SplitRow};

const URGENT: &str = "workspace-row--urgent";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub id: u64,
    pub label: String,
    pub detail: String,
    pub output: String,
    pub focused: bool,
    pub urgent: bool,
    pub windows: Vec<Window>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Window {
    pub id: u64,
    pub title: String,
    pub app_id: String,
    pub focused: bool,
    pub urgent: bool,
}

fn same_rows(previous: &[Workspace], next: &[Workspace]) -> bool {
    previous.len() == next.len()
        && previous.iter().zip(next).all(|(before, after)| {
            before.id == after.id
                && before.label == after.label
                && before.detail == after.detail
                && before.output == after.output
                && before.focused == after.focused
                && before.urgent == after.urgent
        })
}

glib::wrapper! {
    pub struct WorkspaceList(ObjectSubclass<imp::WorkspaceList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for WorkspaceList {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_workspaces(&self, workspaces: &[Workspace]) {
        let imp = self.imp();
        if same_rows(imp.workspaces.borrow().as_slice(), workspaces) {
            return;
        }
        imp.workspaces.replace(workspaces.to_vec());
        self.render();
    }

    pub fn connect_activated<F: Fn(&Self, u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |list: Self, id: u64| f(&list, id)),
        )
    }

    pub fn connect_details<F: Fn(&Self, u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "details",
            false,
            glib::closure_local!(move |list: Self, id: u64| f(&list, id)),
        )
    }

    fn render(&self) {
        let imp = self.imp();
        for section in imp.sections.borrow_mut().drain(..) {
            section.unparent();
        }

        let workspaces = imp.workspaces.borrow();
        let mut sections = imp.sections.borrow_mut();
        let mut column: Option<gtk4::Box> = None;

        for workspace in workspaces.iter() {
            let opened = sections
                .last()
                .and_then(|section| section.title())
                .is_some_and(|title| title == workspace.output);

            if !opened {
                let section = Section::new();
                section.set_title(Some(workspace.output.as_str()));
                let fresh = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                section.set_content(Some(&fresh));
                section.insert_after(self, sections.last());
                sections.push(section);
                column = Some(fresh);
            }

            if let Some(column) = column.as_ref() {
                column.append(&self.build_row(workspace));
            }
        }

        let any = !workspaces.is_empty();
        drop(sections);
        drop(workspaces);
        self.set_visible(any);
    }

    fn build_row(&self, workspace: &Workspace) -> SplitRow {
        let split = SplitRow::new();
        let row = split.row();
        row.set_title(Some(workspace.label.as_str()));
        row.set_subtitle((!workspace.detail.is_empty()).then_some(workspace.detail.as_str()));
        row.set_selectable(true);
        row.set_selected(workspace.focused);
        crate::set_css_class(&split, URGENT, workspace.urgent);

        let id = workspace.id;
        split.connect_activated(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("activated", &[&id])
        ));
        split.connect_details(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("details", &[&id])
        ));
        split
    }
}

#[cfg(test)]
mod tests {
    use super::{Window, Workspace, same_rows};

    fn workspace(windows: Vec<Window>) -> Workspace {
        Workspace {
            id: 1,
            label: "chats".to_owned(),
            detail: "2 windows".to_owned(),
            output: "DP-2".to_owned(),
            focused: false,
            urgent: false,
            windows,
        }
    }

    fn window(title: &str) -> Window {
        Window {
            id: 9,
            title: title.to_owned(),
            app_id: "ghostty".to_owned(),
            focused: false,
            urgent: false,
        }
    }

    #[test]
    fn what_a_window_is_doing_is_not_a_change_to_the_list() {
        assert!(
            same_rows(
                &[workspace(vec![window("a terminal")])],
                &[workspace(vec![window("a browser")])],
            ),
            "the list shows workspaces, and a title changes on every keystroke: rebuilding it \
             there destroys the row the pointer is resting on"
        );
    }

    #[test]
    fn everything_a_row_shows_is_a_change() {
        let before = [workspace(Vec::new())];

        let mut renamed = workspace(Vec::new());
        renamed.label = "work".to_owned();
        assert!(!same_rows(&before, &[renamed]));

        let mut recounted = workspace(Vec::new());
        recounted.detail = "empty".to_owned();
        assert!(!same_rows(&before, &[recounted]));

        let mut moved = workspace(Vec::new());
        moved.output = "eDP-1".to_owned();
        assert!(!same_rows(&before, &[moved]));

        let mut focused = workspace(Vec::new());
        focused.focused = true;
        assert!(!same_rows(&before, &[focused]));

        let mut urgent = workspace(Vec::new());
        urgent.urgent = true;
        assert!(!same_rows(&before, &[urgent]));

        let mut renumbered = workspace(Vec::new());
        renumbered.id = 2;
        assert!(!same_rows(&before, &[renumbered]));

        assert!(
            !same_rows(&before, &[]),
            "a workspace closing is the change the list exists to show"
        );
    }
}
