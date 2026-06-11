use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    services::network::{
        Command, NetworkActiveAction, NetworkConnection, NetworkDevice, NetworkSnapshot, SavedVpn,
        State, WifiAccessPoint,
    },
    utils::popover_scroll,
    widgets::{
        animated_popover::AnimatedPopover, expander_tile::ExpanderTile, hero::Hero,
        key_value_grid::KeyValueGrid, popover_shell::PopoverShell, segmented_tile::SegmentedTile,
        tile::Tile,
    },
};

use super::format;

const OTHER_SECTION_TITLE: &str = "Other networks";
const WIRED_SECTION_TITLE: &str = "Wired networks";
const VPN_SECTION_TITLE: &str = "VPN";

pub struct Popover {
    popover: AnimatedPopover,
    state: State,
    sections: NetworkSections,
    wifi_enabled: bool,
    wifi_toggle_sensitive: bool,
    other_expanded: bool,
    updating_wifi: Rc<Cell<bool>>,
    hero: Hero,
    connected_list: gtk::Box,
    connected_wifi_list: gtk::Box,
    connected_wired_list: gtk::Box,
    connected_vpn_list: gtk::Box,
    wired_section: ExpanderTile,
    other_list: gtk::Box,
    wired_list: gtk::Box,
    vpn_section: ExpanderTile,
    vpn_list: gtk::Box,
    connected_wifi_rows: HashMap<String, SegmentedCommandRow>,
    connected_wired_rows: HashMap<String, SegmentedCommandRow>,
    connected_vpn_rows: HashMap<String, SegmentedCommandRow>,
    other_rows: HashMap<String, SimpleCommandRow>,
    wired_rows: HashMap<String, StaticRow>,
    vpn_rows: HashMap<String, SimpleCommandRow>,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateState(State),
    SetWifiEnabled(bool),
    SetOtherExpanded(bool),
    RowCommand(Command),
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
                    set_icon: Some(icon_name_for_state(&model.state)),
                    set_title: "Network",
                    #[watch]
                    set_subtitle: &format::hero_subtitle(&model.state),
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
                            set_visible: model.sections.connected_visible() && model.sections.lower_than_connected_visible(),
                        },

                        #[name = "other_section"]
                        ExpanderTile {
                            add_css_class: "network-other-section",
                            set_primary: OTHER_SECTION_TITLE,
                            set_secondary: None,
                            #[watch]
                            set_visible: model.sections.other_visible(),
                            #[watch]
                            set_expanded: model.other_expanded,

                            connect_expanded[sender] => move |_, expanded| {
                                sender.input(PopoverInput::SetOtherExpanded(expanded));
                            },
                        },

                        #[name = "wired_section"]
                        ExpanderTile {
                            add_css_class: "network-wired-section",
                            set_primary: WIRED_SECTION_TITLE,
                            set_secondary: None,
                            set_expanded: false,
                            #[watch]
                            set_visible: model.sections.wired_visible(),
                        },

                        #[name = "vpn_section"]
                        ExpanderTile {
                            add_css_class: "network-vpn-section",
                            set_primary: VPN_SECTION_TITLE,
                            set_secondary: None,
                            set_expanded: false,
                            #[watch]
                            set_visible: model.sections.vpn_visible(),
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
        let state = State::default();
        let sections = network_sections(&state);
        let updating_wifi = Rc::new(Cell::new(false));
        let mut model = Popover {
            popover: AnimatedPopover::new(),
            state,
            sections,
            wifi_enabled: false,
            wifi_toggle_sensitive: false,
            other_expanded: false,
            updating_wifi,
            hero: Hero::new(),
            connected_list: gtk::Box::new(gtk::Orientation::Vertical, 2),
            connected_wifi_list: gtk::Box::new(gtk::Orientation::Vertical, 2),
            connected_wired_list: gtk::Box::new(gtk::Orientation::Vertical, 2),
            connected_vpn_list: gtk::Box::new(gtk::Orientation::Vertical, 2),
            wired_section: ExpanderTile::new(),
            other_list: gtk::Box::new(gtk::Orientation::Vertical, 2),
            wired_list: gtk::Box::new(gtk::Orientation::Vertical, 2),
            vpn_section: ExpanderTile::new(),
            vpn_list: gtk::Box::new(gtk::Orientation::Vertical, 2),
            connected_wifi_rows: HashMap::new(),
            connected_wired_rows: HashMap::new(),
            connected_vpn_rows: HashMap::new(),
            other_rows: HashMap::new(),
            wired_rows: HashMap::new(),
            vpn_rows: HashMap::new(),
        };

        let widgets = view_output!();
        model.popover = widgets.root.clone();
        model.hero = widgets.hero.clone();
        model.connected_list = widgets.connected_list.clone();
        model.wired_section = widgets.wired_section.clone();
        model.vpn_section = widgets.vpn_section.clone();

        widgets.root.set_parent(&init.parent);
        popover_scroll::install_half_monitor_limit(
            widgets.root.upcast_ref(),
            &widgets.scroller,
            &init.parent,
        );

        widgets.hero.set_trailing_visible(true);
        widgets.hero.connect_toggled({
            let guard = model.updating_wifi.clone();
            let sender = sender.clone();
            move |_, active| {
                if !guard.get() {
                    sender.input(PopoverInput::SetWifiEnabled(active));
                }
            }
        });

        model.connected_list.append(&model.connected_wifi_list);
        model.connected_list.append(&model.connected_wired_list);
        model.connected_list.append(&model.connected_vpn_list);

        let other_scroller = gtk::ScrolledWindow::new();
        other_scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        other_scroller.set_vexpand(false);
        other_scroller.set_propagate_natural_height(true);
        other_scroller.set_min_content_height(80);
        other_scroller.set_max_content_height(220);
        other_scroller.set_child(Some(&model.other_list));
        widgets.other_section.set_child(Some(other_scroller));
        widgets
            .wired_section
            .set_child(Some(model.wired_list.clone()));
        widgets.vpn_section.set_child(Some(model.vpn_list.clone()));

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
                self.wifi_enabled = state.snapshot.status.wifi_enabled;
                self.wifi_toggle_sensitive =
                    state.snapshot.status.enabled && state.snapshot.status.wifi_hw_enabled;
                self.sections = network_sections(&state);
                self.state = state;
                self.sync_toggles();
                self.sync_rows(&sender);
            }
            PopoverInput::SetWifiEnabled(enabled) => {
                let _ = sender.output(PopoverOutput::Command(Command::SetWifiEnabled(enabled)));
            }
            PopoverInput::SetOtherExpanded(expanded) => {
                self.other_expanded = expanded;
            }
            PopoverInput::RowCommand(command) => {
                let _ = sender.output(PopoverOutput::Command(command));
            }
        }
    }
}

