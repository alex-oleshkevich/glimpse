use std::{collections::HashSet, sync::Arc};

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

            let prev_disc = prev.snapshot.status.discovering;
            let next_disc = next.snapshot.status.discovering;
            if !prev_disc && next_disc {
                emit(&tx, "bluetooth.scanning_started", vec![]);
            } else if prev_disc && !next_disc {
                emit(&tx, "bluetooth.scanning_stopped", vec![]);
            }

            let prev_by_addr: std::collections::HashMap<&str, _> = prev
                .snapshot
                .devices
                .iter()
                .map(|d| (d.address.as_str(), d))
                .collect();
            let next_by_addr: std::collections::HashMap<&str, _> = next
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
                let prev_connected = prev_by_addr[addr].connected;
                if !prev_connected && dev.connected {
                    emit(
                        &tx,
                        "bluetooth.device_connected",
                        vec![("address", dev.address.clone()), ("name", dev.alias.clone())],
                    );
                } else if prev_connected && !dev.connected {
                    emit(
                        &tx,
                        "bluetooth.device_disconnected",
                        vec![("address", dev.address.clone()), ("name", dev.alias.clone())],
                    );
                }
            }
            for (addr, dev) in &prev_by_addr {
                if !next_by_addr.contains_key(addr) {
                    emit(
                        &tx,
                        "bluetooth.device_removed",
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

            let prev_conn = &prev.snapshot.status.connectivity;
            let next_conn = &next.snapshot.status.connectivity;
            if prev_conn != next_conn {
                let was_connected = prev_conn == "full" || prev_conn == "limited";
                let is_connected = next_conn == "full" || next_conn == "limited";
                if !was_connected && is_connected {
                    emit(
                        &tx,
                        "network.connected",
                        vec![("connectivity", next_conn.clone())],
                    );
                } else if was_connected && !is_connected {
                    emit(
                        &tx,
                        "network.disconnected",
                        vec![("connectivity", next_conn.clone())],
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

            prev = next;
        }
    });
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

            let prev_vol = prev.default_output().map(|d| d.volume);
            let next_out = next.default_output();
            let next_vol = next_out.map(|d| d.volume);

            if prev_vol != next_vol {
                emit(
                    &tx,
                    "audio.volume_changed",
                    vec![("volume", next_vol.unwrap_or(0).to_string())],
                );
            }

            let prev_muted = prev.default_output().map(|d| d.muted);
            let next_muted = next_out.map(|d| d.muted);
            match (prev_muted, next_muted) {
                (Some(false) | None, Some(true)) => {
                    emit(&tx, "audio.muted", vec![]);
                }
                (Some(true), Some(false)) => {
                    emit(&tx, "audio.unmuted", vec![]);
                }
                _ => {}
            }

            let prev_indices: HashSet<u64> = prev.outputs.iter().map(|d| d.index).collect();
            let next_indices: HashSet<u64> = next.outputs.iter().map(|d| d.index).collect();
            for &idx in next_indices.difference(&prev_indices) {
                if let Some(d) = next.outputs.iter().find(|d| d.index == idx) {
                    emit(
                        &tx,
                        "audio.device_added",
                        vec![("index", d.index.to_string()), ("name", d.description.clone())],
                    );
                }
            }
            for &idx in prev_indices.difference(&next_indices) {
                if let Some(d) = prev.outputs.iter().find(|d| d.index == idx) {
                    emit(
                        &tx,
                        "audio.device_removed",
                        vec![("index", d.index.to_string()), ("name", d.description.clone())],
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
