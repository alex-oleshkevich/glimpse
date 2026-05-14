use anyhow::{Result, anyhow};
use regex::Regex;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

use crate::{
    dbus::notifications as notification_dbus,
    services::framework::{Control, ServiceCommand, ServiceHandle},
};

use super::model::{ActiveAction, Command, Health, NotificationEntry, Signal, State};

const COMMAND_QUEUE_SIZE: usize = 32;
const SERVER_QUEUE_SIZE: usize = 256;

pub type NotificationsHandle = ServiceHandle<State, Command>;

pub struct NotificationsService {
    session: zbus::Connection,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
    server_rx: mpsc::Receiver<ServerRequest>,
    dispatcher: NotificationServerDispatcher,
    max_history: usize,
    filters: NotificationFilters,
}

pub(crate) enum ServerRequest {
    Inject {
        entry: NotificationEntry,
        reply: oneshot::Sender<Result<()>>,
    },
    Close {
        id: u32,
        reply: oneshot::Sender<Result<()>>,
    },
}

#[derive(Clone)]
pub(crate) struct NotificationServerDispatcher {
    requests: mpsc::Sender<ServerRequest>,
}

impl NotificationServerDispatcher {
    fn new(requests: mpsc::Sender<ServerRequest>) -> Self {
        Self { requests }
    }

    pub(crate) async fn inject(&self, entry: NotificationEntry) -> Result<()> {
        let (reply, response) = oneshot::channel();
        self.requests
            .send(ServerRequest::Inject { entry, reply })
            .await
            .map_err(|_| anyhow!("notifications service stopped"))?;
        response
            .await
            .map_err(|_| anyhow!("notifications service reply dropped"))?
    }

    pub(crate) async fn close(&self, id: u32) -> Result<()> {
        let (reply, response) = oneshot::channel();
        self.requests
            .send(ServerRequest::Close { id, reply })
            .await
            .map_err(|_| anyhow!("notifications service stopped"))?;
        response
            .await
            .map_err(|_| anyhow!("notifications service reply dropped"))?
    }
}

impl NotificationsService {
    pub fn new(session: zbus::Connection) -> (Self, NotificationsHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);
        let (server_tx, server_rx) = mpsc::channel(SERVER_QUEUE_SIZE);
        let dispatcher = NotificationServerDispatcher::new(server_tx);

