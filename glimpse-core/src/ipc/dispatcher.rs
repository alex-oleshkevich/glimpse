use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use tokio::sync::{broadcast, watch};

use crate::services::framework::Services;

use crate::ipc::protocol::IpcEvent;

const BROADCAST_CAPACITY: usize = 256;

pub fn start(services: &Services) -> broadcast::Sender<Arc<IpcEvent>> {
    let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);

    spawn_bluetooth_watcher(services.bluetooth.subscribe(), tx.clone());
    spawn_network_watcher(services.network.subscribe(), tx.clone());
    spawn_audio_watcher(services.audio.subscribe(), tx.clone());
    spawn_battery_watcher(services.battery.subscribe(), tx.clone());
    spawn_compositor_watcher(services.compositor.subscribe(), tx.clone());
    spawn_keyboard_watcher(services.keyboard.subscribe(), tx.clone());
    spawn_mpris_watcher(services.mpris.subscribe(), tx.clone());
    spawn_notifications_watcher(services.notifications.subscribe(), tx.clone());
    spawn_calendar_watcher(services.calendar_events.subscribe(), tx.clone());
    spawn_brightness_watcher(services.brightness.subscribe(), tx.clone());
    spawn_idle_watcher(services.idle.subscribe(), tx.clone());
    spawn_theme_watcher(services.theme.subscribe(), tx.clone());
    spawn_solar_watcher(services.solar.subscribe(), tx.clone());
    spawn_microphone_watcher(services.microphone.subscribe(), tx.clone());
    spawn_clipboard_watcher(services.clipboard.subscribe(), tx.clone());
    spawn_power_watcher(services.power.subscribe(), tx.clone());
    spawn_session_watcher(services.session.subscribe(), tx.clone());
    spawn_storage_watcher(services.storage.subscribe(), tx.clone());
    spawn_webcam_watcher(services.webcam.subscribe(), tx.clone());
    spawn_tray_watcher(services.tray.subscribe(), tx.clone());
    spawn_location_watcher(services.location.subscribe(), tx.clone());

    tx
}

fn emit(tx: &broadcast::Sender<Arc<IpcEvent>>, name: &str, fields: Vec<(&str, String)>) {
    let owned: Vec<(String, String)> = fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
    let _ = tx.send(Arc::new(IpcEvent::new(name, owned)));
}

fn spawn_bluetooth_watcher(
    mut rx: watch::Receiver<crate::services::bluetooth::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            if !prev.snapshot.status.powered && next.snapshot.status.powered {
                emit(&tx, "bluetooth.powered_on", vec![]);
            } else if prev.snapshot.status.powered && !next.snapshot.status.powered {
                emit(&tx, "bluetooth.powered_off", vec![]);
            }

            let prev_disc = prev.snapshot.status.discovering;
            let next_disc = next.snapshot.status.discovering;
            if !prev_disc && next_disc {
                emit(&tx, "bluetooth.scanning_started", vec![]);
            } else if prev_disc && !next_disc {
                emit(&tx, "bluetooth.scanning_stopped", vec![]);
            }

            let prev_adapters: HashMap<&str, _> = prev
                .snapshot
                .adapters
                .iter()
                .map(|a| (a.path.as_str(), a))
                .collect();
            let next_adapters: HashMap<&str, _> = next
                .snapshot
                .adapters
                .iter()
                .map(|a| (a.path.as_str(), a))
                .collect();
            for (path, adapter) in &next_adapters {
                if !prev_adapters.contains_key(path) {
                    emit(
                        &tx,
                        "bluetooth.adapter_added",
                        vec![
                            ("address", adapter.address.clone()),
                            ("name", adapter.name.clone()),
                        ],
                    );
                }
            }
            for (path, adapter) in &prev_adapters {
                if !next_adapters.contains_key(path) {
                    emit(
                        &tx,
                        "bluetooth.adapter_removed",
                        vec![
                            ("address", adapter.address.clone()),
                            ("name", adapter.name.clone()),
                        ],
                    );
                }
            }

            let prev_by_addr: HashMap<&str, _> = prev
                .snapshot
                .devices
                .iter()
                .map(|d| (d.address.as_str(), d))
                .collect();
            let next_by_addr: HashMap<&str, _> = next
                .snapshot
                .devices
                .iter()
                .map(|d| (d.address.as_str(), d))
                .collect();

            for (addr, dev) in &next_by_addr {
                if !prev_by_addr.contains_key(addr) {
                    emit(
                        &tx,
                        "bluetooth.device_added",
                        vec![
                            ("address", dev.address.clone()),
                            ("name", dev.alias.clone()),
                        ],
                    );
                    continue;
                }
                let prev_dev = prev_by_addr[addr];
                if !prev_dev.connected && dev.connected {
                    emit(
                        &tx,
                        "bluetooth.device_connected",
                        vec![
                            ("address", dev.address.clone()),
                            ("name", dev.alias.clone()),
                        ],
                    );
                } else if prev_dev.connected && !dev.connected {
                    emit(
                        &tx,
                        "bluetooth.device_disconnected",
                        vec![
                            ("address", dev.address.clone()),
                            ("name", dev.alias.clone()),
                        ],
                    );
                }
                if !prev_dev.paired && dev.paired {
                    emit(
                        &tx,
                        "bluetooth.device_paired",
                        vec![
                            ("address", dev.address.clone()),
                            ("name", dev.alias.clone()),
                        ],
                    );
                }
                if !prev_dev.trusted && dev.trusted {
                    emit(
                        &tx,
                        "bluetooth.device_trusted",
                        vec![
                            ("address", dev.address.clone()),
                            ("name", dev.alias.clone()),
                        ],
                    );
                }
                if prev_dev.battery != dev.battery {
                    if let Some(battery) = dev.battery {
                        emit(
                            &tx,
                            "bluetooth.device_battery_changed",
                            vec![
                                ("address", dev.address.clone()),
                                ("name", dev.alias.clone()),
                                ("battery", battery.to_string()),
                            ],
                        );
                    }
                }
            }
            for (addr, dev) in &prev_by_addr {
                if !next_by_addr.contains_key(addr) {
                    let event = if dev.paired {
                        "bluetooth.device_forgotten"
                    } else {
                        "bluetooth.device_removed"
                    };
                    emit(
                        &tx,
                        event,
                        vec![
                            ("address", dev.address.clone()),
                            ("name", dev.alias.clone()),
                        ],
                    );
                }
            }

            prev = next;
        }
    });
}

