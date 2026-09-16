pub mod catcher;
pub mod popover;
pub mod runtime;

use glimpse_config::Applet as AppletConfig;
use glimpse_dbus::notifications::{NotificationUrgency, NotificationsProviderHandle};
use glimpse_widgets::IndicatorSpec;
use popover::{PopoverHandle, Seat};
use std::cell::RefCell;
use std::fmt::Display;
use std::future::Future;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::watch;
use tokio::task::AbortHandle;

pub trait Applet: 'static {
    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        let _ = (ctx, config);
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input);

    fn view(&mut self, ctx: &Ctx) -> Option<gtk4::Widget> {
        let _ = ctx;
        None
    }

    fn orient(&mut self, orientation: gtk4::Orientation) {
        let _ = orientation;
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        Vec::new()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let _ = seat;
        None
    }

    fn anchor(&self) -> Option<gtk4::Widget> {
        None
    }
}

#[derive(Debug)]
pub enum Input {
    Pointer(Pointer),
    Tick,
    Woken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pointer {
    Press(Button),
    Scroll(Direction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
    Other(u32),
}

impl Button {
    pub(crate) fn from_code(code: u32) -> Self {
        match code {
            1 => Self::Left,
            2 => Self::Middle,
            3 => Self::Right,
            other => Self::Other(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

pub fn spawn_command<F, T, E>(operation: &'static str, future: F)
where
    F: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + 'static,
    E: Display + Send + 'static,
{
    relm4::spawn(async move {
        if let Err(error) = future.await {
            tracing::warn!(operation, %error, "service command failed");
        }
    });
}

const DEFAULT_APP_ID: &str = "me.aresa.GlimpsePanel";

pub fn app_id() -> String {
    std::env::var("GLIMPSE_PANEL_APP_ID").unwrap_or_else(|_| DEFAULT_APP_ID.to_owned())
}

pub struct Report {
    pub notifications: NotificationsProviderHandle,
    pub app_name: String,
    pub icon: String,
    pub summary: String,
}

pub fn spawn_reported<F, T, E>(
    operation: &'static str,
    report: Report,
    wording: impl Fn(&E) -> Option<String> + Send + 'static,
    future: F,
) where
    F: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + 'static,
    E: Display + Send + 'static,
{
    relm4::spawn(async move {
        let Err(error) = future.await else {
            return;
        };
        report_failure(operation, report, wording(&error), error).await;
    });
}

pub async fn report_failure<E: Display>(
    operation: &'static str,
    report: Report,
    body: Option<String>,
    error: E,
) {
    tracing::warn!(operation, %error, "service command failed");
    let Some(body) = body else {
        return;
    };
    let posted = report
        .notifications
        .post(
            &report.app_name,
            &app_id(),
            &report.icon,
            &report.summary,
            &body,
            NotificationUrgency::Normal,
        )
        .await;
    if let Err(error) = posted {
        tracing::warn!(operation, %error, "could not report a failed command");
    }
}

pub struct Ctx {
    name: String,
    output: Option<String>,
    host: relm4::Sender<runtime::HostInput>,
    sources: RefCell<Vec<SourceGuard>>,
    ticks: RefCell<Option<SourceGuard>>,
}

#[derive(Clone)]
pub struct Opener(relm4::Sender<runtime::HostInput>);

impl Opener {
    pub fn open_popover(&self) {
        let _ = self.0.send(runtime::HostInput::PopoverRequested);
    }

    pub fn wake(&self) {
        let _ = self.0.send(runtime::HostInput::Woken);
    }
}

impl Ctx {
    pub(crate) fn new(
        name: String,
        output: Option<String>,
        host: relm4::Sender<runtime::HostInput>,
    ) -> Self {
        Self {
            name,
            output,
            host,
            sources: RefCell::default(),
            ticks: RefCell::default(),
        }
    }

    pub fn opener(&self) -> Opener {
        Opener(self.host.clone())
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub fn output(&self) -> Option<&str> {
        self.output.as_deref()
    }

    pub(crate) fn shutdown(&self) {
        let stopped = self.sources.borrow_mut().drain(..).count();
        let ticking = self.ticks.take().is_some();
        tracing::debug!(applet = self.name, stopped, ticking, "sources stopped");
    }

    pub fn interval(&self, period: Duration) {
        if period.is_zero() {
            tracing::error!(applet = self.name, "a zero interval would spin; ignored");
            return;
        }

        let host = self.host.clone();
        let applet = self.name.clone();
        let start = tokio::time::Instant::now() + until_boundary(since_epoch(), period);
        let handle = relm4::spawn(async move {
            let mut ticks = ticker(start, period);
            loop {
                ticks.tick().await;
                tracing::trace!(applet, "tick");
                if host.send(runtime::HostInput::Ticked).is_err() {
                    return;
                }
            }
        });

        self.ticks.replace(Some(SourceGuard {
            abort: handle.abort_handle(),
        }));
        tracing::debug!(applet = self.name, ?period, "ticking");
    }

    pub fn watch<T>(&self, state: watch::Receiver<T>)
    where
        T: Send + Sync + 'static,
    {
        let host = self.host.clone();
        let applet = self.name.clone();
        let handle = relm4::spawn(watch_changes(state, host));

        self.sources.borrow_mut().push(SourceGuard {
            abort: handle.abort_handle(),
        });
        tracing::debug!(applet, "watching typed state");
    }
}

async fn watch_changes<T>(mut state: watch::Receiver<T>, host: relm4::Sender<runtime::HostInput>)
where
    T: Send + Sync + 'static,
{
    while state.changed().await.is_ok() {
        if host.send(runtime::HostInput::Woken).is_err() {
            return;
        }
    }
}

fn since_epoch() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

fn ticker(start: tokio::time::Instant, period: Duration) -> tokio::time::Interval {
    let mut ticks = tokio::time::interval_at(start, period);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    ticks
}

fn until_boundary(since_epoch: Duration, period: Duration) -> Duration {
    let step = period.as_nanos();
    if step == 0 {
        return period;
    }
    let past = since_epoch.as_nanos() % step;
    Duration::from_nanos(u64::try_from(step - past).unwrap_or(u64::MAX))
}

struct SourceGuard {
    abort: AbortHandle,
}

impl Drop for SourceGuard {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_stalled_timer_skips_what_it_missed_rather_than_firing_all_of_it() {
        let ticks = ticker(tokio::time::Instant::now(), Duration::from_secs(1));

        assert_eq!(
            ticks.missed_tick_behavior(),
            tokio::time::MissedTickBehavior::Skip,
            "tokio defaults to Burst, which after a suspend would deliver one tick per second \
             slept, all in one pass; Skip is also the only behaviour that keeps the phase"
        );
    }

    #[tokio::test]
    async fn a_typed_state_change_wakes_the_applet() {
        let (state, receiver) = watch::channel(1_u8);
        let (host, inputs) = relm4::channel();
        let task = tokio::spawn(watch_changes(receiver, host));

        state.send(2).expect("the receiver is live");
        assert!(matches!(
            inputs.recv().await,
            Some(runtime::HostInput::Woken)
        ));

        drop(state);
        task.await.expect("the watch task stops");
    }

    #[test]
    fn a_tick_lands_on_the_boundary_rather_than_where_the_panel_happened_to_start() {
        let period = Duration::from_secs(60);
        let started = Duration::from_millis(12_400);

        assert_eq!(
            until_boundary(started, period),
            Duration::from_millis(47_600),
            "starting 12.4s into a minute must wait out the rest of it, not a whole minute"
        );
        assert_eq!(
            (started + until_boundary(started, period)).as_nanos() % period.as_nanos(),
            0
        );
    }

    #[test]
    fn a_tick_exactly_on_the_boundary_waits_a_whole_period() {
        let period = Duration::from_secs(60);

        assert_eq!(
            until_boundary(Duration::from_secs(120), period),
            period,
            "waiting zero would render the same value twice in one instant"
        );
    }

    #[test]
    fn a_second_long_period_aligns_to_the_second() {
        assert_eq!(
            until_boundary(Duration::from_millis(1_250), Duration::from_secs(1)),
            Duration::from_millis(750)
        );
    }

    #[test]
    fn a_zero_period_cannot_be_aligned_and_asks_for_no_wait() {
        assert_eq!(
            until_boundary(Duration::from_secs(5), Duration::ZERO),
            Duration::ZERO,
            "Ctx::interval refuses a zero period before it gets here"
        );
    }

    #[test]
    fn a_pointer_button_keeps_its_gdk_code_when_it_has_no_name() {
        assert_eq!(Button::from_code(1), Button::Left);
        assert_eq!(Button::from_code(2), Button::Middle);
        assert_eq!(Button::from_code(3), Button::Right);
        assert_eq!(Button::from_code(8), Button::Other(8));
    }
}