        (
            Self {
                session,
                state_tx,
                command_rx,
                server_rx,
                dispatcher,
                max_history: 100,
                filters: NotificationFilters::default(),
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        let mut state = State::default();
        let signal_emitter = match notification_dbus::register_server(
            &self.session,
            self.dispatcher.clone(),
        )
        .await
        {
            Ok(emitter) => {
                state.health = Health::Ready;
                Some(emitter)
            }
            Err(error) => {
                tracing::warn!(%error, "failed to register notification server");
                state.health =
                    Health::Degraded(format!("failed to register notification server: {error}"));
                None
            }
        };
        self.publish(&state);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                request = self.server_rx.recv() => match request {
                    Some(request) => {
                        self.handle_server_request(&mut state, signal_emitter.as_ref(), request).await;
                        self.publish(&state);
                    }
                    None => break,
                },
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Command(command)) => {
                        self.handle_command(&mut state, signal_emitter.as_ref(), command).await;
                        self.publish(&state);
                    }
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => break,
                    Some(ServiceCommand::Control(Control::Start(_)))
                    | Some(ServiceCommand::Control(Control::Reconfigure(_))) => {}
                }
            }
        }

        notification_dbus::unregister_server(&self.session).await;
    }

    async fn handle_server_request(
        &self,
        state: &mut State,
        signal_emitter: Option<&zbus::object_server::SignalEmitter<'static>>,
        request: ServerRequest,
    ) {
        match request {
            ServerRequest::Inject { entry, reply } => {
                tracing::debug!(
                    id = entry.id,
                    app = %entry.app_name,
                    summary = %entry.summary,
                    "notification received"
                );
                if !store_notification_if_accepted(state, &self.filters, self.max_history, &entry) {
                    tracing::debug!(
                        id = entry.id,
                        app = %entry.app_name,
                        urgency = entry.urgency,
                        "notification ignored by policy"
                    );
                }
                let _ = reply.send(Ok(()));
            }
            ServerRequest::Close { id, reply } => {
                close_notification(self, state, signal_emitter, id, 3).await;
                let _ = reply.send(Ok(()));
            }
        }
    }

    async fn handle_command(
        &mut self,
        state: &mut State,
        signal_emitter: Option<&zbus::object_server::SignalEmitter<'static>>,
        command: Command,
    ) {
        if let Command::SetMaxHistory(max) = command {
            self.max_history = max;
            return;
        }
        if let Command::SetFilterRegex(rules) = command {
            self.filters = NotificationFilters::compile_lossy(rules);
            return;
        }

        state.active_action = Some(active_action_for(&command));
        self.publish(state);

        let result: Result<()> = match command {
            Command::Dismiss { id } => {
                close_notification(self, state, signal_emitter, id, 2).await;
                Ok(())
            }
            Command::DismissAll => {
                let ids = state
                    .notifications
                    .iter()
                    .map(|entry| entry.id)
                    .collect::<Vec<_>>();
                for id in ids {
                    close_notification(self, state, signal_emitter, id, 2).await;
                }
                Ok(())
            }
            Command::InvokeAction {
                id,
                action_key,
                activation_token,
            } => {
                if let Some(token) = activation_token {
                    self.emit_signal(state, signal_emitter, Signal::ActivationToken { id, token })
                        .await;
                }
                self.emit_signal(
                    state,
                    signal_emitter,
                    Signal::ActionInvoked {
                        id,
                        action_key: action_key.clone(),
                    },
                )
                .await;

                let resident = state
                    .notifications
                    .iter()
                    .find(|entry| entry.id == id)
                    .map(|entry| entry.resident)
                    .unwrap_or(false);
                if !resident {
                    close_notification(self, state, signal_emitter, id, 2).await;
                }
                Ok(())
            }
            Command::SetDnd(enabled) => {
                state.dnd = enabled;
                Ok(())
            }
            Command::SetMaxHistory(_) => unreachable!("handled before active_action tracking"),
            Command::SetFilterRegex(_) => unreachable!("handled before active_action tracking"),
        };

        if let Err(error) = result {
            tracing::warn!(%error, "notification command failed");
            state.health = Health::Degraded(error.to_string());
        }
        state.active_action = None;
    }

    async fn emit_signal(
        &self,
        state: &mut State,
        signal_emitter: Option<&zbus::object_server::SignalEmitter<'static>>,
        signal: Signal,
    ) {
        let Some(signal_emitter) = signal_emitter else {
            return;
        };
        if let Err(error) = notification_dbus::emit_signal(signal_emitter, &signal).await {
            tracing::warn!(%error, signal = signal_name(&signal), "failed to emit notification signal");
            state.health = Health::Degraded(format!(
                "failed to emit notification signal {}: {error}",
                signal_name(&signal)
            ));
        }
    }

    fn publish(&self, next: &State) {
        self.state_tx
            .send_if_modified(|state| set_if_changed(state, next.clone()));
    }
}

async fn close_notification(
    service: &NotificationsService,
    state: &mut State,
    signal_emitter: Option<&zbus::object_server::SignalEmitter<'static>>,
    id: u32,
    reason: u32,
) {
    let Some(index) = state.notifications.iter().position(|entry| entry.id == id) else {
        return;
    };
    state.notifications.remove(index);
    service
        .emit_signal(
            state,
            signal_emitter,
            Signal::NotificationClosed { id, reason },
        )
        .await;
}

fn upsert_notification(notifications: &mut Vec<NotificationEntry>, entry: NotificationEntry) {
    if let Some(index) = notifications
        .iter()
        .position(|existing| existing.id == entry.id)
    {
        notifications[index] = entry;
    } else {
        notifications.insert(0, entry);
    }
}