fn spawn_network_watcher(
    mut rx: watch::Receiver<crate::services::network::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            let prev_connectivity = &prev.snapshot.status.connectivity;
            let next_connectivity = &next.snapshot.status.connectivity;
            if prev_connectivity != next_connectivity {
                let was_connected = prev_connectivity == "full" || prev_connectivity == "limited";
                let is_connected = next_connectivity == "full" || next_connectivity == "limited";
                if !was_connected && is_connected {
                    emit(
                        &tx,
                        "network.connected",
                        vec![("connectivity", next_connectivity.clone())],
                    );
                } else if was_connected && !is_connected {
                    emit(
                        &tx,
                        "network.disconnected",
                        vec![("connectivity", next_connectivity.clone())],
                    );
                }
            }

            let prev_ssid = prev
                .snapshot
                .wifi_access_points
                .iter()
                .find(|ap| ap.connected)
                .map(|ap| ap.ssid.clone());
            let next_ap = next
                .snapshot
                .wifi_access_points
                .iter()
                .find(|ap| ap.connected);
            let next_ssid = next_ap.map(|ap| ap.ssid.clone());
            if prev_ssid != next_ssid {
                emit(
                    &tx,
                    "network.wifi_changed",
                    vec![
                        ("ssid", next_ssid.unwrap_or_default()),
                        ("connected", next_ap.is_some().to_string()),
                    ],
                );
            }

            if !prev.snapshot.status.wifi_enabled && next.snapshot.status.wifi_enabled {
                emit(&tx, "network.wifi_enabled", vec![]);
            } else if prev.snapshot.status.wifi_enabled && !next.snapshot.status.wifi_enabled {
                emit(&tx, "network.wifi_disabled", vec![]);
            }

            if !prev.scanning && next.scanning {
                emit(&tx, "network.scanning_started", vec![]);
            } else if prev.scanning && !next.scanning {
                emit(&tx, "network.scanning_stopped", vec![]);
            }

            let prev_conns: HashMap<&str, _> = prev
                .snapshot
                .connections
                .iter()
                .map(|c| (c.uuid.as_str(), c))
                .collect();
            let next_conns: HashMap<&str, _> = next
                .snapshot
                .connections
                .iter()
                .map(|c| (c.uuid.as_str(), c))
                .collect();

            for (uuid, conn) in &next_conns {
                let prev_state = prev_conns
                    .get(uuid)
                    .map(|c| c.state.as_str())
                    .unwrap_or("unknown");
                let prev_failure = prev_conns.get(uuid).and_then(|c| c.failure.as_ref());

                if conn.vpn {
                    if conn.state == "activated" && prev_state != "activated" {
                        emit(
                            &tx,
                            "network.vpn_connected",
                            vec![
                                ("id", conn.id.clone()),
                                ("uuid", conn.uuid.clone()),
                                ("type", conn.connection_type.clone()),
                            ],
                        );
                    } else if conn.state != "activated" && prev_state == "activated" {
                        emit(
                            &tx,
                            "network.vpn_disconnected",
                            vec![
                                ("id", conn.id.clone()),
                                ("uuid", conn.uuid.clone()),
                                ("type", conn.connection_type.clone()),
                            ],
                        );
                    }
                }

                if conn.failure.is_some() && prev_failure.is_none() {
                    emit(
                        &tx,
                        "network.connection_failed",
                        vec![
                            ("id", conn.id.clone()),
                            ("type", conn.connection_type.clone()),
                            ("reason", network_failure_str(conn.failure.as_ref())),
                        ],
                    );
                }
            }

            for (uuid, conn) in &prev_conns {
                if !next_conns.contains_key(uuid) && conn.vpn && conn.state == "activated" {
                    emit(
                        &tx,
                        "network.vpn_disconnected",
                        vec![
                            ("id", conn.id.clone()),
                            ("uuid", conn.uuid.clone()),
                            ("type", conn.connection_type.clone()),
                        ],
                    );
                }
            }

            let prev_devices: HashMap<&str, _> = prev
                .snapshot
                .devices
                .iter()
                .map(|d| (d.path.as_str(), d))
                .collect();
            let next_devices: HashMap<&str, _> = next
                .snapshot
                .devices
                .iter()
                .map(|d| (d.path.as_str(), d))
                .collect();
            for (path, device) in &next_devices {
                if !prev_devices.contains_key(path) {
                    emit(
                        &tx,
                        "network.adapter_added",
                        vec![
                            ("interface", device.interface.clone()),
                            ("type", device.device_type.clone()),
                        ],
                    );
                }
            }
            for (path, device) in &prev_devices {
                if !next_devices.contains_key(path) {
                    emit(
                        &tx,
                        "network.adapter_removed",
                        vec![
                            ("interface", device.interface.clone()),
                            ("type", device.device_type.clone()),
                        ],
                    );
                }
            }

            prev = next;
        }
    });
}

