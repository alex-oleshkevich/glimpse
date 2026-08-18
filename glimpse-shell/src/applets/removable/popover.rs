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
    error_box: gtk::Box,
    list: gtk::Box,
    hero_subtitle: String,
    rows: HashMap<String, RemovableRow>,
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

                #[local_ref]
                error_box -> gtk::Box {},

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
        let error_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 2);

        let mut model = Popover {
            popover: AnimatedPopover::new(),
            error_box: error_box.clone(),
            list: list.clone(),
            hero_subtitle: hero_subtitle(&State::default()),
            rows: HashMap::new(),
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
                self.hero_subtitle = hero_subtitle(&state);
                self.list.set_visible(!state.devices.is_empty());
                self.sync_error_banner(&state);
                self.sync_rows(&state, &sender);
            }
            PopoverInput::DeviceCommand(command) => {
                let _ = sender.output(popover_output_for_row_command(command));
            }
        }
    }
}

impl Popover {
    /// Mirrors printing's sync_error_banners: state.error is set on every
    /// failed storage command (mount/unmount/eject/...) but previously had
    /// no rendering, so a failure just silently stopped the row's spinner.
    fn sync_error_banner(&self, state: &State) {
        while let Some(child) = self.error_box.first_child() {
            self.error_box.remove(&child);
        }
        if let Some(error) = &state.error {
            let tile = Tile::new();
            tile.add_css_class("error-banner");
            tile.set_primary("Storage error");
            tile.set_secondary(Some(error.as_str()));
            tile.set_activatable(false);
            self.error_box.append(&tile);
        }
        self.error_box
            .set_visible(self.error_box.first_child().is_some());
    }

    /// Keyed diff instead of tearing down and rebuilding every row: a row's
    /// SegmentedTile is a persistent GTK object, so an in-place mount/eject
    /// while a row is expanded doesn't collapse it out from under the user.
    fn sync_rows(&mut self, state: &State, sender: &ComponentSender<Popover>) {
        let mut seen: HashSet<String> = HashSet::new();
        let mut previous: Option<gtk::Widget> = None;
        // The storage service refuses any command while it has an action in
        // flight (not just commands targeting the busy device), so disable
        // every row's actions during that window instead of only the one
        // showing a spinner.
        let globally_busy = state.active_action.is_some();

        for device in &state.devices {
            seen.insert(device.id.clone());
            let row = self
                .rows
                .entry(device.id.clone())
                .or_insert_with(|| RemovableRow::new(device, sender));
            row.update(device, globally_busy, sender);
            place_row(row, &self.list, previous.as_ref());
            previous = Some(row.widget().clone());
        }

        self.rows.retain(|id, row| {
            let keep = seen.contains(id);
            if !keep {
                let widget = row.widget();
                if let Some(parent) = widget.parent()
                    && let Ok(parent) = parent.downcast::<gtk::Box>()
                {
                    parent.remove(widget);
                }
            }
            keep
        });
    }
}

fn place_row(row: &RemovableRow, container: &gtk::Box, previous: Option<&gtk::Widget>) {
    let row_widget = row.widget();
    let target = container.clone().upcast::<gtk::Widget>();
    let already_in_container = row_widget.parent().is_some_and(|parent| parent == target);

    if !already_in_container {
        if let Some(parent) = row_widget.parent()
            && let Ok(parent) = parent.downcast::<gtk::Box>()
        {
            parent.remove(row_widget);
        }
        container.append(row_widget);
    }
    container.reorder_child_after(row_widget, previous);
}

// ─── Row ────────────────────────────────────────────────────────────────

struct RemovableRow {
    root: SegmentedTile,
    icon: gtk::Image,
    status_box: gtk::Box,
    spinner: gtk::Spinner,
    status_label: gtk::Label,
    actions_box: gtk::Box,
    primary_command: Rc<RefCell<Option<RowCommand>>>,
    id: String,
}

