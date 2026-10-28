mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::WorkspaceSection;
use crate::reconcile::by_key;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub id: u64,
    pub label: String,
    pub output: String,
    pub display: String,
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

fn grouped(workspaces: &[Workspace]) -> Vec<(String, &[Workspace])> {
    let mut out: Vec<(String, &[Workspace])> = Vec::new();
    let mut start = 0;
    for index in 1..=workspaces.len() {
        if index == workspaces.len() || workspaces[index].output != workspaces[start].output {
            out.push((workspaces[start].output.clone(), &workspaces[start..index]));
            start = index;
        }
    }
    out
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
        if imp.workspaces.borrow().as_slice() == workspaces {
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

    /// Every workspace row's holder across every output's section, for the popover to reveal and
    /// recede as one surface rather than one section at a time.
    pub(crate) fn holders(&self) -> Vec<(u64, crate::ExpanderRow)> {
        self.imp()
            .sections
            .borrow()
            .iter()
            .flat_map(|(_, section)| section.holders())
            .collect()
    }

    fn render(&self) {
        let imp = self.imp();
        let workspaces = imp.workspaces.borrow();
        let groups = grouped(&workspaces);

        let mut sections = imp.sections.borrow_mut();
        by_key(
            self,
            &mut sections,
            &groups,
            |(output, _)| output.clone(),
            |_| self.section(),
            |section, (output, group)| {
                section.set_output(
                    group
                        .first()
                        .map(|workspace| workspace.display.as_str())
                        .filter(|display| !display.is_empty())
                        .unwrap_or(output),
                );
                section.set_workspaces(group);
            },
        );

        let any = !workspaces.is_empty();
        drop(sections);
        drop(workspaces);
        self.set_visible(any);
    }

    fn section(&self) -> WorkspaceSection {
        let section = WorkspaceSection::new();
        section.connect_activated(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_, id| list.emit_by_name::<()>("activated", &[&id])
        ));
        section.connect_details(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_, id| list.emit_by_name::<()>("details", &[&id])
        ));
        section
    }
}

#[cfg(test)]
mod tests;
