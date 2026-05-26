use std::collections::{BTreeSet, VecDeque};

use chrono::Local;
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{Instant, sleep},
};
use tokio_util::sync::CancellationToken;

use crate::{
    CalendarConfig,
    services::framework::{Control, ServiceCommand, ServiceHandle},
};

use super::{
    aggregate::CalendarAggregator,
    model::{CalendarMonthSnapshot, Command, Health, MonthKey, State},
};

const COMMAND_QUEUE_SIZE: usize = 16;

pub type CalendarEventsHandle = ServiceHandle<State, Command>;

pub struct CalendarEventsService {
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

#[derive(Debug)]
struct MonthLoad {
    result: anyhow::Result<CalendarMonthSnapshot>,
}

impl CalendarEventsService {
    pub fn new(_session: zbus::Connection) -> (Self, CalendarEventsHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                state_tx,
                command_rx,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        let mut aggregator = CalendarAggregator::new(CalendarConfig::default());
        let mut pending = VecDeque::new();
        let mut queued = BTreeSet::new();
        let mut inflight: Option<(MonthKey, JoinHandle<MonthLoad>)> = None;
        let mut active_months = BTreeSet::new();
        let refresh = sleep(aggregator.poll_interval());
        tokio::pin!(refresh);

        self.set_preload_window(
            MonthKey::from_date(Local::now().date_naive()),
            &mut active_months,
            &mut pending,
            &mut queued,
        );
        start_next_load(
            &aggregator,
            &mut pending,
            &mut queued,
            &mut inflight,
            &self.state_tx,
        );

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Command(Command::PreloadAround(month))) => {
                        self.set_preload_window(month, &mut active_months, &mut pending, &mut queued);
                        abort_inactive_load(&active_months, &mut inflight);
                        start_next_load(&aggregator, &mut pending, &mut queued, &mut inflight, &self.state_tx);
                    }
                    Some(ServiceCommand::Command(Command::Refresh)) => {
                        self.queue_active_months(&active_months, &mut pending, &mut queued);
                        start_next_load(&aggregator, &mut pending, &mut queued, &mut inflight, &self.state_tx);
                    }
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => break,
                    Some(ServiceCommand::Control(Control::Start(config)))
                    | Some(ServiceCommand::Control(Control::Reconfigure(config))) => {
                        aggregator.reconfigure(config.calendar);
                        refresh.as_mut().reset(Instant::now() + aggregator.poll_interval());
                        abort_load(&mut inflight, &self.state_tx);
                        self.queue_active_months(&active_months, &mut pending, &mut queued);
                        start_next_load(&aggregator, &mut pending, &mut queued, &mut inflight, &self.state_tx);
                    }
                },
                _ = &mut refresh => {
                    self.queue_active_months(&active_months, &mut pending, &mut queued);
                    start_next_load(&aggregator, &mut pending, &mut queued, &mut inflight, &self.state_tx);
                    refresh.as_mut().reset(Instant::now() + aggregator.poll_interval());
                }
                loaded = async {
                    let Some((_, task)) = inflight.as_mut() else {
                        return None;
                    };
                    Some(task.await)
                }, if inflight.is_some() => {
                    let loaded_key = inflight.take().map(|(key, _)| key);
                    if let Some(key) = loaded_key {
                        self.state_tx.send_if_modified(|state| state.loading_months.remove(&key));
                    }
                    if let Some(key) = loaded_key.filter(|key| active_months.contains(key)) {
                        match loaded {
                            Some(Ok(MonthLoad { result: Ok(month), .. })) => {
                                self.publish_month(key, month);
                            }
                            Some(Ok(MonthLoad { result: Err(error), .. })) => {
                                tracing::warn!(%error, ?key, "failed to load calendar month");
                                self.publish_health(Health::Degraded(error.to_string()));
                            }
                            Some(Err(error)) => {
                                tracing::warn!(%error, "calendar month loader task failed");
                                self.publish_health(Health::Degraded(format!("calendar loader task failed: {error}")));
                            }
                            None => {}
                        }
                    }
                    start_next_load(&aggregator, &mut pending, &mut queued, &mut inflight, &self.state_tx);
                }
            }
        }

        if let Some((_, task)) = inflight {
            task.abort();
        }
    }

    fn set_preload_window(
        &self,
        month: MonthKey,
        active_months: &mut BTreeSet<MonthKey>,
        pending: &mut VecDeque<MonthKey>,
        queued: &mut BTreeSet<MonthKey>,
    ) {
        active_months.clear();
        active_months.extend(preload_window(month));
        pending.retain(|key| active_months.contains(key));
        queued.retain(|key| active_months.contains(key));
        self.evict_inactive_months(active_months);

        for key in active_months.iter().copied() {
            queue_month(&self.state_tx, pending, queued, key, false);
        }
    }

    fn queue_active_months(
        &self,
        active_months: &BTreeSet<MonthKey>,
        pending: &mut VecDeque<MonthKey>,
        queued: &mut BTreeSet<MonthKey>,
    ) {
        for key in active_months.iter().copied() {
            queue_month(&self.state_tx, pending, queued, key, true);
        }
    }

    fn evict_inactive_months(&self, active_months: &BTreeSet<MonthKey>) {
        self.state_tx.send_if_modified(|state| {
            let before_months = state.month_cache.len();
            let before_loading = state.loading_months.len();
            state
                .month_cache
                .retain(|key, _| active_months.contains(key));
            state
                .loading_months
                .retain(|key| active_months.contains(key));
            state.month_cache.len() != before_months || state.loading_months.len() != before_loading
        });
    }

    fn publish_health(&self, health: Health) {
        self.state_tx.send_if_modified(|state| {
            if state.health == health {
                false
            } else {
                state.health = health;
                true
            }
        });
    }

    fn publish_month(&self, key: MonthKey, month: CalendarMonthSnapshot) {
        self.state_tx.send_if_modified(|state| {
            state.health = Health::Ready;
            state.month_cache.insert(key, month);
            true
        });
    }
}

