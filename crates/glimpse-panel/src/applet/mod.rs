pub mod popover;
pub mod runtime;

use glimpse_config::Applet as AppletConfig;
use glimpse_contracts::{Command, Message};
use glimpse_ipc::{Client, Event};
use glimpse_widgets::IndicatorSpec;
use popover::{PopoverHandle, Seat};
use serde::Deserialize;
use std::cell::RefCell;
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
        tracing::debug!(applet = self.caller.name, stopped, "sources stopped");
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

    #[test]
    fn a_pointer_button_keeps_its_gdk_code_when_it_has_no_name() {
        assert_eq!(Button::from_code(1), Button::Left);
        assert_eq!(Button::from_code(2), Button::Middle);
        assert_eq!(Button::from_code(3), Button::Right);
        assert_eq!(Button::from_code(8), Button::Other(8));
    }
}