fn spawn_brightness_watcher(
    mut rx: watch::Receiver<crate::services::brightness::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    use crate::services::brightness::BrightnessSourceKind;

    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            let prev_by_id: HashMap<&str, _> =
                prev.sources.iter().map(|s| (s.id.as_str(), s)).collect();
            let next_by_id: HashMap<&str, _> =
                next.sources.iter().map(|s| (s.id.as_str(), s)).collect();

            for (id, source) in &next_by_id {
                if !prev_by_id.contains_key(id) {
                    emit(
                        &tx,
                        "brightness.source_added",
                        vec![
                            ("id", source.id.clone()),
                            ("name", source.name.clone()),
                            ("kind", brightness_kind_str(source.kind)),
                        ],
                    );
                    continue;
                }
                let prev_source = prev_by_id[id];
                if prev_source.percent != source.percent {
                    let mut fields = vec![
                        ("id", source.id.clone()),
                        ("percent", source.percent.to_string()),
                        ("kind", brightness_kind_str(source.kind)),
                    ];
                    if source.kind == BrightnessSourceKind::ExternalDisplay {
                        fields.push(("name", source.name.clone()));
                    }
                    emit(&tx, "brightness.changed", fields);
                }
            }

            for (id, source) in &prev_by_id {
                if !next_by_id.contains_key(id) {
                    emit(
                        &tx,
                        "brightness.source_removed",
                        vec![
                            ("id", source.id.clone()),
                            ("name", source.name.clone()),
                            ("kind", brightness_kind_str(source.kind)),
                        ],
                    );
                }
            }

            prev = next;
        }
    });
}

fn brightness_kind_str(kind: crate::services::brightness::BrightnessSourceKind) -> String {
    use crate::services::brightness::BrightnessSourceKind as K;
    match kind {
        K::BuiltInDisplay => "built_in_display",
        K::ExternalDisplay => "external_display",
        K::Keyboard => "keyboard",
        K::Other => "other",
    }
    .to_owned()
}

fn network_failure_str(
    failure: Option<&crate::services::network::NetworkFailureClassification>,
) -> String {
    use crate::services::network::NetworkFailureClassification as F;
    match failure {
        Some(F::AuthenticationFailed) => "authentication_failed",
        Some(F::MissingSecrets) => "missing_secrets",
        Some(F::Timeout) => "timeout",
        Some(F::NetworkNotFound) => "network_not_found",
        Some(F::ConfigurationFailed) => "configuration_failed",
        Some(F::ConnectionRemoved) => "connection_removed",
        Some(F::Disconnected) => "disconnected",
        None => "",
    }
    .to_owned()
}

fn spawn_audio_watcher(
    mut rx: watch::Receiver<crate::services::audio::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            // --- default output ---
            {
                let prev_out = prev.default_output();
                let next_out = next.default_output();

                if prev_out.map(|d| d.volume) != next_out.map(|d| d.volume) {
                    emit(
                        &tx,
                        "audio.volume_changed",
                        vec![(
                            "volume",
                            next_out.map(|d| d.volume).unwrap_or(0).to_string(),
                        )],
                    );
                }

                match (prev_out.map(|d| d.muted), next_out.map(|d| d.muted)) {
                    (Some(false) | None, Some(true)) => emit(
                        &tx,
                        "audio.muted",
                        vec![(
                            "volume",
                            next_out.map(|d| d.volume).unwrap_or(0).to_string(),
                        )],
                    ),
                    (Some(true), Some(false)) => emit(
                        &tx,
                        "audio.unmuted",
                        vec![(
                            "volume",
                            next_out.map(|d| d.volume).unwrap_or(0).to_string(),
                        )],
                    ),
                    _ => {}
                }

                if prev_out.map(|d| d.name.as_str()) != next_out.map(|d| d.name.as_str()) {
                    if let Some(d) = next_out {
                        emit(
                            &tx,
                            "audio.default_output_changed",
                            vec![
                                ("name", d.name.clone()),
                                ("description", d.description.clone()),
                            ],
                        );
                    }
                }
            }

            // --- output devices ---
            let prev_out_idxs: HashSet<u64> = prev.outputs.iter().map(|d| d.index).collect();
            let next_out_idxs: HashSet<u64> = next.outputs.iter().map(|d| d.index).collect();
            for &idx in next_out_idxs.difference(&prev_out_idxs) {
                if let Some(d) = next.outputs.iter().find(|d| d.index == idx) {
                    emit(
                        &tx,
                        "audio.device_added",
                        vec![
                            ("index", d.index.to_string()),
                            ("name", d.name.clone()),
                            ("description", d.description.clone()),
                        ],
                    );
                }
            }
            for &idx in prev_out_idxs.difference(&next_out_idxs) {
                if let Some(d) = prev.outputs.iter().find(|d| d.index == idx) {
                    emit(
                        &tx,
                        "audio.device_removed",
                        vec![
                            ("index", d.index.to_string()),
                            ("name", d.name.clone()),
                            ("description", d.description.clone()),
                        ],
                    );
                }
            }

            // --- default input ---
            {
                let prev_in = prev.default_input();
                let next_in = next.default_input();

                if prev_in.map(|d| d.volume) != next_in.map(|d| d.volume) {
                    emit(
                        &tx,
                        "audio.input_volume_changed",
                        vec![("volume", next_in.map(|d| d.volume).unwrap_or(0).to_string())],
                    );
                }

                match (prev_in.map(|d| d.muted), next_in.map(|d| d.muted)) {
                    (Some(false) | None, Some(true)) => emit(
                        &tx,
                        "audio.input_muted",
                        vec![("volume", next_in.map(|d| d.volume).unwrap_or(0).to_string())],
                    ),
                    (Some(true), Some(false)) => emit(
                        &tx,
                        "audio.input_unmuted",
                        vec![("volume", next_in.map(|d| d.volume).unwrap_or(0).to_string())],
                    ),
                    _ => {}
                }

                if prev_in.map(|d| d.name.as_str()) != next_in.map(|d| d.name.as_str()) {
                    if let Some(d) = next_in {
                        emit(
                            &tx,
                            "audio.default_input_changed",
                            vec![
                                ("name", d.name.clone()),
                                ("description", d.description.clone()),
                            ],
                        );
                    }
                }
            }

            // --- input devices ---
            let prev_in_idxs: HashSet<u64> = prev.inputs.iter().map(|d| d.index).collect();
            let next_in_idxs: HashSet<u64> = next.inputs.iter().map(|d| d.index).collect();
            for &idx in next_in_idxs.difference(&prev_in_idxs) {
                if let Some(d) = next.inputs.iter().find(|d| d.index == idx) {
                    emit(
                        &tx,
                        "audio.input_device_added",
                        vec![
                            ("index", d.index.to_string()),
                            ("name", d.name.clone()),
                            ("description", d.description.clone()),
                        ],
                    );
                }
            }
            for &idx in prev_in_idxs.difference(&next_in_idxs) {
                if let Some(d) = prev.inputs.iter().find(|d| d.index == idx) {
                    emit(
                        &tx,
                        "audio.input_device_removed",
                        vec![
                            ("index", d.index.to_string()),
                            ("name", d.name.clone()),
                            ("description", d.description.clone()),
                        ],
                    );
                }
            }

            // --- streams ---
            let prev_stream_idxs: HashSet<u64> = prev.streams.iter().map(|s| s.index).collect();
            let next_stream_idxs: HashSet<u64> = next.streams.iter().map(|s| s.index).collect();
            for &idx in next_stream_idxs.difference(&prev_stream_idxs) {
                if let Some(s) = next.streams.iter().find(|s| s.index == idx) {
                    emit(
                        &tx,
                        "audio.stream_started",
                        vec![
                            ("app_name", s.app_name.clone()),
                            ("app_icon", s.app_icon.clone()),
                        ],
                    );
                }
            }
            for &idx in prev_stream_idxs.difference(&next_stream_idxs) {
                if let Some(s) = prev.streams.iter().find(|s| s.index == idx) {
                    emit(
                        &tx,
                        "audio.stream_stopped",
                        vec![("app_name", s.app_name.clone())],
                    );
                }
            }

            prev = next;
        }
    });
}

