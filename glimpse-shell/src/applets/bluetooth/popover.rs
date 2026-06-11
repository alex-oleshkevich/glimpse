use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
    time::Duration,
};

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    services::bluetooth::{
        BluetoothActiveAction, BluetoothAdapter, BluetoothDevice, BluetoothServiceHealth,
        BluetoothSnapshot, Command, State,
    },
    utils::popover_scroll,
    widgets::{
        animated_popover::AnimatedPopover, expander_tile::ExpanderTile, hero::Hero,
        key_value_grid::KeyValueGrid, popover_shell::PopoverShell, segmented_tile::SegmentedTile,
        switch_tile::SwitchTile, tile::Tile,
    },
};

const DISCOVERABLE_DEBOUNCE: Duration = Duration::from_millis(1200);

pub struct Popover {
    popover: AnimatedPopover,
    state: State,
    sections: DeviceSections,
    powered: bool,
    discoverable: bool,
    pending_discoverable: Option<PendingDiscoverable>,
    discoverable_generation: u64,
    has_adapter: bool,
    paired_expanded: bool,
    nearby_expanded: bool,
    updating_power: Rc<Cell<bool>>,
    updating_discoverable: Rc<Cell<bool>>,
    hero: Hero,
    discoverable_tile: SwitchTile,
    connected_list: gtk::Box,
    paired_list: gtk::Box,
    nearby_list: gtk::Box,
    connected_rows: HashMap<String, SegmentedDeviceRow>,
    paired_rows: HashMap<String, SimpleDeviceRow>,
    nearby_rows: HashMap<String, SimpleDeviceRow>,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateState(State),
    SetPowered(bool),
    SetDiscoverable(bool),
    ExpirePendingDiscoverable(u64),
    SetPairedExpanded(bool),
    SetNearbyExpanded(bool),
    DeviceCommand(Command),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopoverOutput {
    Command(Command),
    Closed,
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

                #[name = "hero"]
                Hero {
                    #[watch]
                    set_icon: Some(hero_icon_name_for_state(&model.state)),
                    set_title: "Bluetooth",
                    #[watch]
                    set_subtitle: &hero_subtitle_text(&model.state),
                },

                #[name = "scroller"]
                gtk::ScrolledWindow {
                    set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                    set_vexpand: false,
                    set_propagate_natural_height: true,

                    #[name = "content"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 2,

                        #[name = "connected_list"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 2,
                            #[watch]
                            set_visible: model.sections.connected_visible(),
                        },

                        gtk::Separator {
                            set_orientation: gtk::Orientation::Horizontal,
                            #[watch]
                            set_visible: model.sections.connected_visible() && model.sections.lower_visible(),
                        },

                        #[name = "paired_section"]
                        ExpanderTile {
                            add_css_class: "bluetooth-paired-section",
                            set_primary: "Paired devices",
                            set_secondary: None,
                            #[watch]
                            set_visible: model.sections.paired_visible(),
                            #[watch]
                            set_expanded: model.paired_expanded,

                            connect_expanded[sender] => move |_, expanded| {
                                sender.input(PopoverInput::SetPairedExpanded(expanded));
                            },
                        },

                        #[name = "nearby_section"]
                        ExpanderTile {
                            add_css_class: "bluetooth-nearby-section",
                            set_primary: "Nearby devices",
                            set_secondary: None,
                            #[watch]
                            set_visible: model.sections.nearby_visible(),
                            #[watch]
                            set_expanded: model.nearby_expanded,

                            connect_expanded[sender] => move |_, expanded| {
                                sender.input(PopoverInput::SetNearbyExpanded(expanded));
                            },
                        },

                        #[name = "discoverable_tile"]
                        SwitchTile {
                            set_primary: "Make discoverable",
                            #[watch]
                            set_visible: model.has_adapter,
                            #[watch]
                            set_sensitive: model.powered,
                        },
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let updating_power = Rc::new(Cell::new(false));
        let updating_discoverable = Rc::new(Cell::new(false));
        let state = State::default();
        let sections = device_sections(&state);
        let mut model = Popover {
            popover: AnimatedPopover::new(),
            state,
            sections,
            powered: false,
            discoverable: false,
            pending_discoverable: None,
            discoverable_generation: 0,
            has_adapter: false,
            paired_expanded: false,
            nearby_expanded: false,
            updating_power,
            updating_discoverable,
            hero: Hero::new(),
            discoverable_tile: SwitchTile::new(),
            connected_list: gtk::Box::new(gtk::Orientation::Vertical, 2),
            paired_list: gtk::Box::new(gtk::Orientation::Vertical, 2),
            nearby_list: gtk::Box::new(gtk::Orientation::Vertical, 2),
            connected_rows: HashMap::new(),
            paired_rows: HashMap::new(),
            nearby_rows: HashMap::new(),
        };

        let widgets = view_output!();
        model.popover = widgets.root.clone();
        model.hero = widgets.hero.clone();
        model.discoverable_tile = widgets.discoverable_tile.clone();
        model.connected_list = widgets.connected_list.clone();

        widgets.root.set_parent(&init.parent);
        popover_scroll::install_half_monitor_limit(
            widgets.root.upcast_ref(),
            &widgets.scroller,
            &init.parent,
        );

        widgets.hero.set_trailing_visible(true);
        widgets.hero.connect_toggled({
            let guard = model.updating_power.clone();
            let sender = sender.clone();
            move |_, active| {
                if !guard.get() {
                    sender.input(PopoverInput::SetPowered(active));
                }
            }
        });

        widgets.discoverable_tile.set_secondary(None);
        widgets.discoverable_tile.connect_toggled({
            let guard = model.updating_discoverable.clone();
            let sender = sender.clone();
            move |_, active| {
                if !guard.get() {
                    sender.input(PopoverInput::SetDiscoverable(active));
                }
            }
        });

        widgets
            .paired_section
            .set_child(Some(model.paired_list.clone()));

        let nearby_scroller = gtk::ScrolledWindow::new();
        nearby_scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        nearby_scroller.set_vexpand(false);
        nearby_scroller.set_propagate_natural_height(true);
        nearby_scroller.set_min_content_height(80);
        nearby_scroller.set_max_content_height(220);
        nearby_scroller.set_child(Some(&model.nearby_list));
        widgets.nearby_section.set_child(Some(nearby_scroller));

        model.sync_toggles();
        model.sync_rows(&sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => {
                let closing = self.popover.is_open();
                self.popover.toggle();
                if closing {
                    let _ = sender.output(PopoverOutput::Closed);
                }
            }
            PopoverInput::UpdateState(state) => {
                self.powered = state.snapshot.status.powered;
                self.discoverable = reconcile_pending_discoverable(
                    &mut self.pending_discoverable,
                    adapter_discoverable(&state),
                );
                self.has_adapter = primary_adapter(&state).is_some();
                self.sections = device_sections(&state);
                self.state = state;
                self.sync_toggles();
                self.sync_rows(&sender);
            }
            PopoverInput::SetPowered(powered) => {
                let _ = sender.output(PopoverOutput::Command(Command::SetPowered(powered)));
            }
            PopoverInput::SetDiscoverable(discoverable) => {
                if let Some(command) = discoverable_command(&self.state, discoverable) {
                    self.discoverable_generation = self.discoverable_generation.wrapping_add(1);
                    let generation = self.discoverable_generation;
                    self.pending_discoverable =
                        Some(PendingDiscoverable::new(discoverable, generation));
                    self.discoverable = discoverable;
                    self.sync_toggles();
                    let _ = sender.output(PopoverOutput::Command(command));
                    let sender = sender.clone();
                    gtk::glib::timeout_add_local_once(DISCOVERABLE_DEBOUNCE, move || {
                        sender.input(PopoverInput::ExpirePendingDiscoverable(generation));
                    });
                }
            }
            PopoverInput::ExpirePendingDiscoverable(generation) => {
                if expire_pending_discoverable(&mut self.pending_discoverable, generation) {
                    self.discoverable = adapter_discoverable(&self.state);
                    self.sync_toggles();
                }
            }
            PopoverInput::SetPairedExpanded(expanded) => {
                self.paired_expanded = expanded;
            }
            PopoverInput::SetNearbyExpanded(expanded) => {
                self.nearby_expanded = expanded;
            }
            PopoverInput::DeviceCommand(command) => {
                let _ = sender.output(PopoverOutput::Command(command));
            }
        }
    }
}