impl RemovableRow {
    fn new(device: &StorageDevice, sender: &ComponentSender<Popover>) -> Self {
        let root = SegmentedTile::new();
        root.add_css_class("removable-device-row");
        root.set_secondary(None);

        let icon = gtk::Image::new();
        icon.add_css_class("removable-device-row__icon");
        icon.set_pixel_size(16);
        icon.set_valign(gtk::Align::Center);
        root.set_left(Some(icon.clone()));

        let spinner = gtk::Spinner::new();
        spinner.add_css_class("removable-device-row__spinner");
        let status_label = gtk::Label::new(None);
        status_label.add_css_class("dim-label");
        status_label.add_css_class("caption");
        let status_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        status_box.set_valign(gtk::Align::Center);
        status_box.append(&spinner);
        status_box.append(&status_label);
        root.set_right(Some(status_box.clone()));

        let actions_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        actions_box.add_css_class("removable-device-row__actions");

        // The primary command depends on mutable device state (Mount vs
        // OpenPath vs disabled), so it's read from a cell at click time
        // rather than wiring a fresh connect_activated closure per update
        // (which would stack handlers on this now-persistent tile).
        let primary_command: Rc<RefCell<Option<RowCommand>>> = Rc::new(RefCell::new(None));
        root.connect_activated({
            let sender = sender.clone();
            let primary_command = primary_command.clone();
            move |_| {
                if let Some(cmd) = primary_command.borrow().clone() {
                    sender.input(PopoverInput::DeviceCommand(cmd));
                }
            }
        });

        let mut row = Self {
            root,
            icon,
            status_box,
            spinner,
            status_label,
            actions_box,
            primary_command,
            id: device.id.clone(),
        };
        // Real value applied immediately after by the sync_rows() caller.
        row.update(device, false, sender);
        row
    }

    fn update(
        &mut self,
        device: &StorageDevice,
        globally_busy: bool,
        sender: &ComponentSender<Popover>,
    ) {
        debug_assert_eq!(self.id, device.id);
        self.root.set_primary(&device.name);
        self.root.set_tooltip_text(Some(&device_tooltip(device)));
        self.icon.set_icon_name(Some(&device.icon));

        if device.mounted_at.is_some() {
            self.root.add_css_class("is-active");
            self.root.add_css_class("is-selected");
        } else {
            self.root.remove_css_class("is-active");
            self.root.remove_css_class("is-selected");
        }

        let status = device_status(device);
        self.status_box
            .set_visible(status.busy || status.text.is_some());
        self.spinner.set_visible(status.busy);
        self.spinner.set_spinning(status.busy);
        self.status_label.set_visible(!status.busy);
        if let Some(text) = &status.text {
            self.status_label.set_label(text);
        }

        let command = if globally_busy {
            None
        } else {
            primary_device_command(device)
        };
        self.root.set_activatable(command.is_some());
        *self.primary_command.borrow_mut() = command;

        while let Some(child) = self.actions_box.first_child() {
            self.actions_box.remove(&child);
        }
        let actions: Vec<_> = device_actions(device)
            .into_iter()
            .filter(|a| a.visible)
            .collect();
        if actions.is_empty() {
            self.root.set_child(None::<gtk::Widget>);
        } else {
            for action in actions {
                let action_tile = Tile::new();
                action_tile.add_css_class("removable-device-action");
                action_tile.set_primary(&action.label);
                action_tile.set_secondary(None);
                action_tile.set_sensitive(action.enabled && !globally_busy);
                if action.destructive {
                    action_tile.add_css_class("destructive-action");
                }
                let cmd = action.command;
                action_tile.connect_activated({
                    let sender = sender.clone();
                    move |_| sender.input(PopoverInput::DeviceCommand(cmd.clone()))
                });
                self.actions_box.append(&action_tile);
            }
            self.root.set_child(Some(self.actions_box.clone()));
        }
    }

    fn widget(&self) -> &gtk::Widget {
        self.root.upcast_ref()
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
            destructive: true,
            enabled: true,
            visible: true,
            command: Command::Eject {
                id: device.id.clone(),
            },
        });
    }

    if device.can_power_off {
        actions.push(DeviceAction {
            id: "power_off".into(),
            label: "Power off".into(),
            destructive: true,
            enabled: true,
            visible: true,
            command: Command::PowerOff {
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
    fn eject_and_power_off_actions_are_marked_destructive() {
        let actions = storage_device_actions(&device());

        let eject = actions.iter().find(|a| a.id == "eject").unwrap();
        assert!(eject.destructive);

        let power_off = actions.iter().find(|a| a.id == "power_off").unwrap();
        assert!(power_off.destructive);
        assert_eq!(
            power_off.command,
            Command::PowerOff {
                id: "device".into()
            }
        );
    }

    #[test]
    fn power_off_action_is_absent_when_device_cannot_power_off() {
        let device = StorageDevice {
            can_power_off: false,
            ..device()
        };

        assert!(
            !storage_device_actions(&device)
                .iter()
                .any(|a| a.id == "power_off")
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
        let device = device();
        assert_eq!(device_status(&device).text, None);
        assert_eq!(
            primary_device_command(&device),
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
            vec!["mount", "eject", "power_off"]
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