fn spawn_battery_watcher(
    mut rx: watch::Receiver<crate::services::battery::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    use crate::services::battery::BatteryState;

    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            // --- main battery status ---
            if prev.status.percentage != next.status.percentage {
                emit(
                    &tx,
                    "battery.level_changed",
                    vec![
                        ("percentage", next.status.percentage.to_string()),
                        ("on_battery", next.status.on_battery.to_string()),
                        ("time_to_empty", next.status.time_to_empty.to_string()),
                        ("energy_rate", format!("{:.2}", next.status.energy_rate)),
                    ],
                );
                if next.status.percentage <= 10 && prev.status.percentage > 10 {
                    emit(
                        &tx,
                        "battery.critical",
                        vec![
                            ("percentage", next.status.percentage.to_string()),
                            ("time_to_empty", next.status.time_to_empty.to_string()),
                            ("energy_rate", format!("{:.2}", next.status.energy_rate)),
                        ],
                    );
                }
            }

            if prev.status.state != next.status.state {
                match &next.status.state {
                    BatteryState::Charging => emit(
                        &tx,
                        "battery.charging_started",
                        vec![
                            ("percentage", next.status.percentage.to_string()),
                            ("time_to_full", next.status.time_to_full.to_string()),
                        ],
                    ),
                    BatteryState::Discharging => emit(
                        &tx,
                        "battery.discharging_started",
                        vec![
                            ("percentage", next.status.percentage.to_string()),
                            ("time_to_empty", next.status.time_to_empty.to_string()),
                            ("energy_rate", format!("{:.2}", next.status.energy_rate)),
                        ],
                    ),
                    BatteryState::FullyCharged => emit(
                        &tx,
                        "battery.fully_charged",
                        vec![("percentage", next.status.percentage.to_string())],
                    ),
                    BatteryState::Empty => emit(
                        &tx,
                        "battery.empty",
                        vec![("percentage", next.status.percentage.to_string())],
                    ),
                    _ => emit(
                        &tx,
                        "battery.state_changed",
                        vec![("on_battery", next.status.on_battery.to_string())],
                    ),
                }
            }

            // --- peripheral devices ---
            let prev_by_path: HashMap<&str, _> =
                prev.devices.iter().map(|d| (d.path.as_str(), d)).collect();
            let next_by_path: HashMap<&str, _> =
                next.devices.iter().map(|d| (d.path.as_str(), d)).collect();
            for (path, dev) in &next_by_path {
                if !prev_by_path.contains_key(path) {
                    emit(
                        &tx,
                        "battery.device_added",
                        vec![
                            ("path", dev.path.clone()),
                            ("model", dev.model.clone()),
                            ("percentage", (dev.percentage as u8).to_string()),
                        ],
                    );
                    continue;
                }
                let prev_dev = prev_by_path[path];
                if (prev_dev.percentage as u8) != (dev.percentage as u8) {
                    emit(
                        &tx,
                        "battery.device_level_changed",
                        vec![
                            ("path", dev.path.clone()),
                            ("model", dev.model.clone()),
                            ("percentage", (dev.percentage as u8).to_string()),
                        ],
                    );
                }
            }
            for (path, dev) in &prev_by_path {
                if !next_by_path.contains_key(path) {
                    emit(
                        &tx,
                        "battery.device_removed",
                        vec![("path", dev.path.clone()), ("model", dev.model.clone())],
                    );
                }
            }

            prev = next;
        }
    });
}