fn queue_month(
    state_tx: &watch::Sender<State>,
    pending: &mut VecDeque<MonthKey>,
    queued: &mut BTreeSet<MonthKey>,
    key: MonthKey,
    force: bool,
) {
    let state = state_tx.borrow();
    if state.loading_months.contains(&key)
        || (!force && state.month_cache.contains_key(&key))
        || !queued.insert(key)
    {
        return;
    }
    drop(state);
    pending.push_back(key);
}

fn start_next_load(
    aggregator: &CalendarAggregator,
    pending: &mut VecDeque<MonthKey>,
    queued: &mut BTreeSet<MonthKey>,
    inflight: &mut Option<(MonthKey, JoinHandle<MonthLoad>)>,
    state_tx: &watch::Sender<State>,
) {
    if inflight.is_some() {
        return;
    }
    let Some(key) = pending.pop_front() else {
        return;
    };
    queued.remove(&key);
    state_tx.send_if_modified(|state| {
        state.health = Health::Loading;
        state.loading_months.insert(key)
    });
    let aggregator = aggregator.clone();
    *inflight = Some((
        key,
        tokio::spawn(async move {
            let result = aggregator.load_month(key).await;
            MonthLoad { result }
        }),
    ));
}

fn abort_inactive_load(
    active_months: &BTreeSet<MonthKey>,
    inflight: &mut Option<(MonthKey, JoinHandle<MonthLoad>)>,
) {
    if inflight
        .as_ref()
        .is_some_and(|(key, _)| !active_months.contains(key))
    {
        if let Some((_, task)) = inflight.take() {
            task.abort();
        }
    }
}

fn abort_load(
    inflight: &mut Option<(MonthKey, JoinHandle<MonthLoad>)>,
    state_tx: &watch::Sender<State>,
) {
    let Some((key, task)) = inflight.take() else {
        return;
    };
    task.abort();
    state_tx.send_if_modified(|state| state.loading_months.remove(&key));
}

fn preload_window(month: MonthKey) -> BTreeSet<MonthKey> {
    let mut months = BTreeSet::from([month]);
    if let Some(next) = month.next() {
        months.insert(next);
    }
    months
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preload_window_keeps_visible_month_and_next_only() {
        assert_eq!(
            preload_window(MonthKey {
                year: 2026,
                month: 12,
            }),
            BTreeSet::from([
                MonthKey {
                    year: 2026,
                    month: 12,
                },
                MonthKey {
                    year: 2027,
                    month: 1,
                },
            ])
        );
    }
}