impl Popover {
    fn sync_toggles(&self) {
        self.updating_power.set(true);
        self.hero.set_toggle_active(self.powered);
        self.updating_power.set(false);

        self.updating_discoverable.set(true);
        self.discoverable_tile.set_active(self.discoverable);
        self.updating_discoverable.set(false);
    }

    fn sync_rows(&mut self, sender: &ComponentSender<Self>) {
        sync_segmented_device_rows(
            &mut self.connected_rows,
            &self.connected_list,
            self.sections.connected.clone(),
            sender,
        );
        sync_simple_device_rows(
            &mut self.paired_rows,
            &self.paired_list,
            self.sections.paired.clone(),
            sender,
        );
        sync_simple_device_rows(
            &mut self.nearby_rows,
            &self.nearby_list,
            self.sections.nearby.clone(),
            sender,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingDiscoverable {
    value: bool,
    generation: u64,
}

impl PendingDiscoverable {
    fn new(value: bool, generation: u64) -> Self {
        Self { value, generation }
    }
}

fn reconcile_pending_discoverable(pending: &mut Option<PendingDiscoverable>, actual: bool) -> bool {
    let Some(pending_value) = pending.as_mut() else {
        return actual;
    };

    if actual == pending_value.value {
        *pending = None;
        return actual;
    }

    pending_value.value
}

fn expire_pending_discoverable(pending: &mut Option<PendingDiscoverable>, generation: u64) -> bool {
    if pending
        .as_ref()
        .is_some_and(|pending_value| pending_value.generation == generation)
    {
        *pending = None;
        true
    } else {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceSection {
    Connected,
    Paired,
    Nearby,
}

impl DeviceSection {
    fn has_expanded_content(self) -> bool {
        matches!(self, Self::Connected)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct DeviceSections {
    connected: Vec<DeviceRowModel>,
    paired: Vec<DeviceRowModel>,
    nearby: Vec<DeviceRowModel>,
}

impl DeviceSections {
    fn connected_visible(&self) -> bool {
        !self.connected.is_empty()
    }

    fn paired_visible(&self) -> bool {
        !self.paired.is_empty()
    }

    fn nearby_visible(&self) -> bool {
        !self.nearby.is_empty()
    }

    fn lower_visible(&self) -> bool {
        self.paired_visible() || self.nearby_visible()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeviceRowModel {
    id: String,
    label: String,
    icon: String,
    status: Option<String>,
    tooltip: String,
    busy: bool,
    active: bool,
    command: Command,
    details: Vec<DetailRow>,
    actions: Vec<DeviceAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DetailRow {
    key: &'static str,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeviceAction {
    id: &'static str,
    label: &'static str,
    destructive: bool,
    command: Command,
}

fn device_sections(state: &State) -> DeviceSections {
    let busy_address = busy_device_address(state);
    let mut sections = DeviceSections::default();

    for device in visible_devices(&state.snapshot) {
        let section = device_section(device);
        let model = device_row_model(device, section, busy_address);
        match section {
            DeviceSection::Connected => sections.connected.push(model),
            DeviceSection::Paired => sections.paired.push(model),
            DeviceSection::Nearby => sections.nearby.push(model),
        }
    }

    sections
}

fn device_row_model(
    device: &BluetoothDevice,
    section: DeviceSection,
    busy_address: Option<&str>,
) -> DeviceRowModel {
    DeviceRowModel {
        id: device.address.clone(),
        label: device.name.clone(),
        icon: device.device_type.icon(device.connected).into(),
        status: device_status(device),
        tooltip: device_tooltip(device, section),
        busy: busy_address == Some(device.address.as_str()),
        active: device.connected,
        command: primary_device_command(device, section),
        details: if section.has_expanded_content() {
            device_details(device, section)
        } else {
            Vec::new()
        },
        actions: if section.has_expanded_content() {
            device_actions(device, section)
        } else {
            Vec::new()
        },
    }
}

fn device_section(device: &BluetoothDevice) -> DeviceSection {
    if device.connected {
        DeviceSection::Connected
    } else if device.paired || device.trusted {
        DeviceSection::Paired
    } else {
        DeviceSection::Nearby
    }
}

fn busy_device_address(state: &State) -> Option<&str> {
    match state.active_action.as_ref()? {
        BluetoothActiveAction::Connect { address } | BluetoothActiveAction::Pair { address } => {
            Some(address.as_str())
        }
        _ => None,
    }
}

fn visible_devices(snapshot: &BluetoothSnapshot) -> Vec<&BluetoothDevice> {
    let mut devices = snapshot
        .devices
        .iter()
        .filter(|device| is_visible_device(device))
        .collect::<Vec<_>>();
    devices.sort_by(|left, right| {
        right
            .connected
            .cmp(&left.connected)
            .then(right.paired.cmp(&left.paired))
            .then(right.trusted.cmp(&left.trusted))
            .then(
                right
                    .rssi
                    .unwrap_or(i16::MIN)
                    .cmp(&left.rssi.unwrap_or(i16::MIN)),
            )
            .then(left.name.cmp(&right.name))
    });
    let mut seen: HashSet<&str> = HashSet::new();
    devices.retain(|device| seen.insert(device.address.as_str()));
    devices
}

fn is_visible_device(device: &BluetoothDevice) -> bool {
    if device.address.is_empty() {
        return false;
    }

    if device.name.is_empty() || looks_like_mac(&device.name) {
        return device.connected || device.paired || device.trusted;
    }

    device.connected || device.paired || device.trusted || device.rssi.is_some()
}

fn primary_device_command(device: &BluetoothDevice, section: DeviceSection) -> Command {
    if device.connected {
        Command::Disconnect {
            address: device.address.clone(),
        }
    } else if matches!(section, DeviceSection::Paired) {
        Command::Connect {
            address: device.address.clone(),
        }
    } else {
        Command::Pair {
            address: device.address.clone(),
        }
    }
}

fn device_actions(device: &BluetoothDevice, section: DeviceSection) -> Vec<DeviceAction> {
    let mut actions = vec![DeviceAction {
        id: if device.trusted { "untrust" } else { "trust" },
        label: if device.trusted { "Untrust" } else { "Trust" },
        destructive: false,
        command: Command::Trust {
            address: device.address.clone(),
            trusted: !device.trusted,
        },
    }];

    if matches!(section, DeviceSection::Connected | DeviceSection::Paired) {
        actions.push(DeviceAction {
            id: "forget",
            label: "Forget",
            destructive: true,
            command: Command::Forget {
                address: device.address.clone(),
            },
        });
    }

    actions
}

fn device_details(device: &BluetoothDevice, section: DeviceSection) -> Vec<DetailRow> {
    let mut rows = Vec::new();
    let device_type = device.device_type.label();
    if !device_type.is_empty() {
        rows.push(detail("Type", device_type));
    }
    rows.push(detail("Status", device_state_label(device, section)));
    if let Some(battery) = device.battery {
        rows.push(detail("Battery", format!("{battery}%")));
    }
    if let Some(rssi) = device.rssi {
        rows.push(detail("Signal", format!("{rssi} dBm")));
    }
    rows.push(detail("Address", device.address.clone()));
    if matches!(section, DeviceSection::Connected | DeviceSection::Paired) {
        rows.push(detail("Trusted", yes_no(device.trusted)));
    }
    rows
}

fn detail(key: &'static str, value: impl Into<String>) -> DetailRow {
    DetailRow {
        key,
        value: value.into(),
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "Yes" } else { "No" }
}

fn device_status(device: &BluetoothDevice) -> Option<String> {
    device.battery.map(|percentage| format!("{percentage}%"))
}

fn device_tooltip(device: &BluetoothDevice, section: DeviceSection) -> String {
    let mut parts = Vec::new();
    let device_type = device.device_type.label();
    if !device_type.is_empty() {
        parts.push(device_type.to_owned());
    }
    parts.push(device_state_label(device, section));
    parts.join(" · ")
}

fn device_state_label(device: &BluetoothDevice, section: DeviceSection) -> String {
    if device.connected {
        "Connected".into()
    } else if device.paired {
        "Paired".into()
    } else if device.trusted {
        "Trusted".into()
    } else if matches!(section, DeviceSection::Nearby) {
        "Nearby".into()
    } else {
        "Available".into()
    }
}

fn discoverable_command(state: &State, discoverable: bool) -> Option<Command> {
    primary_adapter(state).map(|adapter| Command::SetAdapterDiscoverable {
        adapter_path: adapter.path.clone(),
        discoverable,
    })
}

fn primary_adapter(state: &State) -> Option<&BluetoothAdapter> {
    state
        .snapshot
        .adapters
        .iter()
        .find(|adapter| adapter.powered)
        .or_else(|| state.snapshot.adapters.first())
}

fn adapter_discoverable(state: &State) -> bool {
    primary_adapter(state).is_some_and(|adapter| adapter.discoverable)
}

struct DeviceActionTile {
    tile: Tile,
    command: Rc<RefCell<Command>>,
}

struct SegmentedDeviceRow {
    root: SegmentedTile,
    icon: gtk::Image,
    status: gtk::Label,
    command: Rc<RefCell<Command>>,
    details: KeyValueGrid,
    actions: gtk::Box,
    action_tiles: RefCell<HashMap<&'static str, DeviceActionTile>>,
}

impl SegmentedDeviceRow {
    fn new(model: &DeviceRowModel, sender: &ComponentSender<Popover>) -> Self {
        let root = SegmentedTile::new();
        root.add_css_class("bluetooth-device-row");

        let icon = gtk::Image::from_icon_name(&model.icon);
        icon.set_pixel_size(16);
        icon.add_css_class("bluetooth-device-row__icon");
        root.set_left(Some(icon.clone()));
        root.set_secondary(None);

        let status = gtk::Label::new(None);
        status.add_css_class("dim-label");
        status.add_css_class("caption");
        status.add_css_class("numeric");
        status.set_valign(gtk::Align::Center);

        let details = KeyValueGrid::new();
        let actions = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
        content.append(&actions);
        content.append(&details);
        root.set_child(Some(content));

        let command = Rc::new(RefCell::new(model.command.clone()));
        root.connect_activated({
            let sender = sender.clone();
            let command = command.clone();
            move |_| sender.input(PopoverInput::DeviceCommand(command.borrow().clone()))
        });

        let row = Self {
            root,
            icon,
            status,
            command,
            details,
            actions,
            action_tiles: RefCell::new(HashMap::new()),
        };
        row.update(model, sender);
        row
    }

    fn update(&self, model: &DeviceRowModel, sender: &ComponentSender<Popover>) {
        self.root.set_primary(&model.label);
        self.root.set_tooltip_text(Some(&model.tooltip));
        self.icon.set_icon_name(Some(&model.icon));
        self.command.replace(model.command.clone());
        self.root.set_activatable(!model.busy);
        self.root.set_sensitive(!model.busy);

        if let Some(status) = &model.status {
            self.status.set_label(status);
            self.root.set_right(Some(self.status.clone()));
        } else {
            self.root.set_right(None::<gtk::Widget>);
        }

        self.details.clear();
        for row in &model.details {
            self.details.add_row(row.key, &row.value);
        }

        self.actions.set_visible(!model.actions.is_empty());
        
        let mut tiles = self.action_tiles.borrow_mut();
        let current_ids: HashSet<&'static str> = model.actions.iter().map(|a| a.id).collect();
        
        tiles.retain(|id, state| {
            if !current_ids.contains(id) {
                self.actions.remove(&state.tile);
                false
            } else {
                true
            }
        });

        for action in &model.actions {
            if let Some(state) = tiles.get(action.id) {
                state.tile.set_primary(action.label);
                if action.destructive {
                    state.tile.add_css_class("destructive-action");
                } else {
                    state.tile.remove_css_class("destructive-action");
                }
                state.command.replace(action.command.clone());
            } else {
                let tile = Tile::new();
                tile.set_primary(action.label);
                tile.set_secondary(None);
                tile.add_css_class("bluetooth-device-action");
                if action.destructive {
                    tile.add_css_class("destructive-action");
                }
                let command = Rc::new(RefCell::new(action.command.clone()));
                tile.connect_activated({
                    let sender = sender.clone();
                    let command = command.clone();
                    move |_| sender.input(PopoverInput::DeviceCommand(command.borrow().clone()))
                });
                self.actions.append(&tile);
                tiles.insert(action.id, DeviceActionTile { tile, command });
            }
        }
    }

    fn widget(&self) -> &gtk::Widget {
        self.root.upcast_ref()
    }
}

struct SimpleDeviceRow {
    root: Tile,
    icon: gtk::Image,
    status: gtk::Label,
    command: Rc<RefCell<Command>>,
}

impl SimpleDeviceRow {
    fn new(model: &DeviceRowModel, sender: &ComponentSender<Popover>) -> Self {
        let root = Tile::new();
        root.add_css_class("bluetooth-device-row");

        let icon = gtk::Image::from_icon_name(&model.icon);
        icon.set_pixel_size(16);
        icon.add_css_class("bluetooth-device-row__icon");
        root.set_left(Some(icon.clone()));
        root.set_secondary(None);

        let status = gtk::Label::new(None);
        status.add_css_class("dim-label");
        status.add_css_class("caption");
        status.add_css_class("numeric");
        status.set_valign(gtk::Align::Center);

        let command = Rc::new(RefCell::new(model.command.clone()));
        root.connect_activated({
            let sender = sender.clone();
            let command = command.clone();
            move |_| sender.input(PopoverInput::DeviceCommand(command.borrow().clone()))
        });

        let row = Self {
            root,
            icon,
            status,
            command,
        };
        row.update(model);
        row
    }

    fn update(&self, model: &DeviceRowModel) {
        self.root.set_primary(&model.label);
        self.root.set_tooltip_text(Some(&model.tooltip));
        self.icon.set_icon_name(Some(&model.icon));
        self.command.replace(model.command.clone());
        self.root.set_activatable(!model.busy);
        self.root.set_sensitive(!model.busy);

        if let Some(status) = &model.status {
            self.status.set_label(status);
            self.root.set_right(Some(self.status.clone()));
        } else {
            self.root.set_right(None::<gtk::Widget>);
        }
    }

    fn widget(&self) -> &gtk::Widget {
        self.root.upcast_ref()
    }
}

fn sync_segmented_device_rows(
    rows: &mut HashMap<String, SegmentedDeviceRow>,
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
            .or_insert_with(|| SegmentedDeviceRow::new(&model, sender));
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

fn sync_simple_device_rows(
    rows: &mut HashMap<String, SimpleDeviceRow>,
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
            .or_insert_with(|| SimpleDeviceRow::new(&model, sender));
        row.update(&model);
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

fn hero_icon_name_for_state(state: &State) -> &'static str {
    if !state.snapshot.status.powered {
        "bluetooth-disabled-symbolic"
    } else if state.snapshot.status.connected_count > 0 {
        "bluetooth-active-symbolic"
    } else {
        "bluetooth-symbolic"
    }
}

fn hero_subtitle_text(state: &State) -> String {
    match &state.health {
        BluetoothServiceHealth::Starting => return "Starting".into(),
        BluetoothServiceHealth::Reconnecting { .. } => return "Reconnecting".into(),
        BluetoothServiceHealth::Degraded { message } => return message.clone(),
        BluetoothServiceHealth::Ready => {}
    }

    if let Some(activity) = active_action_text(state) {
        return activity;
    }

    let status = &state.snapshot.status;
    if !status.powered {
        "Off".into()
    } else if status.discovering {
        "Discovering".into()
    } else if status.connected_count > 0 {
        format!("{} connected", status.connected_count)
    } else {
        "Ready".into()
    }
}

fn active_action_text(state: &State) -> Option<String> {
    match state.active_action.as_ref()? {
        BluetoothActiveAction::SetPowered(true) => Some("Turning Bluetooth on".into()),
        BluetoothActiveAction::SetPowered(false) => Some("Turning Bluetooth off".into()),
        BluetoothActiveAction::SetAdapterPowered { powered: true, .. } => {
            Some("Turning adapter on".into())
        }
        BluetoothActiveAction::SetAdapterPowered { powered: false, .. } => {
            Some("Turning adapter off".into())
        }
        BluetoothActiveAction::SetAdapterDiscoverable {
            discoverable: true, ..
        } => Some("Making adapter discoverable".into()),
        BluetoothActiveAction::SetAdapterDiscoverable {
            discoverable: false,
            ..
        } => Some("Hiding adapter".into()),
        BluetoothActiveAction::Connect { address } => Some(format!(
            "Connecting {}",
            device_name(&state.snapshot, address)
        )),
        BluetoothActiveAction::Disconnect { address } => Some(format!(
            "Disconnecting {}",
            device_name(&state.snapshot, address)
        )),
        BluetoothActiveAction::Pair { address } => {
            Some(format!("Pairing {}", device_name(&state.snapshot, address)))
        }
        BluetoothActiveAction::Trust { address, trusted } => {
            if *trusted {
                Some(format!(
                    "Trusting {}",
                    device_name(&state.snapshot, address)
                ))
            } else {
                Some(format!(
                    "Untrusting {}",
                    device_name(&state.snapshot, address)
                ))
            }
        }
        BluetoothActiveAction::Forget { address } => Some(format!(
            "Forgetting {}",
            device_name(&state.snapshot, address)
        )),
    }
}

fn device_name(snapshot: &BluetoothSnapshot, address: &str) -> String {
    snapshot
        .devices
        .iter()
        .find(|device| device.address == address)
        .map(|device| device.name.clone())
        .unwrap_or_else(|| address.to_owned())
}

fn looks_like_mac(value: &str) -> bool {
    let value = value.trim();
    if value.len() != 17 {
        return false;
    }

    let separator = if value.contains(':') {
        ':'
    } else if value.contains('-') {
        '-'
    } else {
        return false;
    };
    let parts = value.split(separator).collect::<Vec<_>>();
    parts.len() == 6
        && parts
            .iter()
            .all(|part| part.len() == 2 && part.chars().all(|char| char.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::bluetooth::{
        BluetoothAdapter, BluetoothDeviceType, BluetoothStatus,
    };

    fn device(address: &str, name: &str, connected: bool, paired: bool) -> BluetoothDevice {
        BluetoothDevice {
            path: format!("/org/bluez/hci0/dev_{}", address.replace(':', "_")),
            address: address.into(),
            alias: name.into(),
            name: name.into(),
            device_type: BluetoothDeviceType::Unknown,
            paired,
            connected,
            trusted: false,
            battery: None,
            rssi: Some(-30),
            class: 0,
            appearance: 0,
            adapter: "/org/bluez/hci0".into(),
        }
    }

    fn adapter(path: &str) -> BluetoothAdapter {
        BluetoothAdapter {
            path: path.into(),
            name: String::new(),
            address: String::new(),
            powered: false,
            discovering: false,
            discoverable: false,
            pairable: false,
            address_type: String::new(),
            class: 0,
            discoverable_timeout: 0,
            pairable_timeout: 0,
            modalias: String::new(),
            roles: Vec::new(),
            uuids: Vec::new(),
        }
    }

    #[test]
    fn primary_device_command_matches_device_state() {
        assert_eq!(
            primary_device_command(
                &device("AA:BB", "Headphones", true, true),
                DeviceSection::Connected
            ),
            Command::Disconnect {
                address: "AA:BB".into()
            }
        );
        assert_eq!(
            primary_device_command(
                &device("AA:BB", "Headphones", false, true),
                DeviceSection::Paired
            ),
            Command::Connect {
                address: "AA:BB".into()
            }
        );
        assert_eq!(
            primary_device_command(
                &device("AA:BB", "Headphones", false, false),
                DeviceSection::Nearby
            ),
            Command::Pair {
                address: "AA:BB".into()
            }
        );
    }

    #[test]
    fn trusted_paired_device_primary_command_connects() {
        let mut device = device("AA:BB:CC:DD:EE:01", "Keyboard", false, false);
        device.trusted = true;

        assert_eq!(
            primary_device_command(&device, DeviceSection::Paired),
            Command::Connect {
                address: "AA:BB:CC:DD:EE:01".into()
            }
        );
    }

    #[test]
    fn hero_subtitle_prefers_health_then_activity_then_status() {
        let mut state = State {
            health: BluetoothServiceHealth::Ready,
            snapshot: BluetoothSnapshot {
                status: BluetoothStatus {
                    powered: true,
                    discovering: true,
                    connected_count: 0,
                },
                devices: vec![],
                adapters: vec![],
            },
            active_action: None,
        };

        assert_eq!(hero_subtitle_text(&state), "Discovering");

        state.active_action = Some(BluetoothActiveAction::SetPowered(false));
        assert_eq!(hero_subtitle_text(&state), "Turning Bluetooth off");

        state.health = BluetoothServiceHealth::Reconnecting { attempt: 2 };
        assert_eq!(hero_subtitle_text(&state), "Reconnecting");
    }

    #[test]
    fn device_sections_hide_raw_uninteresting_addresses() {
        let state = State {
            health: BluetoothServiceHealth::Ready,
            snapshot: BluetoothSnapshot {
                status: BluetoothStatus::default(),
                adapters: vec![],
                devices: vec![
                    device("AA:BB:CC:DD:EE:01", "AA:BB:CC:DD:EE:01", false, false),
                    device("AA:BB:CC:DD:EE:02", "Mouse", false, false),
                ],
            },
            active_action: None,
        };

        let sections = device_sections(&state);

        assert!(sections.connected.is_empty());
        assert!(sections.paired.is_empty());
        assert_eq!(sections.nearby.len(), 1);
        assert_eq!(sections.nearby[0].label, "Mouse");
    }

    #[test]
    fn device_sections_split_connected_paired_and_nearby() {
        let connected = device("AA:BB:CC:DD:EE:00", "Speaker", true, true);
        let paired = device("AA:BB:CC:DD:EE:01", "Headphones", false, true);
        let nearby = device("AA:BB:CC:DD:EE:02", "Keyboard", false, false);
        let state = State {
            health: BluetoothServiceHealth::Ready,
            snapshot: BluetoothSnapshot {
                status: BluetoothStatus::default(),
                adapters: vec![],
                devices: vec![nearby, paired, connected],
            },
            active_action: None,
        };

        let sections = device_sections(&state);

        assert_eq!(sections.connected.len(), 1);
        assert_eq!(sections.connected[0].label, "Speaker");
        assert_eq!(sections.paired.len(), 1);
        assert_eq!(sections.paired[0].label, "Headphones");
        assert_eq!(sections.nearby.len(), 1);
        assert_eq!(sections.nearby[0].label, "Keyboard");
    }

    #[test]
    fn device_row_status_is_battery_percentage_when_available() {
        let mut device = device("AA:BB:CC:DD:EE:02", "Mouse", true, true);
        device.battery = Some(75);
        let state = State {
            health: BluetoothServiceHealth::Ready,
            snapshot: BluetoothSnapshot {
                status: BluetoothStatus::default(),
                adapters: vec![],
                devices: vec![device],
            },
            active_action: None,
        };

        let sections = device_sections(&state);

        assert_eq!(sections.connected[0].status.as_deref(), Some("75%"));
        assert!(!sections.connected[0].busy);
        assert!(sections.connected[0].active);
    }

    #[test]
    fn connecting_device_row_sets_busy_status() {
        let device = device("AA:BB:CC:DD:EE:02", "Mouse", false, true);
        let state = State {
            health: BluetoothServiceHealth::Ready,
            snapshot: BluetoothSnapshot {
                status: BluetoothStatus::default(),
                adapters: vec![],
                devices: vec![device],
            },
            active_action: Some(BluetoothActiveAction::Connect {
                address: "AA:BB:CC:DD:EE:02".into(),
            }),
        };

        let sections = device_sections(&state);

        assert!(sections.paired[0].busy);
    }

    #[test]
    fn pairing_device_row_sets_busy_status() {
        let device = device("AA:BB:CC:DD:EE:02", "Mouse", false, false);
        let state = State {
            health: BluetoothServiceHealth::Ready,
            snapshot: BluetoothSnapshot {
                status: BluetoothStatus::default(),
                adapters: vec![],
                devices: vec![device],
            },
            active_action: Some(BluetoothActiveAction::Pair {
                address: "AA:BB:CC:DD:EE:02".into(),
            }),
        };

        let sections = device_sections(&state);

        assert!(sections.nearby[0].busy);
    }

    #[test]
    fn connected_device_exposes_expanded_actions() {
        let device = device("AA:BB:CC:DD:EE:02", "Mouse", true, true);
        let actions = device_actions(&device, DeviceSection::Connected);

        assert_eq!(actions[0].id, "trust");
        assert_eq!(actions[1].id, "forget");
        assert!(actions[1].destructive);
    }

    #[test]
    fn paired_and_nearby_rows_have_no_expanded_payload() {
        let state = State {
            health: BluetoothServiceHealth::Ready,
            snapshot: BluetoothSnapshot {
                status: BluetoothStatus::default(),
                adapters: vec![],
                devices: vec![
                    device("AA:BB:CC:DD:EE:01", "Mouse", false, true),
                    device("AA:BB:CC:DD:EE:02", "Keyboard", false, false),
                ],
            },
            active_action: None,
        };

        let sections = device_sections(&state);

        assert!(sections.paired[0].details.is_empty());
        assert!(sections.paired[0].actions.is_empty());
        assert!(sections.nearby[0].details.is_empty());
        assert!(sections.nearby[0].actions.is_empty());
    }

    #[test]
    fn discoverable_command_targets_powered_adapter() {
        let state = State {
            health: BluetoothServiceHealth::Ready,
            snapshot: BluetoothSnapshot {
                status: BluetoothStatus::default(),
                adapters: vec![
                    BluetoothAdapter {
                        path: "/org/bluez/hci0".into(),
                        powered: false,
                        ..adapter("/org/bluez/hci0")
                    },
                    BluetoothAdapter {
                        path: "/org/bluez/hci1".into(),
                        powered: true,
                        ..adapter("/org/bluez/hci1")
                    },
                ],
                devices: vec![],
            },
            active_action: None,
        };

        assert_eq!(
            discoverable_command(&state, true),
            Some(Command::SetAdapterDiscoverable {
                adapter_path: "/org/bluez/hci1".into(),
                discoverable: true,
            })
        );
    }

    #[test]
    fn pending_discoverable_keeps_optimistic_value_until_snapshot_matches() {
        let mut pending = Some(PendingDiscoverable::new(true, 7));

        assert!(reconcile_pending_discoverable(&mut pending, false));
        assert_eq!(
            pending,
            Some(PendingDiscoverable {
                value: true,
                generation: 7,
            })
        );

        assert!(reconcile_pending_discoverable(&mut pending, false));
        assert_eq!(
            pending,
            Some(PendingDiscoverable {
                value: true,
                generation: 7,
            })
        );

        assert!(reconcile_pending_discoverable(&mut pending, true));
        assert_eq!(pending, None);
    }

    #[test]
    fn pending_discoverable_expires_only_matching_generation() {
        let mut pending = Some(PendingDiscoverable::new(true, 7));

        assert!(!expire_pending_discoverable(&mut pending, 6));
        assert_eq!(pending, Some(PendingDiscoverable::new(true, 7)));

        assert!(expire_pending_discoverable(&mut pending, 7));
        assert_eq!(pending, None);
    }

    #[test]
    fn expired_pending_discoverable_reveals_actual_snapshot_value() {
        let mut pending = Some(PendingDiscoverable::new(true, 7));

        assert!(reconcile_pending_discoverable(&mut pending, false));
        assert!(expire_pending_discoverable(&mut pending, 7));

        assert_eq!(pending, None);
        assert!(!reconcile_pending_discoverable(&mut pending, false));
    }
}
