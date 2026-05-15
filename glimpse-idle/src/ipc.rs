use std::{pin::Pin, sync::Arc};

use glimpse_core::{
    ipc::{self, IpcHandle, IpcServer, client::CommandHandler, idle_socket_path},
    services::idle_inhibitor::{self, SourceKind},
};
use tokio::sync::{Mutex, watch};

use crate::inhibitor_registry::Registry;

pub fn start(
    state_rx: watch::Receiver<idle_inhibitor::State>,
    registry: Arc<Mutex<Registry>>,
    on_change: Arc<dyn Fn() + Send + Sync>,
) -> IpcHandle {
    let tx = ipc::new_event_channel();
    spawn_watcher(state_rx, tx.clone());
    IpcServer::launch_at(tx, idle_socket_path(), IdleCommandHandler { registry, on_change })
}

fn spawn_watcher(
    mut rx: watch::Receiver<idle_inhibitor::State>,
    tx: tokio::sync::broadcast::Sender<Arc<glimpse_core::ipc::protocol::IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            // Diff inhibitors by id.
            let prev_ids: std::collections::HashSet<u64> =
                prev.inhibitors.iter().map(|r| r.id).collect();
            let next_ids: std::collections::HashSet<u64> =
                next.inhibitors.iter().map(|r| r.id).collect();

            for record in &next.inhibitors {
                if !prev_ids.contains(&record.id) {
                    ipc::emit(
                        &tx,
                        "idle.inhibitor_added",
                        vec![
                            ("id", record.id.to_string()),
                            ("who", record.who.clone()),
                            ("why", record.why.clone()),
                            ("source", source_name(&record.source.kind).to_owned()),
                        ],
                    );
                }
            }
            for record in &prev.inhibitors {
                if !next_ids.contains(&record.id) {
                    ipc::emit(
                        &tx,
                        "idle.inhibitor_removed",
                        vec![
                            ("id", record.id.to_string()),
                            ("who", record.who.clone()),
                        ],
                    );
                }
            }

            // Diff health fields.
            if prev.health.screen_saver != next.health.screen_saver {
                ipc::emit(
                    &tx,
                    "idle.backend_health_changed",
                    vec![
                        ("backend", "screen_saver".to_owned()),
                        ("health", backend_health_name(&next.health.screen_saver).to_owned()),
                    ],
                );
            }
            if prev.health.portal != next.health.portal {
                ipc::emit(
                    &tx,
                    "idle.backend_health_changed",
                    vec![
                        ("backend", "portal".to_owned()),
                        ("health", backend_health_name(&next.health.portal).to_owned()),
                    ],
                );
            }
            if prev.health.login1 != next.health.login1 {
                ipc::emit(
                    &tx,
                    "idle.backend_health_changed",
                    vec![
                        ("backend", "login1".to_owned()),
                        ("health", backend_health_name(&next.health.login1).to_owned()),
                    ],
                );
            }

            prev = next;
        }
    });
}

fn source_name(kind: &SourceKind) -> &'static str {
    match kind {
        SourceKind::ScreenSaver => "screen_saver",
        SourceKind::Portal => "portal",
        SourceKind::Login1 => "login1",
    }
}

fn backend_health_name(health: &glimpse_core::services::idle_inhibitor::BackendHealth) -> &'static str {
    use glimpse_core::services::idle_inhibitor::HealthKind;
    match health.kind {
        HealthKind::Ready => "ready",
        HealthKind::Degraded => "degraded",
        HealthKind::Unsupported => "unsupported",
    }
}

#[derive(Clone)]
struct IdleCommandHandler {
    registry: Arc<Mutex<Registry>>,
    on_change: Arc<dyn Fn() + Send + Sync>,
}

impl CommandHandler for IdleCommandHandler {
    fn execute<'a>(
        &'a self,
        name: &'a str,
        fields: &'a [(String, String)],
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            match name {
                "release" => {
                    let id_str = fields
                        .iter()
                        .find(|(k, _)| k == "id")
                        .map(|(_, v)| v.as_str())
                        .unwrap_or("");
                    let id: u64 = id_str
                        .parse()
                        .map_err(|_| format!("invalid id: {id_str}"))?;
                    let released = self.registry.lock().await.release_record(id).is_some();
                    if released {
                        (self.on_change)();
                        Ok(())
                    } else {
                        Err(format!("no inhibitor with id {id}"))
                    }
                }
                _ => Err(format!("unknown command: {name}")),
            }
        })
    }
}
