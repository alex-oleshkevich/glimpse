use std::sync::Arc;

use tokio::sync::broadcast;

use glimpse_core::{
    NightLightHealth, NightLightPhase,
    ipc::{self, IpcHandle, IpcServer, client::NoopCommandHandler, sunset_socket_path},
    services::night_light::{NightLightHandle, State},
};

pub fn start(night_light: NightLightHandle) -> IpcHandle {
    let tx = ipc::new_event_channel();
    spawn_watcher(night_light.subscribe(), tx.clone());
    IpcServer::launch_at(tx, sunset_socket_path(), NoopCommandHandler)
}

fn spawn_watcher(
    mut rx: tokio::sync::watch::Receiver<State>,
    tx: broadcast::Sender<Arc<glimpse_core::ipc::protocol::IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            if prev.phase != next.phase {
                let phase_str = phase_name(next.phase);
                ipc::emit(&tx, "nightlight.phase_changed", vec![("phase", phase_str.to_owned())]);

                if next.phase == NightLightPhase::Night {
                    ipc::emit(
                        &tx,
                        "nightlight.activated",
                        vec![("temperature", next.effective_temperature_kelvin.to_string())],
                    );
                } else if next.phase == NightLightPhase::Day {
                    ipc::emit(&tx, "nightlight.deactivated", vec![]);
                }
            }

            if prev.effective_temperature_kelvin != next.effective_temperature_kelvin {
                ipc::emit(
                    &tx,
                    "nightlight.temperature_changed",
                    vec![
                        ("kelvin", next.effective_temperature_kelvin.to_string()),
                        ("phase", phase_name(next.phase).to_owned()),
                    ],
                );
            }

            if prev.health != next.health {
                ipc::emit(
                    &tx,
                    "nightlight.health_changed",
                    vec![("health", health_name(&next.health).to_owned())],
                );
            }

            prev = next;
        }
    });
}

fn phase_name(phase: NightLightPhase) -> &'static str {
    match phase {
        NightLightPhase::Disabled => "disabled",
        NightLightPhase::Day => "day",
        NightLightPhase::TransitionToNight => "transition_to_night",
        NightLightPhase::Night => "night",
        NightLightPhase::TransitionToDay => "transition_to_day",
    }
}

fn health_name(health: &NightLightHealth) -> &'static str {
    match health {
        NightLightHealth::Starting => "starting",
        NightLightHealth::Ready => "ready",
        NightLightHealth::Unsupported => "unsupported",
        NightLightHealth::Reconnecting { .. } => "reconnecting",
        NightLightHealth::Degraded { .. } => "degraded",
    }
}
