use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::services::{
    framework::{Control, ServiceCommand, ServiceHandle},
    geoclue,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocationError {
    Unavailable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Coordinates {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Unknown,
    Ready(Coordinates),
    Refreshing,
    Degraded(LocationError),
}

#[derive(Debug, Clone)]
pub enum Command {
    Refresh,
    SetManual(f64, f64),
}

pub type LocationHandle = ServiceHandle<State, Command>;

pub struct LocationService {
    geoclue: geoclue::GeoClueHandle,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

struct ActiveProvider {
    task: JoinHandle<()>,
    cancel: CancellationToken,
    command_tx: mpsc::Sender<ProviderCommand>,
    message_rx: mpsc::Receiver<ProviderMessage>,
}

impl ActiveProvider {
    async fn cancel(self) {
        self.cancel.cancel();
        let _ = self.task.await;
    }

    async fn send(&self, command: ProviderCommand) {
        if let Err(e) = self.command_tx.send(command).await {
            tracing::error!("failed to send message to location provider: {:?}", e);
        }
    }

    fn spawn(geoclue: geoclue::GeoClueHandle) -> Self {
        let (command_tx, command_rx) = mpsc::channel(1);
        let (message_tx, message_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            geoclue_provider(geoclue, command_rx, message_tx, task_cancel).await;
        });
        Self {
            cancel,
            task,
            message_rx,
            command_tx,
        }
    }
}

enum Lifecycle {
    Idle,
    Running(ActiveProvider),
}

impl LocationService {
    pub fn new(geoclue: geoclue::GeoClueHandle) -> (Self, LocationHandle) {
        let (state_tx, state_rx) = watch::channel(State::Unknown);
        let (command_tx, command_rx) = mpsc::channel(4);

        (
            Self {
                geoclue,
                state_tx,
                command_rx,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub fn new_standalone() -> (Self, LocationHandle) {
        Self::new(inactive_geoclue_handle())
    }

    fn change_state(&self, state: State) -> bool {
        self.state_tx.send_if_modified(|current| {
            if *current == state {
                false
            } else {
                *current = state;
                true
            }
        })
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        tracing::debug!("location service started");
        let mut lifecycle = Lifecycle::Idle;

        loop {
            lifecycle = tokio::select! {
                _ = cancel.cancelled() => {
                    shutdown_task(lifecycle).await;
                    break;
                },
                provider_message = recv_from_provider(&mut lifecycle) => match provider_message {
                    Some(provider_message) => match provider_message {
                        ProviderMessage::Value(coordinates) => {
                            if self.change_state(State::Ready(coordinates.clone())) {
                                tracing::debug!("location received: {:?}", coordinates);
                            }
                            lifecycle
                        },
                        ProviderMessage::Unavailable(err) => {
                            if self.change_state(State::Degraded(err.clone())) {
                                tracing::debug!("location service degraded: {:?}", err);
                            }
                            lifecycle
                        },
                    },
                    None => {
                        tracing::debug!("location provider stopped");
                        self.change_state(State::Degraded(LocationError::Unavailable));
                        Lifecycle::Idle
                    }
                },
                command_message = self.command_rx.recv() => match command_message{
                    Some(command_message) => match command_message{
                        ServiceCommand::Control(control_command) => match control_command {
                            Control::Start(_) => {
                                tracing::debug!("start location provider: geoclue");
                                match lifecycle {
                                    Lifecycle::Running(_) => lifecycle,
                                    Lifecycle::Idle => Lifecycle::Running(ActiveProvider::spawn(self.geoclue.clone())),
                                }
                            },
                            Control::Reconfigure(_) => lifecycle,
                            Control::Shutdown => {
                                tracing::debug!("location service shutting down");
                                shutdown_task(lifecycle).await;
                                break;
                            },
                        },
                        ServiceCommand::Command(service_command) => match service_command {
                            Command::Refresh => {
                                self.change_state(State::Refreshing);
                                match lifecycle {
                                    Lifecycle::Running(ref provider) => {
                                        provider.send(ProviderCommand::Refresh).await;
                                        lifecycle
                                    },
                                    Lifecycle::Idle => {
                                        // After SetManual the provider was shut down. Refresh restores
                                        // the GeoClue provider so the user can go back to live coords.
                                        Lifecycle::Running(ActiveProvider::spawn(self.geoclue.clone()))
                                    },
                                }
                            },
                            Command::SetManual(lat, lon) => {
                                tracing::info!(lat, lon, "location overridden via IPC");
                                shutdown_task(lifecycle).await;
                                self.change_state(State::Ready(Coordinates { latitude: lat, longitude: lon }));
                                Lifecycle::Idle
                            },
                        },
                    },
                    None => {
                        tracing::debug!("command channel closed");
                        break
                    }
                }
            };
        }

        tracing::debug!("location service quit");
    }
}

fn inactive_geoclue_handle() -> geoclue::GeoClueHandle {
    let (_state_tx, state_rx) = watch::channel(geoclue::State::default());
    let (command_tx, _command_rx) = mpsc::channel(1);
    ServiceHandle::new(state_rx, command_tx)
}

async fn shutdown_task(state: Lifecycle) {
    if let Lifecycle::Running(task) = state {
        task.cancel().await;
    }
}

enum ProviderCommand {
    Refresh,
}
enum ProviderMessage {
    Value(Coordinates),
    Unavailable(LocationError),
}

async fn geoclue_provider(
    geoclue: geoclue::GeoClueHandle,
    mut command_rx: mpsc::Receiver<ProviderCommand>,
    message_tx: mpsc::Sender<ProviderMessage>,
    cancel: CancellationToken,
) {
    let mut state_rx = geoclue.subscribe();
    let state = { state_rx.borrow().clone() };
    if !publish_geoclue_location(state, &message_tx, &cancel).await {
        return;
    }

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            changed = state_rx.changed() => {
                if changed.is_err() {
                    let _ = send_provider_message(
                        &message_tx,
                        ProviderMessage::Unavailable(LocationError::Unavailable),
                        &cancel,
                    )
                    .await;
                    break;
                }
                let state = { state_rx.borrow().clone() };
                if !publish_geoclue_location(state, &message_tx, &cancel).await {
                    break;
                }
            }
            command = command_rx.recv() => match command {
                Some(ProviderCommand::Refresh) => {
                    // Forward to GeoClueService so it starts a new client and
                    // fetches a fresh fix. The new state arrives via
                    // state_rx.changed() and is published from the arm above.
                    let _ = geoclue
                        .send(ServiceCommand::Command(geoclue::Command::Refresh))
                        .await;
                }
                None => break,
            }
        }
    }
}

async fn publish_geoclue_location(
    state: geoclue::State,
    message_tx: &mpsc::Sender<ProviderMessage>,
    cancel: &CancellationToken,
) -> bool {
    if let Some(coordinates) = &state.coordinates {
        send_provider_message(
            message_tx,
            ProviderMessage::Value(Coordinates {
                latitude: coordinates.latitude,
                longitude: coordinates.longitude,
            }),
            cancel,
        )
        .await
    } else if !state.available || state.error.is_some() {
        send_provider_message(
            message_tx,
            ProviderMessage::Unavailable(LocationError::Unavailable),
            cancel,
        )
        .await
    } else {
        true
    }
}

async fn send_provider_message(
    message_tx: &mpsc::Sender<ProviderMessage>,
    message: ProviderMessage,
    cancel: &CancellationToken,
) -> bool {
    tokio::select! {
        _ = cancel.cancelled() => false,
        result = message_tx.send(message) => result.is_ok(),
    }
}

async fn recv_from_provider(state: &mut Lifecycle) -> Option<ProviderMessage> {
    match state {
        Lifecycle::Running(provider) => provider.message_rx.recv().await,
        _ => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, LocationService, State};
    use crate::{
        Config,
        services::framework::{Control, ServiceCommand},
    };
    use tokio::time::{Duration, timeout};
    use tokio_util::sync::CancellationToken;

    async fn wait_for_state(
        rx: &mut tokio::sync::watch::Receiver<State>,
        matches: impl Fn(&State) -> bool,
    ) -> State {
        timeout(Duration::from_secs(1), async {
            loop {
                if matches(&rx.borrow()) {
                    return rx.borrow().clone();
                }
                rx.changed().await.expect("location service should run");
            }
        })
        .await
        .expect("timed out waiting for location state")
    }

    #[tokio::test]
    async fn refresh_recovers_after_standalone_provider_stops() {
        let (service, handle) = LocationService::new_standalone();
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            service.run(task_cancel).await;
        });
        let mut rx = handle.subscribe();

        handle
            .send(ServiceCommand::Control(Control::Start(Config::default())))
            .await
            .expect("start location service");

        wait_for_state(&mut rx, |state| matches!(state, State::Degraded(_))).await;

        handle
            .send(ServiceCommand::Command(Command::SetManual(52.0, 21.0)))
            .await
            .expect("set manual location");
        wait_for_state(&mut rx, |state| matches!(state, State::Ready(_))).await;

        handle
            .send(ServiceCommand::Command(Command::Refresh))
            .await
            .expect("refresh location service");

        timeout(Duration::from_secs(1), async {
            loop {
                if matches!(handle.snapshot(), State::Degraded(_)) {
                    break;
                }
                rx.changed().await.expect("location service should run");
            }
        })
        .await
        .expect("refresh should settle back to degraded");

        cancel.cancel();
        timeout(Duration::from_secs(1), task)
            .await
            .expect("location task should exit after cancellation")
            .expect("location task should not panic");
    }
}