impl Popover {
    fn sync_toggles(&self) {
        self.updating_wifi.set(true);
        self.hero.set_toggle_active(self.wifi_enabled);
        self.hero.set_toggle_sensitive(self.wifi_toggle_sensitive);
        self.updating_wifi.set(false);
    }

    fn sync_rows(&mut self, sender: &ComponentSender<Self>) {
        self.connected_wifi_list
            .set_visible(self.sections.connected_wifi_visible());
        self.connected_wired_list
            .set_visible(self.sections.connected_wired_visible());
        self.connected_vpn_list
            .set_visible(self.sections.connected_vpn_visible());

        sync_segmented_command_rows(
            &mut self.connected_wifi_rows,
            &self.connected_wifi_list,
            self.sections.connected_wifi.clone(),
            sender,
            "network-wifi-row",
        );
        sync_segmented_command_rows(
            &mut self.connected_wired_rows,
            &self.connected_wired_list,
            self.sections.connected_wired.clone(),
            sender,
            "network-wired-row",
        );
        sync_segmented_command_rows(
            &mut self.connected_vpn_rows,
            &self.connected_vpn_list,
            self.sections.connected_vpn.clone(),
            sender,
            "network-vpn-row",
        );
        sync_simple_command_rows(
            &mut self.other_rows,
            &self.other_list,
            self.sections.other_wifi.clone(),
            sender,
            "network-wifi-row",
        );
        sync_static_rows(
            &mut self.wired_rows,
            &self.wired_list,
            self.sections.wired.clone(),
            "network-wired-row",
        );
        sync_simple_command_rows(
            &mut self.vpn_rows,
            &self.vpn_list,
            self.sections.vpn.clone(),
            sender,
            "network-vpn-row",
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct NetworkSections {
    connected_wifi: Vec<CommandRowModel>,
    connected_wired: Vec<CommandRowModel>,
    connected_vpn: Vec<CommandRowModel>,
    other_wifi: Vec<CommandRowModel>,
    wired: Vec<StaticRowModel>,
    vpn: Vec<CommandRowModel>,
}

impl NetworkSections {
    fn connected_wifi_visible(&self) -> bool {
        !self.connected_wifi.is_empty()
    }

    fn connected_wired_visible(&self) -> bool {
        !self.connected_wired.is_empty()
    }

    fn connected_vpn_visible(&self) -> bool {
        !self.connected_vpn.is_empty()
    }

    fn connected_visible(&self) -> bool {
        self.connected_wifi_visible()
            || self.connected_wired_visible()
            || self.connected_vpn_visible()
    }

    fn other_visible(&self) -> bool {
        !self.other_wifi.is_empty()
    }

    fn wired_visible(&self) -> bool {
        !self.wired.is_empty()
    }

    fn vpn_visible(&self) -> bool {
        !self.vpn.is_empty()
    }

    fn lower_than_connected_visible(&self) -> bool {
        self.other_visible() || self.lower_than_wifi_visible()
    }

    fn lower_than_wifi_visible(&self) -> bool {
        self.wired_visible() || self.vpn_visible()
    }

    #[cfg(test)]
    fn named_section_titles(&self) -> Vec<&'static str> {
        let mut titles = Vec::new();
        if self.other_visible() {
            titles.push(OTHER_SECTION_TITLE);
        }
        if self.wired_visible() {
            titles.push(WIRED_SECTION_TITLE);
        }
        if self.vpn_visible() {
            titles.push(VPN_SECTION_TITLE);
        }
        titles
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandRowModel {
    id: String,
    label: String,
    icon: String,
    status: Option<String>,
    tooltip: String,
    busy: bool,
    activatable: bool,
    active: bool,
    command: Command,
    details: Vec<DetailRow>,
    ip4_addresses: Vec<String>,
    ip6_addresses: Vec<String>,
    connection_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StaticRowModel {
    id: String,
    label: String,
    icon: String,
    status: Option<String>,
    tooltip: String,
    active: bool,
    disconnect_uuid: Option<String>,
    ip4_addresses: Vec<String>,
    ip6_addresses: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DetailRow {
    key: &'static str,
    value: String,
}

fn network_sections(state: &State) -> NetworkSections {
    let wired = wired_rows(&state.snapshot);
    let vpn = vpn_rows(state);
    NetworkSections {
        connected_wifi: connected_wifi_rows(state),
        connected_wired: connected_wired_rows(&wired, state.active_action.as_ref()),
        connected_vpn: connected_vpn_rows(&vpn),
        other_wifi: other_wifi_rows(state),
        wired,
        vpn,
    }
}

fn connected_wifi_rows(state: &State) -> Vec<CommandRowModel> {
    if !state.snapshot.status.wifi_enabled {
        return Vec::new();
    }

    state
        .snapshot
        .wifi_access_points
        .iter()
        .filter(|access_point| {
            is_visible_access_point(access_point)
                && access_point.connected
                && access_point.uuid.is_some()
        })
        .map(|access_point| {
            wifi_row_model(
                access_point,
                true,
                state.active_action.as_ref(),
                device_interface(&state.snapshot, &access_point.device_path).as_deref(),
            )
        })
        .collect()
}

fn other_wifi_rows(state: &State) -> Vec<CommandRowModel> {
    if !state.snapshot.status.wifi_enabled {
        return Vec::new();
    }

    state
        .snapshot
        .wifi_access_points
        .iter()
        .filter(|access_point| is_visible_access_point(access_point) && !access_point.connected)
        .map(|access_point| wifi_row_model(access_point, false, state.active_action.as_ref(), None))
        .collect()
}

fn wifi_row_model(
    access_point: &WifiAccessPoint,
    connected: bool,
    active_action: Option<&NetworkActiveAction>,
    device_interface: Option<&str>,
) -> CommandRowModel {
    CommandRowModel {
        id: wifi_item_id(access_point),
        label: access_point.ssid.clone(),
        icon: format::wifi_icon(access_point.strength).into(),
        status: connected.then(|| format::wifi_status(access_point)),
        tooltip: access_point_tooltip(access_point),
        busy: is_wifi_busy(active_action, access_point),
        activatable: true,
        active: access_point.connected,
        command: primary_wifi_command(access_point),
        ip4_addresses: access_point.ip4_addresses.clone(),
        ip6_addresses: access_point.ip6_addresses.clone(),
        connection_type: None,
        details: connected
            .then(|| wifi_details(access_point, device_interface))
            .unwrap_or_default(),
    }
}

fn wired_rows(snapshot: &NetworkSnapshot) -> Vec<StaticRowModel> {
    let ethernet_connections: Vec<_> = snapshot
        .connections
        .iter()
        .filter(|connection| connection.connection_type == "ethernet")
        .collect();
    let mut rows: Vec<_> = snapshot
        .devices
        .iter()
        .filter(|device| device.device_type == "ethernet")
        .map(|device| {
            let connection = ethernet_connections
                .iter()
                .find(|connection| {
                    connection.device_path == device.path || connection.device == device.interface
                })
                .copied();
            StaticRowModel {
                id: wired_key(device).to_owned(),
                label: device.interface.clone(),
                icon: "network-wired-symbolic".into(),
                status: Some(wired_status(device)),
                tooltip: wired_tooltip(device),
                active: device.state == "connected",
                disconnect_uuid: connection
                    .filter(|connection| connection.state == "activated")
                    .map(|connection| connection.uuid.clone()),
                ip4_addresses: if device.ip4_addresses.is_empty() {
                    connection
                        .map(|connection| connection.ip4_addresses.clone())
                        .unwrap_or_default()
                } else {
                    device.ip4_addresses.clone()
                },
                ip6_addresses: if device.ip6_addresses.is_empty() {
                    connection
                        .map(|connection| connection.ip6_addresses.clone())
                        .unwrap_or_default()
                } else {
                    device.ip6_addresses.clone()
                },
            }
        })
        .collect();

    let mut seen: HashSet<_> = rows.iter().map(|row| row.id.clone()).collect();
    for connection in snapshot
        .connections
        .iter()
        .filter(|connection| connection.connection_type == "ethernet")
    {
        let id = wired_connection_key(connection).to_owned();
        if !seen.insert(id.clone()) {
            continue;
        }
        rows.push(StaticRowModel {
            id,
            label: wired_connection_label(connection),
            icon: "network-wired-symbolic".into(),
            status: Some(wired_connection_status(connection)),
            tooltip: wired_connection_tooltip(connection),
            active: connection.state == "activated",
            disconnect_uuid: (connection.state == "activated").then(|| connection.uuid.clone()),
            ip4_addresses: connection.ip4_addresses.clone(),
            ip6_addresses: connection.ip6_addresses.clone(),
        });
    }

    rows
}

fn connected_wired_rows(
    wired: &[StaticRowModel],
    active_action: Option<&NetworkActiveAction>,
) -> Vec<CommandRowModel> {
    wired
        .iter()
        .filter(|row| row.active && row.disconnect_uuid.is_some())
        .map(|row| connected_wired_row_model(row, active_action))
        .collect()
}

fn vpn_rows(state: &State) -> Vec<CommandRowModel> {
    state
        .snapshot
        .saved_vpns
        .iter()
        .filter(|vpn| !vpn.id.is_empty() && !vpn.uuid.is_empty())
        .map(|vpn| CommandRowModel {
            id: vpn.uuid.clone(),
            label: vpn.id.clone(),
            icon: "network-vpn-symbolic".into(),
            status: Some(vpn_status(vpn)),
            tooltip: vpn_tooltip(vpn),
            busy: is_vpn_busy(state.active_action.as_ref(), vpn),
            activatable: true,
            active: vpn.active,
            command: primary_vpn_command(vpn),
            ip4_addresses: vpn.ip4_addresses.clone(),
            ip6_addresses: vpn.ip6_addresses.clone(),
            connection_type: (!vpn.connection_type.is_empty()).then(|| vpn.connection_type.clone()),
            details: Vec::new(),
        })
        .collect()
}

fn connected_vpn_rows(vpn: &[CommandRowModel]) -> Vec<CommandRowModel> {
    vpn.iter()
        .filter(|row| row.active)
        .map(|row| {
            let mut row = row.clone();
            row.status = None;
            row.details = vpn_details(&row);
            row
        })
        .collect()
}

fn connected_wired_row_model(
    row: &StaticRowModel,
    active_action: Option<&NetworkActiveAction>,
) -> CommandRowModel {
    let uuid = row
        .disconnect_uuid
        .clone()
        .expect("connected wired rows require an active connection uuid");
    CommandRowModel {
        id: row.id.clone(),
        label: row.label.clone(),
        icon: row.icon.clone(),
        status: connected_row_status(row.status.as_ref()),
        tooltip: row.tooltip.clone(),
        busy: is_uuid_busy(active_action, &uuid),
        activatable: true,
        active: row.active,
        command: Command::Disconnect { uuid },
        ip4_addresses: row.ip4_addresses.clone(),
        ip6_addresses: row.ip6_addresses.clone(),
        connection_type: None,
        details: wired_details(row),
    }
}

fn primary_wifi_command(access_point: &WifiAccessPoint) -> Command {
    if access_point.connected {
        if let Some(uuid) = &access_point.uuid {
            return Command::Disconnect { uuid: uuid.clone() };
        }
    }

    if access_point.saved {
        if let Some(uuid) = &access_point.uuid {
            return Command::ConnectSaved { uuid: uuid.clone() };
        }
    }

    Command::ConnectWifi {
        ssid: access_point.ssid.clone(),
        path: access_point.path.clone(),
    }
}

fn primary_vpn_command(vpn: &SavedVpn) -> Command {
    if vpn.active {
        Command::Disconnect {
            uuid: vpn.uuid.clone(),
        }
    } else {
        Command::ConnectSaved {
            uuid: vpn.uuid.clone(),
        }
    }
}

fn wifi_details(access_point: &WifiAccessPoint, device_interface: Option<&str>) -> Vec<DetailRow> {
    let mut rows = vec![detail("Signal", format::wifi_status(access_point))];
    if let Some(device_interface) = device_interface.filter(|value| !value.is_empty()) {
        rows.push(detail("Interface", device_interface));
    }
    add_ip_details(
        &mut rows,
        &access_point.ip4_addresses,
        &access_point.ip6_addresses,
    );
    if !access_point.security.is_empty() {
        rows.push(detail("Security", security_label(&access_point.security)));
    }
    if access_point.frequency > 0 {
        rows.push(detail("Frequency", frequency_text(access_point.frequency)));
    }
    rows.push(detail(
        "Profile",
        if access_point.saved {
            "Saved"
        } else {
            "Unsaved"
        },
    ));
    rows
}

fn wired_details(row: &StaticRowModel) -> Vec<DetailRow> {
    let mut rows = Vec::new();
    if row
        .status
        .as_deref()
        .is_some_and(|status| status.ends_with(" Mbps"))
    {
        rows.push(detail("Speed", row.status.clone().unwrap()));
    }
    rows.push(detail("Interface", row.label.clone()));
    add_ip_details(&mut rows, &row.ip4_addresses, &row.ip6_addresses);
    rows
}

fn vpn_details(row: &CommandRowModel) -> Vec<DetailRow> {
    let mut rows = Vec::new();
    if let Some(connection_type) = row.connection_type.as_deref() {
        rows.push(detail("Type", connection_type_label(connection_type)));
    }
    rows.push(detail("Profile", row.label.clone()));
    add_ip_details(&mut rows, &row.ip4_addresses, &row.ip6_addresses);
    rows
}

fn add_ip_details(rows: &mut Vec<DetailRow>, ip4_addresses: &[String], ip6_addresses: &[String]) {
    if !ip4_addresses.is_empty() {
        rows.push(detail(
            "IPv4",
            display_ip_addresses(ip4_addresses).join(", "),
        ));
    }
    if !ip6_addresses.is_empty() {
        rows.push(detail(
            "IPv6",
            display_ip_addresses(ip6_addresses).join(", "),
        ));
    }
}

fn display_ip_addresses(addresses: &[String]) -> Vec<String> {
    addresses
        .iter()
        .map(|address| display_ip_address(address))
        .collect()
}

fn display_ip_address(address: &str) -> String {
    address
        .rsplit_once('/')
        .filter(|(_, prefix)| prefix.chars().all(|ch| ch.is_ascii_digit()))
        .map(|(address, _)| address)
        .unwrap_or(address)
        .into()
}

fn connection_type_label(connection_type: &str) -> String {
    match connection_type {
        "wireguard" => "WireGuard".into(),
        "vpn" => "VPN".into(),
        _ => connection_type.into(),
    }
}

fn detail(key: &'static str, value: impl Into<String>) -> DetailRow {
    DetailRow {
        key,
        value: value.into(),
    }
}

fn is_wifi_busy(
    active_action: Option<&NetworkActiveAction>,
    access_point: &WifiAccessPoint,
) -> bool {
    match active_action {
        Some(NetworkActiveAction::ConnectWifi { path, .. }) => path == &access_point.path,
        Some(NetworkActiveAction::ConnectSaved { uuid })
        | Some(NetworkActiveAction::Disconnect { uuid })
        | Some(NetworkActiveAction::Forget { uuid }) => access_point.uuid.as_deref() == Some(uuid),
        Some(NetworkActiveAction::SetWifiEnabled(_)) | None => false,
    }
}

fn is_vpn_busy(active_action: Option<&NetworkActiveAction>, vpn: &SavedVpn) -> bool {
    is_uuid_busy(active_action, &vpn.uuid)
}

fn is_uuid_busy(active_action: Option<&NetworkActiveAction>, target_uuid: &str) -> bool {
    match active_action {
        Some(NetworkActiveAction::ConnectSaved { uuid })
        | Some(NetworkActiveAction::Disconnect { uuid })
        | Some(NetworkActiveAction::Forget { uuid }) => uuid == target_uuid,
        Some(NetworkActiveAction::SetWifiEnabled(_))
        | Some(NetworkActiveAction::ConnectWifi { .. })
        | None => false,
    }
}

fn is_visible_access_point(access_point: &WifiAccessPoint) -> bool {
    !access_point.ssid.is_empty()
}

fn wifi_item_id(access_point: &WifiAccessPoint) -> String {
    access_point.path.clone()
}

fn access_point_tooltip(access_point: &WifiAccessPoint) -> String {
    let mut parts = Vec::new();
    if access_point.connected {
        parts.push("Connected".into());
    } else if access_point.saved {
        parts.push("Saved".into());
    }
    if !access_point.security.is_empty() {
        parts.push(security_label(&access_point.security));
    }
    if access_point.frequency > 0 {
        parts.push(frequency_text(access_point.frequency));
    }
    parts.push(format::wifi_status(access_point));
    parts.join(" - ")
}

fn wired_key(device: &NetworkDevice) -> &str {
    if device.path.is_empty() {
        &device.interface
    } else {
        &device.path
    }
}

fn wired_connection_key(connection: &NetworkConnection) -> &str {
    if !connection.device_path.is_empty() {
        &connection.device_path
    } else if !connection.device.is_empty() {
        &connection.device
    } else if !connection.uuid.is_empty() {
        &connection.uuid
    } else {
        &connection.active_path
    }
}

fn wired_connection_label(connection: &NetworkConnection) -> String {
    if !connection.device.is_empty() {
        connection.device.clone()
    } else {
        connection.id.clone()
    }
}

fn wired_status(device: &NetworkDevice) -> String {
    if device.state == "connected" {
        if device.speed > 0 {
            format!("{} Mbps", device.speed)
        } else {
            "Connected".into()
        }
    } else if device.carrier.unwrap_or(false) {
        "Cable connected".into()
    } else {
        "Disconnected".into()
    }
}

fn wired_connection_status(connection: &NetworkConnection) -> String {
    if connection.state == "activated" {
        if connection.speed > 0 {
            format!("{} Mbps", connection.speed)
        } else {
            "Connected".into()
        }
    } else {
        connection.state.clone()
    }
}

fn connected_row_status(status: Option<&String>) -> Option<String> {
    status
        .filter(|status| status.as_str() != "Connected")
        .cloned()
}

fn device_interface(snapshot: &NetworkSnapshot, device_path: &str) -> Option<String> {
    snapshot
        .devices
        .iter()
        .find(|device| device.path == device_path)
        .map(|device| device.interface.clone())
        .filter(|interface| !interface.is_empty())
}

fn wired_tooltip(device: &NetworkDevice) -> String {
    if device
        .driver
        .as_deref()
        .is_some_and(|driver| !driver.is_empty())
    {
        format!(
            "{} - {}",
            device.interface,
            device.driver.as_deref().unwrap()
        )
    } else {
        device.interface.clone()
    }
}

fn wired_connection_tooltip(connection: &NetworkConnection) -> String {
    if !connection.id.is_empty() && !connection.device.is_empty() {
        format!("{} - {}", connection.device, connection.id)
    } else if !connection.device.is_empty() {
        connection.device.clone()
    } else {
        connection.id.clone()
    }
}

fn vpn_status(vpn: &SavedVpn) -> String {
    if vpn.active {
        "Connected".into()
    } else {
        vpn.state.clone().unwrap_or_else(|| "Off".into())
    }
}

fn vpn_tooltip(vpn: &SavedVpn) -> String {
    if vpn.connection_type.is_empty() {
        vpn.id.clone()
    } else {
        format!("{} - {}", vpn.id, vpn.connection_type)
    }
}

fn security_label(security: &str) -> String {
    if security.eq_ignore_ascii_case("open") {
        "Open".into()
    } else {
        security.to_uppercase()
    }
}

fn frequency_text(frequency_mhz: u32) -> String {
    if frequency_mhz >= 1000 {
        format!("{:.1} GHz", frequency_mhz as f32 / 1000.0)
    } else {
        format!("{frequency_mhz} MHz")
    }
}

struct SegmentedCommandRow {
    root: SegmentedTile,
    icon: gtk::Image,
    status: gtk::Label,
    command: Rc<RefCell<Command>>,
    details: KeyValueGrid,
}

impl SegmentedCommandRow {
    fn new(model: &CommandRowModel, sender: &ComponentSender<Popover>, css_class: &str) -> Self {
        let root = SegmentedTile::new();
        root.add_css_class(css_class);

        let icon = gtk::Image::from_icon_name(&model.icon);
        icon.set_pixel_size(16);
        root.set_left(Some(icon.clone()));
        root.set_secondary(None);

        let status = gtk::Label::new(None);
        status.add_css_class("dim-label");
        status.add_css_class("caption");
        status.add_css_class("numeric");
        status.set_valign(gtk::Align::Center);

        let details = KeyValueGrid::new();
        root.set_child(Some(details.clone()));

        let command = Rc::new(RefCell::new(model.command.clone()));
        root.connect_activated({
            let sender = sender.clone();
            let command = command.clone();
            move |_| sender.input(PopoverInput::RowCommand(command.borrow().clone()))
        });

        let row = Self {
            root,
            icon,
            status,
            command,
            details,
        };
        row.update(model);
        row
    }

    fn update(&self, model: &CommandRowModel) {
        self.root.set_primary(&model.label);
        self.root.set_tooltip_text(Some(&model.tooltip));
        self.icon.set_icon_name(Some(&model.icon));
        self.command.replace(model.command.clone());
        self.root.set_activatable(!model.busy && model.activatable);
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
    }

    fn widget(&self) -> &gtk::Widget {
        self.root.upcast_ref()
    }
}

struct SimpleCommandRow {
    root: Tile,
    icon: gtk::Image,
    status: gtk::Label,
    command: Rc<RefCell<Command>>,
}

impl SimpleCommandRow {
    fn new(model: &CommandRowModel, sender: &ComponentSender<Popover>, css_class: &str) -> Self {
        let root = Tile::new();
        root.add_css_class(css_class);

        let icon = gtk::Image::from_icon_name(&model.icon);
        icon.set_pixel_size(16);
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
            move |_| sender.input(PopoverInput::RowCommand(command.borrow().clone()))
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

    fn update(&self, model: &CommandRowModel) {
        self.root.set_primary(&model.label);
        self.root.set_tooltip_text(Some(&model.tooltip));
        self.icon.set_icon_name(Some(&model.icon));
        self.command.replace(model.command.clone());
        self.root.set_activatable(!model.busy && model.activatable);
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

struct StaticRow {
    root: Tile,
    icon: gtk::Image,
    status: gtk::Label,
}

impl StaticRow {
    fn new(model: &StaticRowModel, css_class: &str) -> Self {
        let root = Tile::new();
        root.add_css_class(css_class);

        let icon = gtk::Image::from_icon_name(&model.icon);
        icon.set_pixel_size(16);
        root.set_left(Some(icon.clone()));
        root.set_activatable(false);

        let status = gtk::Label::new(None);
        status.add_css_class("dim-label");
        status.add_css_class("caption");
        status.add_css_class("numeric");
        status.set_valign(gtk::Align::Center);

        let row = Self { root, icon, status };
        row.update(model);
        row
    }

    fn update(&self, model: &StaticRowModel) {
        self.root.set_primary(&model.label);
        self.root.set_tooltip_text(Some(&model.tooltip));
        self.icon.set_icon_name(Some(&model.icon));
        self.root.set_sensitive(true);
        if model.active {
            self.root.add_css_class("active");
        } else {
            self.root.remove_css_class("active");
        }

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

fn sync_segmented_command_rows(
    rows: &mut HashMap<String, SegmentedCommandRow>,
    container: &gtk::Box,
    models: Vec<CommandRowModel>,
    sender: &ComponentSender<Popover>,
    css_class: &'static str,
) {
    let mut seen = HashSet::new();
    let mut previous: Option<gtk::Widget> = None;

    for model in models {
        seen.insert(model.id.clone());
        let row = rows
            .entry(model.id.clone())
            .or_insert_with(|| SegmentedCommandRow::new(&model, sender, css_class));
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

fn sync_simple_command_rows(
    rows: &mut HashMap<String, SimpleCommandRow>,
    container: &gtk::Box,
    models: Vec<CommandRowModel>,
    sender: &ComponentSender<Popover>,
    css_class: &'static str,
) {
    let mut seen = HashSet::new();
    let mut previous: Option<gtk::Widget> = None;

    for model in models {
        seen.insert(model.id.clone());
        let row = rows
            .entry(model.id.clone())
            .or_insert_with(|| SimpleCommandRow::new(&model, sender, css_class));
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

fn sync_static_rows(
    rows: &mut HashMap<String, StaticRow>,
    container: &gtk::Box,
    models: Vec<StaticRowModel>,
    css_class: &'static str,
) {
    let mut seen = HashSet::new();
    let mut previous: Option<gtk::Widget> = None;

    for model in models {
        seen.insert(model.id.clone());
        let row = rows
            .entry(model.id.clone())
            .or_insert_with(|| StaticRow::new(&model, css_class));
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

fn icon_name_for_state(state: &State) -> &str {
    if state.snapshot.status.icon.is_empty() {
        "network-offline-symbolic"
    } else {
        &state.snapshot.status.icon
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::network::{
        NetworkConnection, NetworkServiceHealth, NetworkStatus, SavedVpn, WifiAccessPoint,
    };

    #[test]
    fn hero_subtitle_prefers_health_then_activity_then_status() {
        let mut state = State {
            health: NetworkServiceHealth::Ready,
            snapshot: NetworkSnapshot {
                status: NetworkStatus {
                    enabled: true,
                    wifi_enabled: true,
                    wifi_hw_enabled: true,
                    primary_connection: "Home".into(),
                    ..NetworkStatus::default()
                },
                ..NetworkSnapshot::default()
            },
            ..State::default()
        };

        assert_eq!(format::hero_subtitle(&state), "Connected to Home");

        state.active_action = Some(NetworkActiveAction::SetWifiEnabled(false));
        assert_eq!(format::hero_subtitle(&state), "Turning Wi-Fi off");

        state.active_action = None;
        state.scanning = true;
        assert_eq!(format::hero_subtitle(&state), "Scanning");

        state.health = NetworkServiceHealth::Reconnecting { attempt: 2 };
        assert_eq!(format::hero_subtitle(&state), "Reconnecting");
    }

    fn access_point(ssid: &str, strength: u8, connected: bool, saved: bool) -> WifiAccessPoint {
        WifiAccessPoint {
            path: format!("/ap/{ssid}"),
            device_path: "/dev/wlan0".into(),
            ssid: ssid.into(),
            strength,
            frequency: 5200,
            security: "wpa2".into(),
            connected,
            saved,
            uuid: saved.then(|| format!("uuid-{ssid}")),
            ..WifiAccessPoint::default()
        }
    }

    #[test]
    fn network_sections_split_connected_and_other_wifi() {
        let state = State {
            health: NetworkServiceHealth::Ready,
            snapshot: NetworkSnapshot {
                status: NetworkStatus {
                    enabled: true,
                    wifi_enabled: true,
                    wifi_hw_enabled: true,
                    ..NetworkStatus::default()
                },
                devices: vec![NetworkDevice {
                    path: "/dev/wlan0".into(),
                    interface: "wlan0".into(),
                    device_type: "wifi".into(),
                    ..NetworkDevice::default()
                }],
                wifi_access_points: vec![
                    access_point("Cafe", 42, false, false),
                    WifiAccessPoint {
                        ip4_addresses: vec!["203.0.113.20/32".into()],
                        ..access_point("Home", 86, true, true)
                    },
                ],
                ..NetworkSnapshot::default()
            },
            ..State::default()
        };

        let sections = network_sections(&state);

        assert_eq!(sections.connected_wifi.len(), 1);
        assert_eq!(sections.connected_wifi[0].label, "Home");
        assert_eq!(sections.connected_wifi[0].status.as_deref(), Some("86%"));
        assert!(sections.connected_wifi[0].activatable);
        assert_eq!(
            sections.connected_wifi[0].command,
            Command::Disconnect {
                uuid: "uuid-Home".into()
            }
        );
        assert!(
            !sections.connected_wifi[0]
                .details
                .iter()
                .any(|row| row.value == "Connected")
        );
        assert_eq!(sections.connected_wifi[0].details[0].key, "Signal");
        assert_eq!(sections.connected_wifi[0].details[1].key, "Interface");
        assert_eq!(sections.connected_wifi[0].details[1].value, "wlan0");
        assert_eq!(sections.connected_wifi[0].details[2].key, "IPv4");
        assert_eq!(sections.connected_wifi[0].details[2].value, "203.0.113.20");

        assert_eq!(sections.other_wifi.len(), 1);
        assert_eq!(sections.other_wifi[0].label, "Cafe");
        assert_eq!(
            sections.other_wifi[0].command,
            Command::ConnectWifi {
                ssid: "Cafe".into(),
                path: "/ap/Cafe".into()
            }
        );
    }

    #[test]
    fn network_sections_expose_wired_and_vpn_tiles() {
        let state = State {
            health: NetworkServiceHealth::Ready,
            snapshot: NetworkSnapshot {
                devices: vec![
                    NetworkDevice {
                        path: "/dev/wlan0".into(),
                        interface: "wlan0".into(),
                        device_type: "wifi".into(),
                        ..NetworkDevice::default()
                    },
                    NetworkDevice {
                        path: "/dev/eth0".into(),
                        interface: "eth0".into(),
                        device_type: "ethernet".into(),
                        state: "connected".into(),
                        speed: 1000,
                        ip4_addresses: vec!["192.0.2.10/24".into()],
                        ..NetworkDevice::default()
                    },
                ],
                connections: vec![NetworkConnection {
                    active_path: "/active/wired".into(),
                    id: "Office Wired".into(),
                    uuid: "wired-1".into(),
                    connection_type: "ethernet".into(),
                    device_path: "/dev/eth0".into(),
                    device: "eth0".into(),
                    state: "activated".into(),
                    speed: 1000,
                    ..NetworkConnection::default()
                }],
                saved_vpns: vec![SavedVpn {
                    id: "Work".into(),
                    uuid: "vpn-1".into(),
                    connection_type: "wireguard".into(),
                    active: true,
                    ip6_addresses: vec!["2001:db8::10/128".into()],
                    ..SavedVpn::default()
                }],
                ..NetworkSnapshot::default()
            },
            ..State::default()
        };

        let sections = network_sections(&state);

        assert_eq!(sections.wired.len(), 1);
        assert_eq!(sections.wired[0].label, "eth0");
        assert_eq!(sections.wired[0].status.as_deref(), Some("1000 Mbps"));
        assert_eq!(sections.connected_wired.len(), 1);
        assert_eq!(sections.connected_wired[0].label, "eth0");
        assert!(sections.connected_wired[0].activatable);
        assert_eq!(
            sections.connected_wired[0].command,
            Command::Disconnect {
                uuid: "wired-1".into()
            }
        );
        assert_eq!(sections.connected_wired[0].details[0].key, "Speed");
        assert_eq!(sections.connected_wired[0].details[0].value, "1000 Mbps");
        assert_eq!(sections.connected_wired[0].details[1].key, "Interface");
        assert_eq!(sections.connected_wired[0].details[1].value, "eth0");
        assert_eq!(sections.connected_wired[0].details[2].key, "IPv4");
        assert_eq!(sections.connected_wired[0].details[2].value, "192.0.2.10");
        assert_eq!(sections.connected_vpn.len(), 1);
        assert_eq!(sections.connected_vpn[0].label, "Work");
        assert!(sections.connected_vpn[0].activatable);
        assert_eq!(
            sections.connected_vpn[0].command,
            Command::Disconnect {
                uuid: "vpn-1".into()
            }
        );
        assert_eq!(sections.connected_vpn[0].details[0].key, "Type");
        assert_eq!(sections.connected_vpn[0].details[0].value, "WireGuard");
        assert_eq!(sections.connected_vpn[0].details[1].key, "Profile");
        assert_eq!(sections.connected_vpn[0].details[1].value, "Work");
        assert_eq!(sections.connected_vpn[0].details[2].key, "IPv6");
        assert_eq!(sections.connected_vpn[0].details[2].value, "2001:db8::10");
        assert_eq!(sections.named_section_titles(), ["Wired networks", "VPN"]);
        assert_eq!(sections.vpn.len(), 1);
        assert_eq!(sections.vpn[0].label, "Work");
        assert_eq!(
            sections.vpn[0].command,
            Command::Disconnect {
                uuid: "vpn-1".into()
            }
        );
    }

    #[test]
    fn network_sections_expose_active_ethernet_connection_without_device_row() {
        let state = State {
            health: NetworkServiceHealth::Ready,
            snapshot: NetworkSnapshot {
                connections: vec![NetworkConnection {
                    active_path: "/active/wired".into(),
                    id: "Glimpse Test Wired".into(),
                    uuid: "wired-1".into(),
                    connection_type: "ethernet".into(),
                    device_path: "/dev/veth-glimpse".into(),
                    device: "veth-glimpse".into(),
                    state: "activated".into(),
                    speed: 10000,
                    ip4_addresses: vec!["198.51.100.2/32".into()],
                    ..NetworkConnection::default()
                }],
                ..NetworkSnapshot::default()
            },
            ..State::default()
        };

        let sections = network_sections(&state);

        assert_eq!(sections.wired.len(), 1);
        assert_eq!(sections.wired[0].label, "veth-glimpse");
        assert_eq!(sections.wired[0].status.as_deref(), Some("10000 Mbps"));
        assert_eq!(sections.connected_wired.len(), 1);
        assert_eq!(sections.connected_wired[0].label, "veth-glimpse");
        assert_eq!(
            sections.connected_wired[0].command,
            Command::Disconnect {
                uuid: "wired-1".into()
            }
        );
        assert_eq!(sections.connected_wired[0].details[0].key, "Speed");
        assert_eq!(sections.connected_wired[0].details[0].value, "10000 Mbps");
        assert_eq!(sections.connected_wired[0].details[2].key, "IPv4");
        assert_eq!(sections.connected_wired[0].details[2].value, "198.51.100.2");
        assert_eq!(sections.named_section_titles(), ["Wired networks"]);
    }

    #[test]
    fn connected_group_visibility_tracks_each_connected_group() {
        let sections = NetworkSections {
            connected_wired: vec![command_row("eth0")],
            ..NetworkSections::default()
        };

        assert!(!sections.connected_wifi_visible());
        assert!(sections.connected_wired_visible());
        assert!(!sections.connected_vpn_visible());
    }

    #[test]
    fn display_ip_address_hides_prefix_lengths() {
        assert_eq!(display_ip_address("192.0.2.10/32"), "192.0.2.10");
        assert_eq!(display_ip_address("192.0.2.10/24"), "192.0.2.10");
        assert_eq!(display_ip_address("2001:db8::10/128"), "2001:db8::10");
        assert_eq!(display_ip_address("2001:db8::10/64"), "2001:db8::10");
        assert_eq!(display_ip_address("192.0.2.10"), "192.0.2.10");
    }

    fn command_row(id: &str) -> CommandRowModel {
        CommandRowModel {
            id: id.into(),
            label: id.into(),
            icon: "network-wired-symbolic".into(),
            status: None,
            tooltip: id.into(),
            busy: false,
            activatable: true,
            active: true,
            command: Command::Disconnect { uuid: id.into() },
            details: Vec::new(),
            ip4_addresses: Vec::new(),
            ip6_addresses: Vec::new(),
            connection_type: None,
        }
    }
}