fn store_notification_if_accepted(
    state: &mut State,
    filters: &NotificationFilters,
    max_history: usize,
    entry: &NotificationEntry,
) -> bool {
    if !accepts_notification_with_filters(state, filters, entry) {
        return false;
    }

    upsert_notification(&mut state.notifications, entry.clone());
    enforce_history_limit(&mut state.notifications, max_history);
    true
}

fn accepts_notification_with_filters(
    state: &State,
    filters: &NotificationFilters,
    entry: &NotificationEntry,
) -> bool {
    if entry.urgency == 2 {
        return true;
    }
    if state.dnd {
        return false;
    }
    !filters.matches(entry)
}

#[derive(Debug, Default)]
struct NotificationFilters {
    rules: Vec<Regex>,
}

impl NotificationFilters {
    #[cfg(test)]
    fn compile<I, S>(rules: I) -> std::result::Result<Self, regex::Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        rules
            .into_iter()
            .map(|rule| Regex::new(rule.as_ref()))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map(|rules| Self { rules })
    }

    fn compile_lossy<I, S>(rules: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let rules = rules
            .into_iter()
            .filter_map(|rule| match Regex::new(rule.as_ref()) {
                Ok(regex) => Some(regex),
                Err(error) => {
                    tracing::warn!(
                        rule = rule.as_ref(),
                        %error,
                        "invalid notification filter regex; skipping rule"
                    );
                    None
                }
            })
            .collect();
        Self { rules }
    }

    fn matches(&self, entry: &NotificationEntry) -> bool {
        self.rules.iter().any(|rule| {
            rule.is_match(&entry.summary)
                || rule.is_match(&entry.body)
                || rule.is_match(&entry.app_name)
        })
    }
}

fn active_action_for(command: &Command) -> ActiveAction {
    match command {
        Command::Dismiss { id } => ActiveAction::Dismiss { id: *id },
        Command::DismissAll => ActiveAction::DismissAll,
        Command::InvokeAction { id, action_key, .. } => ActiveAction::InvokeAction {
            id: *id,
            action_key: action_key.clone(),
        },
        Command::SetDnd(enabled) => ActiveAction::SetDnd(*enabled),
        Command::SetMaxHistory(_) => unreachable!("handled before active_action tracking"),
        Command::SetFilterRegex(_) => unreachable!("handled before active_action tracking"),
    }
}

fn signal_name(signal: &Signal) -> &'static str {
    match signal {
        Signal::NotificationClosed { .. } => "NotificationClosed",
        Signal::ActionInvoked { .. } => "ActionInvoked",
        Signal::ActivationToken { .. } => "ActivationToken",
    }
}

fn enforce_history_limit(notifications: &mut Vec<NotificationEntry>, max: usize) {
    if max == 0 || notifications.len() <= max {
        return;
    }
    let mut i = notifications.len();
    while notifications.len() > max && i > 0 {
        i -= 1;
        if notifications[i].urgency < 2 {
            notifications.remove(i);
        }
    }
}

