use futures_util::StreamExt;
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{Duration, sleep},
};
use tokio_util::sync::CancellationToken;
use zbus::zvariant::OwnedObjectPath;

use crate::{
    dbus::geoclue::{GeoClueClientProxy, GeoClueLocationProxy, GeoClueManagerProxy},
    services::framework::{Control, ServiceCommand, ServiceHandle},
};

const COMMAND_QUEUE_SIZE: usize = 4;
const RETRY_DELAY: Duration = Duration::from_secs(5);
const DESKTOP_ID: &str = "glimpse-shell";
const EXACT_ACCURACY: u32 = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct Coordinates {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub available: bool,
    pub in_use: bool,
    pub coordinates: Option<Coordinates>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Refresh,
}

pub type GeoClueHandle = ServiceHandle<State, Command>;

pub struct GeoClueService {
    conn: zbus::Connection,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
    location_tx: mpsc::Sender<()>,
    location_rx: mpsc::Receiver<()>,
}

struct ActiveClient {
    path: OwnedObjectPath,
    proxy: GeoClueClientProxy<'static>,
    location_task: JoinHandle<()>,
}

enum RunOutcome {
    Cancelled,
    RetryAfterDelay,
}

impl GeoClueService {
    pub fn new(conn: zbus::Connection) -> (Self, GeoClueHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);
        let (location_tx, location_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                conn,
                state_tx,
                command_rx,
                location_tx,
                location_rx,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        loop {
            let outcome = match self.run_inner(cancel.clone()).await {
                Ok(outcome) => outcome,
                Err(error) => {
                    tracing::warn!(%error, "geoclue service failed");
                    self.change_state(State {
                        error: Some(error.to_string()),
                        ..State::default()
                    });
                    RunOutcome::RetryAfterDelay
                }
            };

            match outcome {
                RunOutcome::Cancelled => break,
                RunOutcome::RetryAfterDelay => {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        _ = sleep(RETRY_DELAY) => {}
                    }
                }
            }
        }
    }

    async fn run_inner(&mut self, cancel: CancellationToken) -> anyhow::Result<RunOutcome> {
        let manager = GeoClueManagerProxy::new(&self.conn).await?;
        let mut in_use_changes = manager.receive_in_use_changed().await;
        let mut active = None;
        self.publish_manager_state(&manager, &active).await;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    self.stop_client(active.take()).await;
                    return Ok(RunOutcome::Cancelled);
                }
                change = in_use_changes.next() => match change {
                    Some(_) => self.publish_manager_state(&manager, &active).await,
                    None => {
                        self.stop_client(active.take()).await;
                        return Ok(RunOutcome::RetryAfterDelay);
                    }
                },
                location = self.location_rx.recv() => {
                    if location.is_some() {
                        self.publish_manager_state(&manager, &active).await;
                        // Got a fix — release our client so we don't pin
                        // Manager.InUse=true forever (which would keep the
                        // privacy indicator stuck on). Subsequent refreshes
                        // restart the client transiently.
                        if self.state_tx.borrow().coordinates.is_some() && active.is_some() {
                            self.stop_client(active.take()).await;
                            self.publish_manager_state(&manager, &active).await;
                        }
                    }
                }
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => {
                        self.stop_client(active.take()).await;
                        return Ok(RunOutcome::Cancelled);
                    }
                    Some(ServiceCommand::Control(Control::Start(_)))
                    | Some(ServiceCommand::Control(Control::Reconfigure(_)))
                    | Some(ServiceCommand::Command(Command::Refresh)) => {
                        if self.reconcile_client(&manager, &mut active, true).await {
                            self.publish_manager_state(&manager, &active).await;
                        }
                    }
                }
            }
        }
    }

    async fn reconcile_client(
        &self,
        manager: &GeoClueManagerProxy<'_>,
        active: &mut Option<ActiveClient>,
        should_run: bool,
    ) -> bool {
        match (should_run, active.is_some()) {
            (true, false) => match self.start_client(manager).await {
                Ok(client) => {
                    *active = Some(client);
                    true
                }
                Err(error) => {
                    tracing::warn!(%error, "failed to start geoclue client");
                    self.set_error(error.to_string());
                    false
                }
            },
            (false, true) => {
                self.stop_client(active.take()).await;
                true
            }
            _ => true,
        }
    }

    async fn start_client(
        &self,
        manager: &GeoClueManagerProxy<'_>,
    ) -> anyhow::Result<ActiveClient> {
        let path = match manager.get_client().await {
            Ok(path) => path,
            Err(_) => manager.create_client().await?,
        };
        let proxy = GeoClueClientProxy::builder(&self.conn)
            .path(path.clone())?
            .build()
            .await?;
        let mut location_changes = proxy.receive_location_changed().await;
        proxy.set_desktop_id(DESKTOP_ID).await?;
        proxy.set_requested_accuracy_level(EXACT_ACCURACY).await?;
        proxy.start().await?;
        let location_tx = self.location_tx.clone();
        let location_task = tokio::spawn(async move {
            while location_changes.next().await.is_some() {
                if location_tx.send(()).await.is_err() {
                    break;
                }
            }
        });

        Ok(ActiveClient {
            path,
            proxy,
            location_task,
        })
    }

    async fn stop_client(&self, active: Option<ActiveClient>) {
        let Some(active) = active else {
            return;
        };
        let ActiveClient {
            path,
            proxy,
            location_task,
        } = active;
        location_task.abort();
        let _ = location_task.await;

        if let Err(error) = proxy.stop().await {
            tracing::debug!(%error, "failed to stop geoclue client");
        }
        if let Ok(manager) = GeoClueManagerProxy::new(&self.conn).await {
            if let Err(error) = manager.delete_client(path).await {
                tracing::debug!(%error, "failed to delete geoclue client");
            }
        }
    }

    async fn publish_manager_state(
        &self,
        manager: &GeoClueManagerProxy<'_>,
        active: &Option<ActiveClient>,
    ) {
        let in_use = manager.in_use().await.unwrap_or(false);
        // When we don't have an active client, keep the last known coordinates
        // around instead of clearing them — clients depend on the cached fix
        // between transient client lifetimes.
        let coordinates = match active {
            Some(active) => self.read_coordinates(active).await,
            None => self.state_tx.borrow().coordinates.clone(),
        };

        self.change_state(State {
            available: true,
            in_use,
            coordinates,
            error: None,
        });
    }

    async fn read_coordinates(&self, active: &ActiveClient) -> Option<Coordinates> {
        let path = active.proxy.location().await.ok()?;
        if path.as_str() == "/" {
            return None;
        }
        let location = GeoClueLocationProxy::builder(&self.conn)
            .path(path.as_str())
            .ok()?
            .build()
            .await
            .ok()?;
        Some(Coordinates {
            latitude: location.latitude().await.ok()?,
            longitude: location.longitude().await.ok()?,
        })
    }

    fn set_error(&self, error: String) {
        self.state_tx.send_if_modified(|state| {
            if state.error.as_deref() == Some(error.as_str()) {
                false
            } else {
                state.error = Some(error);
                true
            }
        });
    }

    fn change_state(&self, state: State) {
        self.state_tx.send_if_modified(|current| {
            if *current == state {
                false
            } else {
                *current = state;
                true
            }
        });
    }
}
