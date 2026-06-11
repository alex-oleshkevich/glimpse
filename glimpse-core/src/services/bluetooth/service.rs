#![allow(dead_code)]

use std::time::Duration;

use anyhow::{Context, anyhow};
use tokio::{
    sync::{mpsc, watch},
    time::{sleep, timeout},
};
use tokio_util::sync::CancellationToken;

use crate::services::framework::{Control, ServiceCommand, ServiceHandle};

use super::{
    BluetoothActiveAction, BluetoothServiceHealth, BluezClient, BluezEvent, Command, State,
};

const COMMAND_QUEUE_SIZE: usize = 16;
const EVENT_QUEUE_SIZE: usize = 32;
const RETRY_DELAY: Duration = Duration::from_secs(2);
const DBUS_TIMEOUT: Duration = Duration::from_secs(5);
const DEVICE_OP_TIMEOUT: Duration = Duration::from_secs(30);
const PAIR_TIMEOUT: Duration = Duration::from_secs(60);

pub type BluetoothHandle = ServiceHandle<State, Command>;

pub struct BluetoothService {
    client: BluezClient,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

enum RunOutcome {
    Cancelled,
    RetryAfterDelay,
}

impl BluetoothService {
    pub fn new(conn: zbus::Connection) -> (Self, BluetoothHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                client: BluezClient::new(conn),
                state_tx,
                command_rx,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        let mut reconnect_attempt = 0;

        loop {
            let outcome = match self.run_inner(cancel.clone()).await {
                Ok(outcome) => {
                    reconnect_attempt = 0;
                    outcome
                }
                Err(error) => {
                    reconnect_attempt += 1;
                    tracing::warn!(error = %error, "bluetooth service failed");
                    self.update_state(|state| {
                        state.health = BluetoothServiceHealth::Reconnecting {
                            attempt: reconnect_attempt,
                        };
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
        tracing::debug!("bluetooth service started");
        self.refresh_snapshot()
            .await
            .context("failed to load initial bluetooth snapshot")?;

        let (event_tx, mut event_rx) = mpsc::channel(EVENT_QUEUE_SIZE);
        let listener_cancel = CancellationToken::new();
        let listener = spawn_bluez_listener(self.client.clone(), event_tx, listener_cancel.clone());
        let mut device_op: Option<tokio::task::JoinHandle<anyhow::Result<bool>>> = None;

        let outcome = loop {
            tokio::select! {
                _ = cancel.cancelled() => break Ok(RunOutcome::Cancelled),
                event = event_rx.recv() => match event {
                    Some(BluezEvent::Changed { reason }) => {
                        tracing::debug!(reason = %reason, "bluetooth: refreshing service state");
                        if let Err(error) = self.refresh_snapshot().await {
                            tracing::warn!(error = %error, "bluetooth: refresh failed after change event");
                            self.set_degraded("Bluetooth data is stale");
                        }
                    }
                    None => break Err(anyhow!("bluetooth event listener stopped")),
                },
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Command(command)) => {
                        if is_device_operation(&command) {
                            let action = active_action_for(&command);
                            if action.is_some() && self.state_tx.borrow().active_action.is_some() {
                                tracing::warn!("bluetooth: command ignored while another action is active");
                            } else {
                                if let Some(action) = action {
                                    self.update_state(|state| state.active_action = Some(action));
                                }
                                let client = self.client.clone();
                                device_op = Some(tokio::spawn(run_device_operation(client, command)));
                            }
                        } else if self.execute_command(command).await {
                            if let Err(error) = self.refresh_snapshot().await {
                                tracing::warn!(error = %error, "bluetooth: refresh failed after command");
                                self.set_degraded("Bluetooth data is stale");
                            }
                        }
                    }
                    Some(ServiceCommand::Control(control)) => match control {
                        Control::Start(_) | Control::Reconfigure(_) => {}
                        Control::Shutdown => break Ok(RunOutcome::Cancelled),
                    },
                    None => break Ok(RunOutcome::Cancelled),
                },
                result = async { device_op.as_mut().unwrap().await }, if device_op.is_some() => {
                    device_op = None;
                    self.update_state(|state| state.active_action = None);
                    let should_refresh = match result {
                        Ok(Ok(r)) => r,
                        Ok(Err(error)) => {
                            tracing::warn!(error = %error, "bluetooth command failed");
                            true
                        }
                        Err(error) if error.is_cancelled() => false,
                        Err(error) => {
                            tracing::warn!(error = %error, "bluetooth: device op task failed");
                            true
                        }
                    };
                    if should_refresh {
                        if let Err(error) = self.refresh_snapshot().await {
                            tracing::warn!(error = %error, "bluetooth: refresh failed after command");
                            self.set_degraded("Bluetooth data is stale");
                        }
                    }
                }
            }
        };

        listener_cancel.cancel();
        let _ = listener.await;
        if let Some(op) = device_op {
            op.abort();
            self.update_state(|state| state.active_action = None);
        }

        outcome
    }

    async fn refresh_snapshot(&self) -> anyhow::Result<()> {
        let snapshot = timeout(DBUS_TIMEOUT, self.client.scan())
            .await
            .context("bluetooth: scan timed out")??;
        self.update_state(|state| {
            state.health = BluetoothServiceHealth::Ready;
            state.snapshot = snapshot;
        });
        Ok(())
    }

    async fn execute_command(&self, command: Command) -> bool {
        let action = active_action_for(&command);
        if action.is_some() && self.state_tx.borrow().active_action.is_some() {
            tracing::warn!("bluetooth: command ignored while another action is active");
            return false;
        }

        if let Some(action) = action.clone() {
            self.update_state(|state| {
                state.active_action = Some(action);
            });
        }

        let result = self.execute_client_command(command).await;

        if action.is_some() {
            self.update_state(|state| {
                state.active_action = None;
            });
        }

        match result {
            Ok(refresh) => refresh,
            Err(error) => {
                tracing::warn!(error = %error, "bluetooth command failed");
                true
            }
        }
    }

    async fn execute_client_command(&self, command: Command) -> anyhow::Result<bool> {
        match command {
            Command::SetPowered(powered) => {
                timeout(DBUS_TIMEOUT, self.client.set_powered(powered))
                    .await
                    .context("bluetooth: set_powered timed out")??;
                Ok(true)
            }
            Command::SetAdapterPowered {
                adapter_path,
                powered,
            } => {
                timeout(
                    DBUS_TIMEOUT,
                    self.client.set_adapter_powered(&adapter_path, powered),
                )
                .await
                .context("bluetooth: set_adapter_powered timed out")??;
                Ok(true)
            }
            Command::SetAdapterDiscoverable {
                adapter_path,
                discoverable,
            } => {
                timeout(
                    DBUS_TIMEOUT,
                    self.client
                        .set_adapter_discoverable(&adapter_path, discoverable),
                )
                .await
                .context("bluetooth: set_adapter_discoverable timed out")??;
                Ok(true)
            }
            Command::StartDiscovery => {
                timeout(DBUS_TIMEOUT, self.client.start_discovery())
                    .await
                    .context("bluetooth: start_discovery timed out")??;
                Ok(true)
            }
            Command::StopDiscovery => {
                timeout(DBUS_TIMEOUT, self.client.stop_discovery())
                    .await
                    .context("bluetooth: stop_discovery timed out")??;
                Ok(true)
            }
            _ => unreachable!("device operations are routed through run_device_operation"),
        }
    }

    fn set_degraded(&self, message: &str) {
        self.update_state(|state| {
            state.health = BluetoothServiceHealth::Degraded {
                message: message.into(),
            };
        });
    }

    fn update_state(&self, update: impl FnOnce(&mut State)) {
        let mut next = self.state_tx.borrow().clone();
        update(&mut next);
        if should_emit_state(&self.state_tx.borrow(), &next) {
            self.change_state(next);
        }
    }

    fn change_state(&self, state: State) {
        if let Err(error) = self.state_tx.send(state) {
            tracing::error!("failed to send new bluetooth state: {:?}", error);
        }
    }
}

fn is_device_operation(command: &Command) -> bool {
    matches!(
        command,
        Command::Connect { .. }
            | Command::Disconnect { .. }
            | Command::Pair { .. }
            | Command::Trust { .. }
            | Command::Forget { .. }
    )
}

async fn run_device_operation(client: BluezClient, command: Command) -> anyhow::Result<bool> {
    match command {
        Command::Connect { address } => {
            timeout(DEVICE_OP_TIMEOUT, client.connect(&address))
                .await
                .context("bluetooth: connect timed out")??;
            Ok(true)
        }
        Command::Disconnect { address } => {
            timeout(DEVICE_OP_TIMEOUT, client.disconnect(&address))
                .await
                .context("bluetooth: disconnect timed out")??;
            Ok(true)
        }
        Command::Pair { address } => {
            tracing::debug!(address = %address, "bluetooth: pair command started");
            timeout(PAIR_TIMEOUT, client.pair(&address))
                .await
                .context("bluetooth: pair timed out")??;
            tracing::debug!(address = %address, "bluetooth: pair command finished");
            Ok(true)
        }
        Command::Trust { address, trusted } => {
            tracing::debug!(address = %address, trusted, "bluetooth: trust command started");
            timeout(DBUS_TIMEOUT, client.trust(&address, trusted))
                .await
                .context("bluetooth: trust timed out")??;
            tracing::debug!(address = %address, trusted, "bluetooth: trust command finished");
            Ok(true)
        }
        Command::Forget { address } => {
            tracing::debug!(address = %address, "bluetooth: forget command started");
            timeout(DBUS_TIMEOUT, client.forget(&address))
                .await
                .context("bluetooth: forget timed out")??;
            tracing::debug!(address = %address, "bluetooth: forget command finished");
            Ok(true)
        }
        _ => unreachable!("is_device_operation should prevent reaching here"),
    }
}

fn spawn_bluez_listener(
    client: BluezClient,
    events: mpsc::Sender<BluezEvent>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(error) = client.listen(events, cancel).await {
            tracing::warn!(error = %error, "bluetooth listener failed");
        }
    })
}

fn active_action_for(command: &Command) -> Option<BluetoothActiveAction> {
    match command {
        Command::SetPowered(powered) => Some(BluetoothActiveAction::SetPowered(*powered)),
        Command::SetAdapterPowered {
            adapter_path,
            powered,
        } => Some(BluetoothActiveAction::SetAdapterPowered {
            adapter_path: adapter_path.clone(),
            powered: *powered,
        }),
        Command::SetAdapterDiscoverable {
            adapter_path,
            discoverable,
        } => Some(BluetoothActiveAction::SetAdapterDiscoverable {
            adapter_path: adapter_path.clone(),
            discoverable: *discoverable,
        }),
        Command::Connect { address } => Some(BluetoothActiveAction::Connect {
            address: address.clone(),
        }),
        Command::Disconnect { address } => Some(BluetoothActiveAction::Disconnect {
            address: address.clone(),
        }),
        Command::Pair { address } => Some(BluetoothActiveAction::Pair {
            address: address.clone(),
        }),
        Command::Trust { address, trusted } => Some(BluetoothActiveAction::Trust {
            address: address.clone(),
            trusted: *trusted,
        }),
        Command::Forget { address } => Some(BluetoothActiveAction::Forget {
            address: address.clone(),
        }),
        Command::StartDiscovery | Command::StopDiscovery => None,
    }
}

fn should_emit_state(previous: &State, next: &State) -> bool {
    previous != next
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::bluetooth::{
        BluetoothActiveAction, BluetoothServiceHealth, Command, State,
    };

    #[test]
    fn active_action_tracks_long_running_device_commands() {
        assert_eq!(
            active_action_for(&Command::Connect {
                address: "AA:BB".into()
            }),
            Some(BluetoothActiveAction::Connect {
                address: "AA:BB".into()
            })
        );
        assert_eq!(
            active_action_for(&Command::Trust {
                address: "AA:BB".into(),
                trusted: true,
            }),
            Some(BluetoothActiveAction::Trust {
                address: "AA:BB".into(),
                trusted: true,
            })
        );
    }

    #[test]
    fn discovery_commands_do_not_claim_active_action() {
        assert_eq!(active_action_for(&Command::StartDiscovery), None);
        assert_eq!(active_action_for(&Command::StopDiscovery), None);
    }

    #[test]
    fn should_emit_state_only_for_real_changes() {
        let previous = State::default();
        assert!(!should_emit_state(&previous, &previous));

        let mut next = previous.clone();
        next.health = BluetoothServiceHealth::Ready;
        assert!(should_emit_state(&previous, &next));
    }
}