fn spawn_compositor_watcher(
    mut rx: watch::Receiver<crate::services::compositor::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    use crate::compositors::{ScreencastStateCapability, ScreencastTarget};

    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            // --- compositor.workspace_changed ---
            if prev.current_workspace != next.current_workspace {
                let ws = next.current_workspace.and_then(|i| next.workspaces.get(i));
                emit(
                    &tx,
                    "compositor.workspace_changed",
                    vec![
                        (
                            "workspace",
                            ws.and_then(|w| w.name.as_deref()).unwrap_or("").to_owned(),
                        ),
                        (
                            "monitor",
                            ws.and_then(|w| w.monitor.as_deref())
                                .unwrap_or("")
                                .to_owned(),
                        ),
                    ],
                );
            }

            // --- window.* ---
            let prev_wins: HashMap<usize, _> = prev.windows.iter().map(|w| (w.id, w)).collect();
            let next_wins: HashMap<usize, _> = next.windows.iter().map(|w| (w.id, w)).collect();
            for (&id, w) in &next_wins {
                if !prev_wins.contains_key(&id) {
                    emit(
                        &tx,
                        "window.opened",
                        vec![
                            ("title", w.title.as_deref().unwrap_or("").to_owned()),
                            ("app_id", w.app_id.as_deref().unwrap_or("").to_owned()),
                        ],
                    );
                }
            }
            for (&id, w) in &prev_wins {
                if !next_wins.contains_key(&id) {
                    emit(
                        &tx,
                        "window.closed",
                        vec![
                            ("title", w.title.as_deref().unwrap_or("").to_owned()),
                            ("app_id", w.app_id.as_deref().unwrap_or("").to_owned()),
                        ],
                    );
                }
            }
            if prev.focused_window != next.focused_window {
                let w = next.focused_window.and_then(|i| next.windows.get(i));
                emit(
                    &tx,
                    "window.focused",
                    vec![
                        (
                            "title",
                            w.and_then(|w| w.title.as_deref()).unwrap_or("").to_owned(),
                        ),
                        (
                            "app_id",
                            w.and_then(|w| w.app_id.as_deref()).unwrap_or("").to_owned(),
                        ),
                    ],
                );
            }

            // --- monitor.* ---
            let prev_mons: HashMap<&str, _> =
                prev.monitors.iter().map(|m| (m.name.as_str(), m)).collect();
            let next_mons: HashMap<&str, _> =
                next.monitors.iter().map(|m| (m.name.as_str(), m)).collect();
            for (name, m) in &next_mons {
                if !prev_mons.contains_key(name) {
                    emit(
                        &tx,
                        "monitor.added",
                        vec![
                            ("name", m.name.clone()),
                            (
                                "description",
                                m.description.as_deref().unwrap_or("").to_owned(),
                            ),
                        ],
                    );
                } else {
                    let prev_m = prev_mons[name];
                    if !prev_m.enabled && m.enabled {
                        emit(&tx, "monitor.enabled", vec![("name", m.name.clone())]);
                    } else if prev_m.enabled && !m.enabled {
                        emit(&tx, "monitor.disabled", vec![("name", m.name.clone())]);
                    }
                }
            }
            for (name, m) in &prev_mons {
                if !next_mons.contains_key(name) {
                    emit(
                        &tx,
                        "monitor.removed",
                        vec![
                            ("name", m.name.clone()),
                            (
                                "description",
                                m.description.as_deref().unwrap_or("").to_owned(),
                            ),
                        ],
                    );
                }
            }

            // --- screencast.* ---
            if next.capabilities.screencast_state != ScreencastStateCapability::None {
                let prev_casts: HashMap<&str, _> = prev
                    .screencasts
                    .iter()
                    .map(|s| (s.id.as_str(), s))
                    .collect();
                let next_casts: HashMap<&str, _> = next
                    .screencasts
                    .iter()
                    .map(|s| (s.id.as_str(), s))
                    .collect();
                for (id, s) in &next_casts {
                    if !prev_casts.contains_key(id) {
                        let target = match s.target {
                            ScreencastTarget::Monitor => "monitor",
                            ScreencastTarget::Window => "window",
                            ScreencastTarget::Unknown => "unknown",
                        };
                        emit(
                            &tx,
                            "screencast.started",
                            vec![("id", s.id.clone()), ("target", target.to_owned())],
                        );
                    }
                }
                for (id, s) in &prev_casts {
                    if !next_casts.contains_key(id) {
                        emit(&tx, "screencast.stopped", vec![("id", s.id.clone())]);
                    }
                }
            }

            prev = next;
        }
    });
}

fn spawn_keyboard_watcher(
    mut rx: watch::Receiver<crate::services::keyboard::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            if prev.current_layout != next.current_layout {
                if let Some(layout) = &next.current_layout {
                    emit(
                        &tx,
                        "input.keyboard_layout_changed",
                        vec![
                            ("name", layout.name.clone()),
                            ("code", layout.code.clone()),
                            ("label", layout.label.clone()),
                        ],
                    );
                }
            }

            prev = next;
        }
    });
}

