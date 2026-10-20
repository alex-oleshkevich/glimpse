use glimpse_dbus::dbusmenu::{MenuNode, MenuToggle, ToggleState};
use gtk4::glib;
use gtk4::prelude::*;

/// The action group a built menu installs itself under. dbusmenu ids are integers, so an action
/// name is that id: unique within one item's menu, which is the only scope a group has.
pub const GROUP: &str = "tray";

/// A menu as `gio::Menu` needs it: a list of sections, each a list of items. dbusmenu expresses a
/// separator as an *item*, while `GMenu` expresses it as the boundary between sections, so the
/// transform is a split rather than a mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section(pub Vec<Item>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub icon: Option<String>,
    pub toggle: MenuToggle,
    pub state: ToggleState,
    pub submenu: Vec<Section>,
}

impl Item {
    /// `tray.<id>`, the name the action group answers to.
    pub fn action(&self) -> String {
        format!("{GROUP}.{}", self.id)
    }
}

/// Split one level of a decoded layout into sections. An item marked `visible: false` is not built
/// at all — leaving it disabled would still show the application something it asked to hide.
pub fn sections(node: &MenuNode) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    let mut current: Vec<Item> = Vec::new();

    for child in node.children.iter().filter(|child| child.visible) {
        if child.separator {
            if !current.is_empty() {
                sections.push(Section(std::mem::take(&mut current)));
            }
            continue;
        }
        current.push(Item {
            id: child.id,
            label: child.label.clone(),
            enabled: child.enabled,
            icon: child.icon_name.clone(),
            toggle: child.toggle,
            state: child.toggle_state,
            submenu: match child.submenu || !child.children.is_empty() {
                true => self::sections(child),
                false => Vec::new(),
            },
        });
    }
    if !current.is_empty() {
        sections.push(Section(current));
    }
    sections
}

/// A `gio::Menu` mirroring the sections. Submenus nest natively, which is the whole reason a
/// `PopoverMenu` was chosen over a hand-built drawer: dbusmenu nests arbitrarily deep and a
/// side-drawer of rows handles exactly one level.
pub fn model(sections: &[Section]) -> gio::Menu {
    build(sections, "")
}

fn build(sections: &[Section], level: &str) -> gio::Menu {
    let menu = gio::Menu::new();
    for (index, section) in sections.iter().enumerate() {
        let part = gio::Menu::new();
        for item in &section.0 {
            part.append_item(&entry(item, &group_name(level, index)));
        }
        menu.append_section(None, &part);
    }
    menu
}

/// A radio group's action name has to carry its whole path. Both the model and the actions restart
/// the section index at 0 inside every submenu, and one `SimpleActionGroup` holds them all, so a
/// bare `radio-0` in a submenu silently replaces the one at the top level — leaving the two groups
/// sharing a selection.
fn group_name(level: &str, section: usize) -> String {
    format!("{level}{section}")
}

fn entry(item: &Item, group: &str) -> gio::MenuItem {
    let built = gio::MenuItem::new(Some(&item.label.replace('_', "__")), None);
    if let Some(icon) = &item.icon {
        built.set_icon(&gio::ThemedIcon::new(icon));
    }
    if !item.submenu.is_empty() {
        built.set_submenu(Some(&build(
            &item.submenu,
            &format!("{group}-{}-", item.id),
        )));
        return built;
    }
    match item.toggle {
        // A string state plus a per-item target is what renders a radio group; dbusmenu does not
        // say which items form one, so a section is the group.
        MenuToggle::Radio => {
            built.set_action_and_target_value(
                Some(&radio_action(group)),
                Some(&item.id.to_string().to_variant()),
            );
        }
        // A boolean state renders a checkmark. `_old` faked this with a unicode prefix because it
        // never wired stateful actions; that was its defect, not a `PopoverMenu` limitation.
        MenuToggle::Checkmark | MenuToggle::None => {
            built.set_detailed_action(&item.action());
        }
    }
    built
}

fn radio_action(group: &str) -> String {
    format!("{GROUP}.radio-{group}")
}

/// Every action the model names, with the state the application reported. `fired` is handed the
/// dbusmenu id, which is what `Event` carries back.
pub fn actions(
    sections: &[Section],
    fired: impl Fn(i32) + Clone + 'static,
) -> gio::SimpleActionGroup {
    let group = gio::SimpleActionGroup::new();
    install(&group, sections, "", &fired);
    group
}

fn install(
    group: &gio::SimpleActionGroup,
    sections: &[Section],
    level: &str,
    fired: &(impl Fn(i32) + Clone + 'static),
) {
    for (index, section) in sections.iter().enumerate() {
        let mut selected: Option<String> = None;
        let mut disabled: Vec<String> = Vec::new();
        for item in &section.0 {
            if !item.submenu.is_empty() {
                let deeper = format!("{}-{}-", group_name(level, index), item.id);
                install(group, &item.submenu, &deeper, fired);
                continue;
            }
            match item.toggle {
                MenuToggle::Radio => {
                    if item.state == ToggleState::On {
                        selected = Some(item.id.to_string());
                    }
                    if !item.enabled {
                        disabled.push(item.id.to_string());
                    }
                }
                MenuToggle::Checkmark => {
                    let action = gio::SimpleAction::new_stateful(
                        &item.id.to_string(),
                        None,
                        &(item.state == ToggleState::On).to_variant(),
                    );
                    action.set_enabled(item.enabled);
                    let fired = fired.clone();
                    let id = item.id;
                    action.connect_activate(move |action, _| {
                        let now = !action
                            .state()
                            .and_then(|s| s.get::<bool>())
                            .unwrap_or(false);
                        action.set_state(&now.to_variant());
                        fired(id);
                    });
                    group.add_action(&action);
                }
                MenuToggle::None => {
                    let action = gio::SimpleAction::new(&item.id.to_string(), None);
                    action.set_enabled(item.enabled);
                    let fired = fired.clone();
                    let id = item.id;
                    action.connect_activate(move |_, _| fired(id));
                    group.add_action(&action);
                }
            }
        }
        if section
            .0
            .iter()
            .any(|item| item.toggle == MenuToggle::Radio)
        {
            let action = gio::SimpleAction::new_stateful(
                &format!("radio-{}", group_name(level, index)),
                Some(glib::VariantTy::STRING),
                &selected.unwrap_or_default().to_variant(),
            );
            let fired = fired.clone();
            action.connect_activate(move |action, target| {
                let Some(target) = target else { return };
                // A disabled option in an otherwise live group: one action serves the whole group,
                // so the refusal has to be per target rather than per action.
                if target
                    .get::<String>()
                    .is_some_and(|id| disabled.iter().any(|off| off == &id))
                {
                    return;
                }
                action.set_state(target);
                if let Some(id) = target.get::<String>().and_then(|id| id.parse().ok()) {
                    fired(id);
                }
            });
            group.add_action(&action);
        }
    }
}

#[cfg(test)]
mod tests;