fn set_if_changed<T: PartialEq>(target: &mut T, next: T) -> bool {
    if target == &next {
        false
    } else {
        *target = next;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::notifications::model::NotificationAction;

    fn notification(id: u32, summary: &str) -> NotificationEntry {
        NotificationEntry {
            id,
            app_name: "App".into(),
            app_icon: String::new(),
            desktop_entry: None,
            summary: summary.into(),
            body: String::new(),
            urgency: 1,
            actions: vec![NotificationAction {
                key: "default".into(),
                label: "Open".into(),
            }],
            image: None,
            timestamp: id as u64,
            resident: false,
        }
    }

    #[test]
    fn upsert_replaces_existing_notification_and_keeps_newest_first() {
        let mut notifications = vec![notification(1, "old")];

        upsert_notification(&mut notifications, notification(2, "new"));
        upsert_notification(&mut notifications, notification(1, "updated"));

        assert_eq!(notifications.len(), 2);
        assert_eq!(notifications[0].id, 2);
        assert_eq!(notifications[1].summary, "updated");
    }

    #[test]
    fn active_action_tracks_command_kind() {
        assert_eq!(
            active_action_for(&Command::InvokeAction {
                id: 9,
                action_key: "default".into(),
                activation_token: None,
            }),
            ActiveAction::InvokeAction {
                id: 9,
                action_key: "default".into(),
            }
        );
    }

    #[test]
    fn dnd_accepts_only_critical_notifications() {
        let state = State {
            dnd: true,
            ..State::default()
        };
        let normal = notification(1, "normal");
        let mut critical = notification(2, "critical");
        critical.urgency = 2;

        assert!(!accepts_notification_with_filters(
            &state,
            &NotificationFilters::default(),
            &normal
        ));
        assert!(accepts_notification_with_filters(
            &state,
            &NotificationFilters::default(),
            &critical
        ));
    }

    #[test]
    fn filter_rejects_non_critical_notification_when_title_matches() {
        let filters = NotificationFilters::compile(["Build succeeded"]).expect("valid regex");
        let notification = notification(1, "Build succeeded");

        assert!(!accepts_notification_with_filters(
            &State::default(),
            &filters,
            &notification
        ));
    }

    #[test]
    fn filter_rejects_non_critical_notification_when_body_matches() {
        let filters = NotificationFilters::compile(["sync complete"]).expect("valid regex");
        let mut notification = notification(1, "Backup");
        notification.body = "Nightly sync complete".into();

        assert!(!accepts_notification_with_filters(
            &State::default(),
            &filters,
            &notification
        ));
    }

    #[test]
    fn filter_rejects_non_critical_notification_when_app_matches() {
        let filters = NotificationFilters::compile(["(?i)^discord$"]).expect("valid regex");
        let mut notification = notification(1, "Message");
        notification.app_name = "Discord".into();

        assert!(!accepts_notification_with_filters(
            &State::default(),
            &filters,
            &notification
        ));
    }

    #[test]
    fn filter_keeps_critical_notification_even_when_rule_matches() {
        let filters = NotificationFilters::compile(["urgent spam"]).expect("valid regex");
        let mut notification = notification(1, "urgent spam");
        notification.urgency = 2;

        assert!(accepts_notification_with_filters(
            &State::default(),
            &filters,
            &notification
        ));
    }

    #[test]
    fn filtered_notification_is_not_kept_in_memory() {
        let filters = NotificationFilters::compile(["Build succeeded"]).expect("valid regex");
        let mut state = State::default();

        let notification = notification(1, "Build succeeded");
        let rejected = store_notification_if_accepted(&mut state, &filters, 100, &notification);

        assert!(!rejected);
        assert!(state.notifications.is_empty());
    }

    #[test]
    fn enforce_history_limit_removes_oldest_non_critical_first() {
        let mut notifications = vec![
            notification(3, "newest"),
            notification(2, "middle"),
            notification(1, "oldest"),
        ];
        enforce_history_limit(&mut notifications, 2);
        assert_eq!(notifications.len(), 2);
        assert_eq!(notifications[0].id, 3);
        assert_eq!(notifications[1].id, 2);
    }

    #[test]
    fn enforce_history_limit_preserves_critical_notifications() {
        let mut critical = notification(1, "oldest critical");
        critical.urgency = 2;
        let mut notifications = vec![
            notification(3, "newest"),
            notification(2, "middle"),
            critical,
        ];
        enforce_history_limit(&mut notifications, 2);
        assert_eq!(notifications.len(), 2);
        assert!(notifications.iter().any(|n| n.id == 1));
        assert!(notifications.iter().any(|n| n.id == 3));
    }

    #[test]
    fn enforce_history_limit_zero_means_unlimited() {
        let mut notifications: Vec<NotificationEntry> =
            (1..=200).map(|i| notification(i, "n")).collect();
        enforce_history_limit(&mut notifications, 0);
        assert_eq!(notifications.len(), 200);
    }

    #[test]
    fn enforce_history_limit_noop_when_within_limit() {
        let mut notifications = vec![notification(1, "a"), notification(2, "b")];
        enforce_history_limit(&mut notifications, 5);
        assert_eq!(notifications.len(), 2);
    }
}