fn spawn_mpris_watcher(
    mut rx: watch::Receiver<crate::services::mpris::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    use crate::services::mpris::PlaybackStatus;

    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            // --- player list lifecycle ---
            let prev_by_id: HashMap<&str, _> = prev
                .snapshot
                .players
                .iter()
                .map(|p| (p.player_id.as_str(), p))
                .collect();
            let next_by_id: HashMap<&str, _> = next
                .snapshot
                .players
                .iter()
                .map(|p| (p.player_id.as_str(), p))
                .collect();
            for (id, p) in &next_by_id {
                if !prev_by_id.contains_key(id) {
                    emit(
                        &tx,
                        "mpris.player_appeared",
                        vec![
                            ("player", p.identity.clone()),
                            ("player_id", p.player_id.clone()),
                        ],
                    );
                }
            }
            for (id, p) in &prev_by_id {
                if !next_by_id.contains_key(id) {
                    emit(
                        &tx,
                        "mpris.player_disappeared",
                        vec![
                            ("player", p.identity.clone()),
                            ("player_id", p.player_id.clone()),
                        ],
                    );
                }
            }

            // --- current player ---
            let prev_cur = prev.snapshot.current_player.as_ref();
            let next_cur = next.snapshot.current_player.as_ref();

            let prev_player_id = prev_cur.map(|p| p.player_id.as_str());
            let next_player_id = next_cur.map(|p| p.player_id.as_str());
            if prev_player_id != next_player_id {
                if let Some(p) = next_cur {
                    emit(
                        &tx,
                        "mpris.player_switched",
                        vec![
                            ("player", p.identity.clone()),
                            ("player_id", p.player_id.clone()),
                        ],
                    );
                }
            }

            let prev_status = prev_cur.map(|p| p.playback_status);
            let next_status = next_cur.map(|p| p.playback_status);
            if prev_status != next_status {
                let event = match next_status {
                    Some(PlaybackStatus::Playing) => "mpris.playing",
                    Some(PlaybackStatus::Paused) => "mpris.paused",
                    _ => "mpris.stopped",
                };
                emit(
                    &tx,
                    event,
                    vec![
                        (
                            "player",
                            next_cur
                                .map(|p| p.identity.as_str())
                                .unwrap_or("")
                                .to_owned(),
                        ),
                        (
                            "player_id",
                            next_cur
                                .map(|p| p.player_id.as_str())
                                .unwrap_or("")
                                .to_owned(),
                        ),
                        (
                            "title",
                            next_cur.map(|p| p.title.as_str()).unwrap_or("").to_owned(),
                        ),
                        (
                            "artist",
                            next_cur.map(|p| p.artist.as_str()).unwrap_or("").to_owned(),
                        ),
                        (
                            "album",
                            next_cur.map(|p| p.album.as_str()).unwrap_or("").to_owned(),
                        ),
                    ],
                );
            }

            let prev_track = prev_cur.map(|p| (p.title.as_str(), p.artist.as_str()));
            let next_track = next_cur.map(|p| (p.title.as_str(), p.artist.as_str()));
            if prev_track != next_track && next_status == Some(PlaybackStatus::Playing) {
                if let Some(p) = next_cur {
                    emit(
                        &tx,
                        "mpris.track_changed",
                        vec![
                            ("player", p.identity.clone()),
                            ("player_id", p.player_id.clone()),
                            ("title", p.title.clone()),
                            ("artist", p.artist.clone()),
                            ("album", p.album.clone()),
                        ],
                    );
                }
            }

            prev = next;
        }
    });
}

fn spawn_idle_watcher(
    mut rx: watch::Receiver<crate::services::idle::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            let prev_fired: HashSet<usize> = prev.fired_listeners.iter().copied().collect();
            let next_fired: HashSet<usize> = next.fired_listeners.iter().copied().collect();
            for &id in next_fired.difference(&prev_fired) {
                let timeout = next
                    .listeners
                    .iter()
                    .find(|l| l.id == id)
                    .map(|l| l.timeout)
                    .unwrap_or(0);
                emit(
                    &tx,
                    "idle.triggered",
                    vec![
                        ("listener", id.to_string()),
                        ("timeout", timeout.to_string()),
                    ],
                );
            }
            for &id in prev_fired.difference(&next_fired) {
                let timeout = prev
                    .listeners
                    .iter()
                    .find(|l| l.id == id)
                    .map(|l| l.timeout)
                    .unwrap_or(0);
                emit(
                    &tx,
                    "idle.resumed",
                    vec![
                        ("listener", id.to_string()),
                        ("timeout", timeout.to_string()),
                    ],
                );
            }

            prev = next;
        }
    });
}

fn notification_urgency_str(urgency: u8) -> &'static str {
    match urgency {
        0 => "low",
        2 => "critical",
        _ => "normal",
    }
}

fn spawn_notifications_watcher(
    mut rx: watch::Receiver<crate::services::notifications::model::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            if !prev.dnd && next.dnd {
                emit(&tx, "notification.dnd_enabled", vec![]);
            } else if prev.dnd && !next.dnd {
                emit(&tx, "notification.dnd_disabled", vec![]);
            }

            let prev_ids: HashSet<u32> = prev.notifications.iter().map(|n| n.id).collect();
            for n in &next.notifications {
                if !prev_ids.contains(&n.id) {
                    emit(
                        &tx,
                        "notification.received",
                        vec![
                            ("id", n.id.to_string()),
                            ("app", n.app_name.clone()),
                            ("summary", n.summary.clone()),
                            ("body", n.body.clone()),
                            ("urgency", notification_urgency_str(n.urgency).to_owned()),
                        ],
                    );
                }
            }

            let next_ids: HashSet<u32> = next.notifications.iter().map(|n| n.id).collect();
            for n in &prev.notifications {
                if !next_ids.contains(&n.id) {
                    emit(
                        &tx,
                        "notification.closed",
                        vec![("id", n.id.to_string()), ("app", n.app_name.clone())],
                    );
                }
            }

            prev = next;
        }
    });
}

