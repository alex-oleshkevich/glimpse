pub mod catcher;
pub mod popover;
pub mod runtime;

use glimpse_config::Applet as AppletConfig;
use glimpse_contracts::{Command, Message};
use glimpse_ipc::{Client, Event};
use glimpse_widgets::IndicatorSpec;
use popover::{PopoverHandle, Seat};
use serde::Deserialize;
use std::cell::RefCell;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::task::AbortHandle;

pub trait Applet: 'static {
    fn topics(&self) -> &'static [&'static str] {
        &[]
    }

    fn start() -> Self
    where
        Self: Sized;

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
    Topic(Event),
    Pointer(Pointer),
    Tick,
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

pub struct Ctx {
    caller: Caller,
    output: Option<String>,
    events: relm4::Sender<Event>,
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

    #[allow(
        dead_code,
        reason = "an applet's half of dismissal; no applet dismisses its own yet"
    )]
    pub fn close_popover(&self) {
        let _ = self.0.send(runtime::HostInput::PopoverClosed);
    }
}

#[derive(Clone)]
pub struct Caller {
    name: String,
    client: Client,
}

impl Caller {
    pub fn call<C: Command>(&self, args: C::Args) {
        let client = self.client.clone();
        let applet = self.name.clone();
        tracing::debug!(applet, command = C::NAME, "calling");
        relm4::spawn(async move {
            let args = match serde_json::to_value(args) {
                Ok(args) => args,
                Err(error) => {
                    tracing::error!(applet, command = C::NAME, %error, "unserializable arguments");
                    return;
                }
            };
            if let Err(error) = client.call(C::NAME, args).await {
                tracing::warn!(applet, command = C::NAME, %error, "command failed");
            }
        });
    }
}

impl Ctx {
    pub(crate) fn new(
        name: String,
        output: Option<String>,
        client: Client,
        events: relm4::Sender<Event>,
        host: relm4::Sender<runtime::HostInput>,
    ) -> Self {
        Self {
            caller: Caller { name, client },
            output,
            events,
            host,
            sources: RefCell::default(),
            ticks: RefCell::default(),
        }
    }

    pub fn opener(&self) -> Opener {
        Opener(self.host.clone())
    }

    pub(crate) fn name(&self) -> &str {
        &self.caller.name
    }

    pub fn caller(&self) -> Caller {
        self.caller.clone()
    }

    pub fn output(&self) -> Option<&str> {
        self.output.as_deref()
    }

    pub(crate) fn shutdown(&self) {
        let stopped = self.sources.borrow_mut().drain(..).count();
        let ticking = self.ticks.take().is_some();
        tracing::debug!(
            applet = self.caller.name,
            stopped,
            ticking,
            "sources stopped"
        );
    }

    pub fn interval(&self, period: Duration) {
        if period.is_zero() {
            tracing::error!(
                applet = self.caller.name,
                "a zero interval would spin; ignored"
            );
            return;
        }

        let host = self.host.clone();
        let applet = self.caller.name.clone();
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
        tracing::debug!(applet = self.caller.name, ?period, "ticking");
    }

    pub fn call<C: Command>(&self, args: C::Args) {
        self.caller.call::<C>(args);
    }

    pub(crate) fn subscribe(&self, topic: &'static str) {
        let client = self.caller.client.clone();
        let events = self.events.clone();
        let applet = self.caller.name.clone();
        let mut states = client.watch_state();

        let handle = relm4::spawn(async move {
            loop {
                match client.subscribe(topic).await {
                    Ok(mut subscription) => {
                        let matched = subscription.matched();
                        if matched == 0 {
                            tracing::warn!(applet, topic = topic, "no declared topic matched");
                        } else {
                            tracing::debug!(applet, topic = topic, matched, "subscribed");
                        }
                        while let Some(event) = subscription.next().await {
                            if events.send(event).is_err() {
                                return;
                            }
                        }
                        return;
                    }
                    Err(error) => {
                        tracing::debug!(applet, topic = topic, %error, "subscribe refused, waiting");
                        if states.changed().await.is_err() {
                            return;
                        }
                    }
                }
            }
        });

        self.sources.borrow_mut().push(SourceGuard {
            abort: handle.abort_handle(),
        });
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

pub fn payload<T: Message>(event: &Event) -> Option<T::Payload> {
    if event.topic != T::NAME {
        return None;
    }
    match T::Payload::deserialize(&event.data) {
        Ok(payload) => Some(payload),
        Err(error) => {
            tracing::warn!(topic = T::NAME, %error, "undecodable payload");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_contracts::HeartbeatTick;

    fn event(topic: &str, data: serde_json::Value) -> Event {
        Event {
            topic: topic.to_owned(),
            seq: 1,
            ts: 0,
            stale: false,
            data,
        }
    }

    #[test]
    fn a_payload_decodes_only_for_the_topic_that_declares_it() {
        let tick = event(HeartbeatTick::NAME, serde_json::json!({ "count": 7 }));
        assert_eq!(payload::<HeartbeatTick>(&tick).map(|t| t.count), Some(7));

        let other = event("solar.status", serde_json::json!({ "count": 7 }));
        assert!(
            payload::<HeartbeatTick>(&other).is_none(),
            "a wildcard subscription must not decode a sibling topic as this one"
        );

        let broken = event(HeartbeatTick::NAME, serde_json::json!({ "count": "many" }));
        assert!(payload::<HeartbeatTick>(&broken).is_none());
    }

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
