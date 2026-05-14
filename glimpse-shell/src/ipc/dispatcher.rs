use std::{collections::{HashMap, HashSet}, sync::Arc};

use tokio::sync::{broadcast, watch};

use crate::services::framework::Services;

use super::protocol::IpcEvent;

const BROADCAST_CAPACITY: usize = 256;

pub fn start(services: &Services) -> broadcast::Sender<Arc<IpcEvent>> {
    let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);

    spawn_bluetooth_watcher(services.bluetooth.subscribe(), tx.clone());
    spawn_network_watcher(services.network.subscribe(), tx.clone());
    spawn_audio_watcher(services.audio.subscribe(), tx.clone());
    spawn_battery_watcher(services.battery.subscribe(), tx.clone());
    spawn_compositor_watcher(services.compositor.subscribe(), tx.clone());
    spawn_mpris_watcher(services.mpris.subscribe(), tx.clone());
    spawn_notifications_watcher(services.notifications.subscribe(), tx.clone());
    spawn_brightness_watcher(services.brightness.subscribe(), tx.clone());

    tx
}

fn emit(tx: &broadcast::Sender<Arc<IpcEvent>>, name: &str, fields: Vec<(&str, String)>) {
    let owned: Vec<(String, String)> = fields
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
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
                        vec![("address", dev.address.clone()), ("name", dev.alias.clone())],
                    );
                    continue;
                }
                let prev_dev = prev_by_addr[addr];
                if !prev_dev.connected && dev.connected {
                    emit(
                        &tx,
                        "bluetooth.device_connected",
                        vec![("address", dev.address.clone()), ("name", dev.alias.clone())],
                    );
                } else if prev_dev.connected && !dev.connected {
                    emit(
                        &tx,
                        "bluetooth.device_disconnected",
                        vec![("address", dev.address.clone()), ("name", dev.alias.clone())],
                    );
                }
                if !prev_dev.paired && dev.paired {
                    emit(
                        &tx,
                        "bluetooth.device_paired",
                        vec![("address", dev.address.clone()), ("name", dev.alias.clone())],
                    );
                }
                if !prev_dev.trusted && dev.trusted {
                    emit(
                        &tx,
                        "bluetooth.device_trusted",
                        vec![("address", dev.address.clone()), ("name", dev.alias.clone())],
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
                        vec![("address", dev.address.clone()), ("name", dev.alias.clone())],
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
                let was_connected =
                    prev_connectivity == "full" || prev_connectivity == "limited";
                let is_connected =
                    next_connectivity == "full" || next_connectivity == "limited";
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
                        vec![("volume", next_out.map(|d| d.volume).unwrap_or(0).to_string())],
                    );
                }

                match (prev_out.map(|d| d.muted), next_out.map(|d| d.muted)) {
                    (Some(false) | None, Some(true)) => emit(
                        &tx,
                        "audio.muted",
                        vec![("volume", next_out.map(|d| d.volume).unwrap_or(0).to_string())],
                    ),
                    (Some(true), Some(false)) => emit(
                        &tx,
                        "audio.unmuted",
                        vec![("volume", next_out.map(|d| d.volume).unwrap_or(0).to_string())],
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
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            if prev.status.percentage != next.status.percentage {
                emit(
                    &tx,
                    "battery.level_changed",
                    vec![("percentage", next.status.percentage.to_string())],
                );
                if next.status.percentage <= 10 && prev.status.percentage > 10 {
                    emit(
                        &tx,
                        "battery.critical",
                        vec![("percentage", next.status.percentage.to_string())],
                    );
                }
            }

            if prev.status.state != next.status.state {
                let event = match &next.status.state {
                    crate::services::battery::BatteryState::Charging => "battery.charging_started",
                    crate::services::battery::BatteryState::Discharging => {
                        "battery.discharging_started"
                    }
                    _ => "battery.state_changed",
                };
                emit(
                    &tx,
                    event,
                    vec![("on_battery", next.status.on_battery.to_string())],
                );
            }

            prev = next;
        }
    });
}

fn spawn_compositor_watcher(
    mut rx: watch::Receiver<crate::services::compositor::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            if prev.current_workspace != next.current_workspace {
                let name = next
                    .current_workspace
                    .and_then(|i| next.workspaces.get(i))
                    .and_then(|w| w.name.as_deref())
                    .unwrap_or("")
                    .to_owned();
                emit(
                    &tx,
                    "compositor.workspace_changed",
                    vec![("workspace", name)],
                );
            }

            if prev.focused_window != next.focused_window {
                let title = next
                    .focused_window
                    .and_then(|i| next.windows.get(i))
                    .and_then(|w| w.title.as_deref())
                    .unwrap_or("")
                    .to_owned();
                emit(
                    &tx,
                    "compositor.window_focused",
                    vec![("title", title)],
                );
            }

            prev = next;
        }
    });
}

fn spawn_mpris_watcher(
    mut rx: watch::Receiver<crate::services::mpris::State>,
    tx: broadcast::Sender<Arc<IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            let prev_status = prev
                .snapshot
                .current_player
                .as_ref()
                .map(|p| p.playback_status);
            let next_player = next.snapshot.current_player.as_ref();
            let next_status = next_player.map(|p| p.playback_status);

            if prev_status != next_status {
                let event = match next_status {
                    Some(crate::services::mpris::PlaybackStatus::Playing) => "mpris.playing",
                    Some(crate::services::mpris::PlaybackStatus::Paused) => "mpris.paused",
                    _ => "mpris.stopped",
                };
                emit(
                    &tx,
                    event,
                    vec![
                        ("player", next_player.map(|p| p.identity.as_str()).unwrap_or("").to_owned()),
                        ("title", next_player.map(|p| p.title.as_str()).unwrap_or("").to_owned()),
                        ("artist", next_player.map(|p| p.artist.as_str()).unwrap_or("").to_owned()),
                    ],
                );
            }

            let prev_track = prev
                .snapshot
                .current_player
                .as_ref()
                .map(|p| (p.title.clone(), p.artist.clone()));
            let next_track = next_player.map(|p| (p.title.clone(), p.artist.clone()));
            if prev_track != next_track
                && next_status == Some(crate::services::mpris::PlaybackStatus::Playing)
            {
                if let Some(p) = next_player {
                    emit(
                        &tx,
                        "mpris.track_changed",
                        vec![
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
                            ("urgency", n.urgency.to_string()),
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