fn spawn_calendar_watcher(
    mut rx: watch::Receiver<crate::services::calendar_events::model::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            let prev_events: HashMap<&str, _> = prev
                .month_cache
                .values()
                .flat_map(|m| m.day_snapshots.values())
                .flat_map(|d| d.events.iter())
                .map(|e| (e.event_id.as_str(), e))
                .collect();
            let next_events: HashMap<&str, _> = next
                .month_cache
                .values()
                .flat_map(|m| m.day_snapshots.values())
                .flat_map(|d| d.events.iter())
                .map(|e| (e.event_id.as_str(), e))
                .collect();

            for (id, e) in &next_events {
                if !prev_events.contains_key(id) {
                    emit(
                        &tx,
                        "calendar.event_added",
                        vec![
                            ("event_id", e.event_id.clone()),
                            ("title", e.title.clone()),
                            ("start", e.start.clone()),
                            ("end", e.end.clone()),
                            ("all_day", e.all_day.to_string()),
                            ("source", e.source.display_name.clone()),
                        ],
                    );
                }
            }
            for (id, e) in &prev_events {
                if !next_events.contains_key(id) {
                    emit(
                        &tx,
                        "calendar.event_removed",
                        vec![
                            ("event_id", e.event_id.clone()),
                            ("title", e.title.clone()),
                            ("source", e.source.display_name.clone()),
                        ],
                    );
                }
            }

            prev = next;
        }
    });
}

fn spawn_theme_watcher(
    mut rx: watch::Receiver<crate::services::theme::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    use crate::services::theme::{EffectiveThemeMode, ThemeReason};

    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            if prev.effective_mode != next.effective_mode || prev.reason != next.reason {
                let mode = match next.effective_mode {
                    EffectiveThemeMode::Light => "light",
                    EffectiveThemeMode::Dark => "dark",
                };
                let reason = match next.reason {
                    ThemeReason::Config => "config",
                    ThemeReason::SolarDay => "solar_day",
                    ThemeReason::SolarNight => "solar_night",
                    ThemeReason::SolarUnavailable => "solar_unavailable",
                };
                emit(
                    &tx,
                    "theme.changed",
                    vec![("mode", mode.to_owned()), ("reason", reason.to_owned())],
                );
            }

            prev = next;
        }
    });
}

fn spawn_microphone_watcher(
    mut rx: watch::Receiver<crate::services::microphone::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            if prev.usages.is_empty() && !next.usages.is_empty() {
                emit(&tx, "mic.in_use", vec![]);
            } else if !prev.usages.is_empty() && next.usages.is_empty() {
                emit(&tx, "mic.released", vec![]);
            }

            let prev_indices: HashSet<u64> = prev.usages.iter().map(|u| u.index).collect();
            let next_indices: HashSet<u64> = next.usages.iter().map(|u| u.index).collect();
            for usage in next
                .usages
                .iter()
                .filter(|u| !prev_indices.contains(&u.index))
            {
                emit(
                    &tx,
                    "mic.app_started",
                    vec![("app_name", usage.app_name.clone())],
                );
            }
            for usage in prev
                .usages
                .iter()
                .filter(|u| !next_indices.contains(&u.index))
            {
                emit(
                    &tx,
                    "mic.app_stopped",
                    vec![("app_name", usage.app_name.clone())],
                );
            }

            prev = next;
        }
    });
}

fn spawn_clipboard_watcher(
    mut rx: watch::Receiver<crate::services::clipboard::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            if prev.current_id != next.current_id {
                match next.current_id {
                    None => emit(&tx, "clipboard.cleared", vec![]),
                    Some(id) => {
                        if let Some(entry) = next.history.iter().find(|e| e.id == id) {
                            use crate::services::clipboard::ClipboardEntryKind;
                            let kind_str = match entry.kind {
                                ClipboardEntryKind::Text => "text",
                                ClipboardEntryKind::Html => "html",
                                ClipboardEntryKind::Image => "image",
                                ClipboardEntryKind::Files => "files",
                                ClipboardEntryKind::Other => "other",
                            };
                            emit(
                                &tx,
                                "clipboard.changed",
                                vec![
                                    ("kind", kind_str.to_owned()),
                                    ("mime_type", entry.mime_type.clone()),
                                    ("preview", entry.preview.clone()),
                                ],
                            );
                        }
                    }
                }
            }

            prev = next;
        }
    });
}

fn spawn_power_watcher(
    mut rx: watch::Receiver<crate::services::power::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            if prev.profiles.active != next.profiles.active {
                emit(
                    &tx,
                    "power.profile_changed",
                    vec![("profile", next.profiles.active.clone())],
                );
            }

            prev = next;
        }
    });
}

fn spawn_session_watcher(
    mut rx: watch::Receiver<crate::services::session::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            if prev.active_action != next.active_action {
                if let Some(action) = &next.active_action {
                    use crate::services::session::SessionAction;
                    let action_str = match action {
                        SessionAction::Lock => "lock",
                        SessionAction::Logout => "logout",
                        SessionAction::Suspend => "suspend",
                        SessionAction::Hibernate => "hibernate",
                        SessionAction::Reboot => "reboot",
                        SessionAction::PowerOff => "power-off",
                    };
                    emit(
                        &tx,
                        "session.action",
                        vec![("action", action_str.to_owned())],
                    );
                }
            }

            prev = next;
        }
    });
}

