use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::PathBuf,
    rc::Rc,
};

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    services::storage::{Command, State, StorageDevice},
    utils::popover_scroll,
    widgets::{
        animated_popover::AnimatedPopover, hero::Hero, popover_shell::PopoverShell,
        segmented_tile::SegmentedTile, tile::Tile,
    },
};

pub struct Popover {
    popover: AnimatedPopover,
    rows: HashMap<String, DeviceRow>,
    list: gtk::Box,
    hero_subtitle: String,
}

#[derive(Debug)]
pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateState(State),
    DeviceCommand(RowCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopoverOutput {
    Command(Command),
    OpenPath(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RowCommand {
    Storage(Command),
    OpenPath(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DeviceAction<Command> {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) destructive: bool,
    pub(super) enabled: bool,
    pub(super) visible: bool,
    pub(super) command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeviceRowModel {
    id: String,
    icon: String,
    label: String,
    status: DeviceStatus,
    busy: bool,
    tooltip: String,
    active: bool,
    command: Option<RowCommand>,
    actions: Vec<DeviceAction<RowCommand>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeviceStatus {
    text: Option<String>,
    busy: bool,
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = AnimatedPopover {
            add_css_class: "popover-size-medium",

            PopoverShell {

                Hero {
                    set_icon: Some("drive-removable-media-symbolic"),
                    set_title: "Removable devices",
                    #[watch]
                    set_subtitle: &model.hero_subtitle,
                },

                #[name = "scroller"]
                gtk::ScrolledWindow {
                    set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                    set_vexpand: false,
                    set_propagate_natural_height: true,

                    #[local_ref]
                    list -> gtk::Box {},
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let list = gtk::Box::new(gtk::Orientation::Vertical, 2);

        let mut model = Popover {
            popover: AnimatedPopover::new(),
            rows: HashMap::new(),
            list: list.clone(),
            hero_subtitle: hero_subtitle(&State::default()),
        };

        let widgets = view_output!();
        model.popover = widgets.root.clone();
        widgets.root.set_parent(&init.parent);
        popover_scroll::install_half_monitor_limit(
            widgets.root.upcast_ref(),
            &widgets.scroller,
            &init.parent,
        );
        model.list.set_visible(false);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => {
                self.popover.toggle();
            }
            PopoverInput::UpdateState(state) => {
                let rows = build_device_rows(&state);
                self.hero_subtitle = hero_subtitle(&state);
                self.list.set_visible(!rows.is_empty());
                sync_device_rows(&mut self.rows, &self.list, rows, &sender);
            }
            PopoverInput::DeviceCommand(command) => {
                let _ = sender.output(popover_output_for_row_command(command));
            }
        }
    }
}

struct DeviceRow {
    root: SegmentedTile,
    icon: gtk::Image,
    status_box: gtk::Box,
    status_spinner: gtk::Spinner,
    status_label: gtk::Label,
    actions: gtk::Box,
    command: Rc<RefCell<Option<RowCommand>>>,
}

impl DeviceRow {
    fn new(model: &DeviceRowModel, sender: &ComponentSender<Popover>) -> Self {
        let root = SegmentedTile::new();
        root.add_css_class("removable-device-row");
        root.set_secondary(None);

        let icon = gtk::Image::from_icon_name(&model.icon);
        icon.add_css_class("removable-device-row__icon");
        icon.set_pixel_size(16);
        icon.set_valign(gtk::Align::Center);
        root.set_left(Some(icon.clone()));

        let status_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        status_box.set_valign(gtk::Align::Center);

        let status_spinner = gtk::Spinner::new();
        status_spinner.add_css_class("removable-device-row__spinner");
        status_spinner.set_visible(false);
        status_box.append(&status_spinner);

        let status_label = gtk::Label::new(None);
        status_label.add_css_class("dim-label");
        status_label.add_css_class("caption");
        status_label.set_visible(false);
        status_box.append(&status_label);

        let actions = gtk::Box::new(gtk::Orientation::Vertical, 2);
        actions.add_css_class("removable-device-row__actions");

        let command = Rc::new(RefCell::new(model.command.clone()));
        root.connect_activated({
            let sender = sender.clone();
            let command = command.clone();
            move |_| {
                if let Some(command) = command.borrow().clone() {
                    sender.input(PopoverInput::DeviceCommand(command));
                }
            }
        });

        let row = Self {
            root,
            icon,
            status_box,
            status_spinner,
            status_label,
            actions,
            command,
        };
        row.update(model, sender);
        row
    }

    fn update(&self, model: &DeviceRowModel, sender: &ComponentSender<Popover>) {
        self.root.set_primary(&model.label);
        self.root.set_tooltip_text(Some(&model.tooltip));
        self.root
            .set_activatable(model.command.is_some() && !model.busy);
        self.root.set_sensitive(true);
        self.command.replace(model.command.clone());
        self.icon.set_icon_name(Some(&model.icon));

        if model.active {
            self.root.add_css_class("is-active");
            self.root.add_css_class("is-selected");
        } else {
            self.root.remove_css_class("is-active");
            self.root.remove_css_class("is-selected");
        }

        self.sync_status(&model.status);
        self.sync_actions(&model.actions, sender);
    }

    fn sync_status(&self, status: &DeviceStatus) {
        self.status_spinner.set_visible(status.busy);
        self.status_spinner.set_spinning(status.busy);

        match &status.text {
            Some(text) if !status.busy => {
                self.status_label.set_label(text);
                self.status_label.set_visible(true);
            }
            _ => {
                self.status_label.set_visible(false);
            }
        }

        if status.busy || status.text.is_some() {
            self.root.set_right(Some(self.status_box.clone()));
        } else {
            self.root.set_right(None::<gtk::Widget>);
        }
    }

    fn sync_actions(
        &self,
        actions: &[DeviceAction<RowCommand>],
        sender: &ComponentSender<Popover>,
    ) {
        while let Some(child) = self.actions.first_child() {
            self.actions.remove(&child);
        }

        let visible_actions: Vec<_> = actions.iter().filter(|action| action.visible).collect();
        for action in &visible_actions {
            let tile = Tile::new();
            tile.add_css_class("removable-device-action");
            tile.set_primary(&action.label);
            tile.set_secondary(None);
            tile.set_sensitive(action.enabled);
            if action.destructive {
                tile.add_css_class("destructive-action");
            }
            tile.connect_activated({
                let sender = sender.clone();
                let command = action.command.clone();
                move |_| sender.input(PopoverInput::DeviceCommand(command.clone()))
            });
            self.actions.append(&tile);
        }

        if visible_actions.is_empty() {
            self.root.set_child(None::<gtk::Widget>);
        } else {
            self.root.set_child(Some(self.actions.clone()));
        }
    }

    fn widget(&self) -> &gtk::Widget {
        self.root.upcast_ref()
    }
}

fn build_device_rows(state: &State) -> Vec<DeviceRowModel> {
    state
        .devices
        .iter()
        .map(|device| DeviceRowModel {
            id: device.id.clone(),
            icon: device.icon.clone(),
            label: device.name.clone(),
            status: device_status(device),
            busy: device.busy,
            tooltip: device_tooltip(device),
            active: device.mounted_at.is_some(),
            command: primary_device_command(device),
            actions: device_actions(device),
        })
        .collect()
}

fn sync_device_rows(
    rows: &mut HashMap<String, DeviceRow>,
    container: &gtk::Box,
    models: Vec<DeviceRowModel>,
    sender: &ComponentSender<Popover>,
) {
    let mut seen = HashSet::new();
    let mut previous: Option<gtk::Widget> = None;

    for model in models {
        seen.insert(model.id.clone());
        let row = rows
            .entry(model.id.clone())
            .or_insert_with(|| DeviceRow::new(&model, sender));
        row.update(&model, sender);
        place_row(row.widget(), container, previous.as_ref());
        previous = Some(row.widget().clone());
    }

    rows.retain(|id, row| {
        let keep = seen.contains(id);
        if !keep {
            remove_row(row.widget());
        }
        keep
    });
}

fn place_row(row_widget: &gtk::Widget, container: &gtk::Box, previous: Option<&gtk::Widget>) {
    let target = container.clone().upcast::<gtk::Widget>();
    let already_in_container = row_widget.parent().is_some_and(|parent| parent == target);

    if !already_in_container {
        remove_row(row_widget);
        container.append(row_widget);
    }
    container.reorder_child_after(row_widget, previous);
}

fn remove_row(row_widget: &gtk::Widget) {
    if let Some(parent) = row_widget.parent()
        && let Ok(parent) = parent.downcast::<gtk::Box>()
    {
        parent.remove(row_widget);
    }
}

fn hero_subtitle(state: &State) -> String {
    match state.devices.len() {
        0 => "No devices connected".into(),
        1 => "1 device".into(),
        count => format!("{count} devices"),
    }
}

fn primary_device_command(device: &StorageDevice) -> Option<RowCommand> {
    if device.busy {
        return None;
    }

    if let Some(path) = &device.mounted_at {
        Some(RowCommand::OpenPath(path.clone()))
    } else if device.can_mount {
        Some(RowCommand::Storage(Command::Mount {
            id: device.id.clone(),
        }))
    } else {
        None
    }
}

fn device_actions(device: &StorageDevice) -> Vec<DeviceAction<RowCommand>> {
    storage_device_actions(device)
        .into_iter()
        .map(|action| DeviceAction {
            id: action.id,
            label: action.label,
            destructive: action.destructive,
            enabled: action.enabled,
            visible: action.visible,
            command: RowCommand::Storage(action.command),
        })
        .collect()
}

pub(super) fn storage_device_actions(device: &StorageDevice) -> Vec<DeviceAction<Command>> {
    if device.busy {
        return Vec::new();
    }

    let mut actions = Vec::new();

    if device.mounted_at.is_some() && device.can_unmount {
        actions.push(DeviceAction {
            id: "unmount".into(),
            label: "Unmount".into(),
            destructive: false,
            enabled: true,
            visible: true,
            command: Command::Unmount {
                id: device.id.clone(),
            },
        });
    } else if device.can_mount {
        actions.push(DeviceAction {
            id: "mount".into(),
            label: "Mount".into(),
            destructive: false,
            enabled: true,
            visible: true,
            command: Command::Mount {
                id: device.id.clone(),
            },
        });
    }

    if device.can_eject {
        actions.push(DeviceAction {
            id: "eject".into(),
            label: "Eject".into(),
            destructive: false,
            enabled: true,
            visible: true,
            command: Command::Eject {
                id: device.id.clone(),
            },
        });
    }

    actions
}

fn popover_output_for_row_command(command: RowCommand) -> PopoverOutput {
    match command {
        RowCommand::Storage(command) => PopoverOutput::Command(command),
        RowCommand::OpenPath(path) => PopoverOutput::OpenPath(path),
    }
}

fn device_status(device: &StorageDevice) -> DeviceStatus {
    if device.busy {
        DeviceStatus {
            text: None,
            busy: true,
        }
    } else if device.mounted_at.is_some() {
        DeviceStatus::text("Mounted")
    } else if device.can_power_off || device.can_eject {
        DeviceStatus::empty()
    } else if device.can_mount {
        DeviceStatus::text("Available")
    } else {
        DeviceStatus::text("Not mounted")
    }
}

impl DeviceStatus {
    fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            busy: false,
        }
    }

    fn empty() -> Self {
        Self {
            text: None,
            busy: false,
        }
    }
}

fn device_tooltip(device: &StorageDevice) -> String {
    let mut parts = Vec::new();
    if let Some(mounted_at) = &device.mounted_at {
        parts.push(format!("Mounted at {}", mounted_at.display()));
    }
    if let Some(size) = device.size_bytes {
        parts.push(format_size(size));
    }
    if let Some(filesystem) = &device.filesystem {
        parts.push(filesystem.clone());
    }

    if parts.is_empty() {
        device.name.clone()
    } else {
        parts.join(" - ")
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1000.0 && unit + 1 < UNITS.len() {
        size /= 1000.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> StorageDevice {
        StorageDevice {
            id: "device".into(),
            name: "USB Drive".into(),
            icon: "drive-removable-media-symbolic".into(),
            can_mount: true,
            can_unmount: true,
            can_eject: true,
            can_power_off: true,
            ..StorageDevice::default()
        }
    }

    #[test]
    fn mounted_device_primary_action_opens_mount_point() {
        let device = StorageDevice {
            mounted_at: Some("/run/media/alex/USB".into()),
            ..device()
        };

        assert_eq!(
            primary_device_command(&device),
            Some(RowCommand::OpenPath("/run/media/alex/USB".into()))
        );
    }

    #[test]
    fn unmounted_mountable_device_primary_action_is_mount() {
        let device = device();
        assert_eq!(
            primary_device_command(&device),
            Some(RowCommand::Storage(Command::Mount {
                id: "device".into()
            }))
        );
    }

    #[test]
    fn busy_device_has_no_primary_action() {
        let device = StorageDevice {
            busy: true,
            ..device()
        };

        assert_eq!(primary_device_command(&device), None);
        assert!(device_actions(&device).is_empty());
    }

    #[test]
    fn hero_subtitle_summarizes_device_count() {
        assert_eq!(hero_subtitle(&State::default()), "No devices connected");

        let one = State {
            devices: vec![device()],
            ..State::default()
        };
        assert_eq!(hero_subtitle(&one), "1 device");

        let two = State {
            devices: vec![
                device(),
                StorageDevice {
                    id: "other".into(),
                    ..device()
                },
            ],
            ..State::default()
        };
        assert_eq!(hero_subtitle(&two), "2 devices");
    }

    #[test]
    fn safe_to_remove_device_has_no_status_and_still_uses_mount_primary_action() {
        let row = build_device_rows(&State {
            devices: vec![device()],
            ..State::default()
        })
        .remove(0);

        assert_eq!(row.status.text, None);
        assert_eq!(
            row.command,
            Some(RowCommand::Storage(Command::Mount {
                id: "device".into()
            }))
        );
    }

    #[test]
    fn device_status_uses_user_facing_states() {
        assert_eq!(
            device_status(&device()),
            DeviceStatus {
                text: None,
                busy: false,
            }
        );

        let mounted = StorageDevice {
            mounted_at: Some("/run/media/alex/USB".into()),
            ..device()
        };
        assert_eq!(device_status(&mounted), DeviceStatus::text("Mounted"));

        let mountable = StorageDevice {
            can_power_off: false,
            can_eject: false,
            ..device()
        };
        assert_eq!(device_status(&mountable), DeviceStatus::text("Available"));
    }

    #[test]
    fn removable_device_actions_include_available_operations() {
        let actions = device_actions(&device());

        assert_eq!(
            actions
                .iter()
                .map(|action| action.id.as_str())
                .collect::<Vec<_>>(),
            vec!["mount", "eject"]
        );

        let mounted = StorageDevice {
            mounted_at: Some("/run/media/alex/USB".into()),
            ..device()
        };
        let actions = device_actions(&mounted);

        assert_eq!(actions[0].id, "unmount");
        assert!(actions.iter().all(|action| action.enabled));
        assert_eq!(
            actions[0].command,
            RowCommand::Storage(Command::Unmount {
                id: "device".into()
            })
        );
    }

    #[test]
    fn popover_output_maps_row_commands() {
        assert_eq!(
            popover_output_for_row_command(RowCommand::OpenPath("/run/media/alex/USB".into())),
            PopoverOutput::OpenPath("/run/media/alex/USB".into())
        );
        assert_eq!(
            popover_output_for_row_command(RowCommand::Storage(Command::Refresh)),
            PopoverOutput::Command(Command::Refresh)
        );
    }
}