fn spawn_storage_watcher(
    mut rx: watch::Receiver<crate::services::storage::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            let prev_ids: HashSet<&str> = prev.devices.iter().map(|d| d.id.as_str()).collect();
            let next_ids: HashSet<&str> = next.devices.iter().map(|d| d.id.as_str()).collect();

            for device in next
                .devices
                .iter()
                .filter(|d| !prev_ids.contains(d.id.as_str()))
            {
                use crate::services::storage::StorageKind;
                let kind_str = match device.kind {
                    StorageKind::Drive => "drive",
                    StorageKind::Optical => "optical",
                    StorageKind::Card => "card",
                };
                emit(
                    &tx,
                    "storage.device_added",
                    vec![
                        ("id", device.id.clone()),
                        ("name", device.name.clone()),
                        ("kind", kind_str.to_owned()),
                        ("removable", device.removable.to_string()),
                    ],
                );
            }
            for device in prev
                .devices
                .iter()
                .filter(|d| !next_ids.contains(d.id.as_str()))
            {
                emit(
                    &tx,
                    "storage.device_removed",
                    vec![("id", device.id.clone()), ("name", device.name.clone())],
                );
            }

            for next_dev in next
                .devices
                .iter()
                .filter(|d| prev_ids.contains(d.id.as_str()))
            {
                if let Some(prev_dev) = prev.devices.iter().find(|d| d.id == next_dev.id) {
                    if prev_dev.mounted_at != next_dev.mounted_at {
                        match &next_dev.mounted_at {
                            Some(path) => emit(
                                &tx,
                                "storage.device_mounted",
                                vec![
                                    ("id", next_dev.id.clone()),
                                    ("name", next_dev.name.clone()),
                                    ("path", path.display().to_string()),
                                ],
                            ),
                            None => emit(
                                &tx,
                                "storage.device_unmounted",
                                vec![("id", next_dev.id.clone()), ("name", next_dev.name.clone())],
                            ),
                        }
                    }
                }
            }

            prev = next;
        }
    });
}

fn spawn_webcam_watcher(
    mut rx: watch::Receiver<crate::services::webcam::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            if prev.usages.is_empty() && !next.usages.is_empty() {
                emit(&tx, "webcam.in_use", vec![]);
            } else if !prev.usages.is_empty() && next.usages.is_empty() {
                emit(&tx, "webcam.released", vec![]);
            }

            let prev_ids: HashSet<&str> = prev.usages.iter().map(|u| u.id.as_str()).collect();
            let next_ids: HashSet<&str> = next.usages.iter().map(|u| u.id.as_str()).collect();
            for usage in next
                .usages
                .iter()
                .filter(|u| !prev_ids.contains(u.id.as_str()))
            {
                emit(
                    &tx,
                    "webcam.app_started",
                    vec![
                        ("app_name", usage.app_name.clone()),
                        ("camera_name", usage.camera_name.clone()),
                    ],
                );
            }
            for usage in prev
                .usages
                .iter()
                .filter(|u| !next_ids.contains(u.id.as_str()))
            {
                emit(
                    &tx,
                    "webcam.app_stopped",
                    vec![
                        ("app_name", usage.app_name.clone()),
                        ("camera_name", usage.camera_name.clone()),
                    ],
                );
            }

            prev = next;
        }
    });
}

fn spawn_tray_watcher(
    mut rx: watch::Receiver<crate::services::tray::protocol::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            let prev_addrs: HashSet<&str> = prev
                .snapshot
                .items
                .iter()
                .map(|i| i.address.as_str())
                .collect();
            let next_addrs: HashSet<&str> = next
                .snapshot
                .items
                .iter()
                .map(|i| i.address.as_str())
                .collect();

            for item in next
                .snapshot
                .items
                .iter()
                .filter(|i| !prev_addrs.contains(i.address.as_str()))
            {
                emit(
                    &tx,
                    "tray.item_added",
                    vec![("id", item.id.clone()), ("title", item.title.clone())],
                );
            }
            for item in prev
                .snapshot
                .items
                .iter()
                .filter(|i| !next_addrs.contains(i.address.as_str()))
            {
                emit(
                    &tx,
                    "tray.item_removed",
                    vec![("id", item.id.clone()), ("title", item.title.clone())],
                );
            }

            prev = next;
        }
    });
}

fn spawn_location_watcher(
    mut rx: watch::Receiver<crate::services::location::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    use crate::services::location::State as LocationState;

    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            let prev_coords = if let LocationState::Ready(c) = &prev {
                Some((c.latitude.to_bits(), c.longitude.to_bits()))
            } else {
                None
            };
            let next_coords = if let LocationState::Ready(c) = &next {
                Some((c.latitude.to_bits(), c.longitude.to_bits()))
            } else {
                None
            };

            if prev_coords != next_coords {
                if let LocationState::Ready(c) = &next {
                    emit(
                        &tx,
                        "location.updated",
                        vec![
                            ("latitude", c.latitude.to_string()),
                            ("longitude", c.longitude.to_string()),
                        ],
                    );
                }
            }

            prev = next;
        }
    });
}

fn solar_is_day(sunrise: &str, sunset: &str) -> bool {
    use chrono::Local;
    use chrono::NaiveTime;
    let now = Local::now().time();
    let Ok(rise) = NaiveTime::parse_from_str(sunrise, "%H:%M") else {
        return false;
    };
    let Ok(set) = NaiveTime::parse_from_str(sunset, "%H:%M") else {
        return false;
    };
    now >= rise && now < set
}

fn spawn_solar_watcher(
    mut rx: watch::Receiver<crate::services::solar::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    use crate::services::solar::State as SolarState;

    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            let prev_snap = if let SolarState::Ready(s) = &prev {
                Some(s)
            } else {
                None
            };
            let next_snap = if let SolarState::Ready(s) = &next {
                Some(s)
            } else {
                None
            };

            if prev_snap.map(|s| (&s.date, &s.times)) != next_snap.map(|s| (&s.date, &s.times)) {
                if let Some(s) = next_snap {
                    let is_day = solar_is_day(&s.times.sunrise, &s.times.sunset);
                    emit(
                        &tx,
                        "solar.updated",
                        vec![
                            ("date", s.date.to_string()),
                            ("sunrise", s.times.sunrise.clone()),
                            ("sunset", s.times.sunset.clone()),
                            ("is_day", is_day.to_string()),
                        ],
                    );
                }
            }

            prev = next;
        }
    });
}
