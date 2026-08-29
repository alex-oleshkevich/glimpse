pub mod runtime;

use glimpse_contracts::{Command, Message};
use glimpse_ipc::{Client, Event};
use glimpse_widgets::IndicatorSpec;
use serde::Deserialize;
use std::cell::RefCell;
use tokio::task::AbortHandle;

pub trait Applet: 'static {
    fn start(ctx: &Ctx) -> Self
    where
        Self: Sized;

    fn handle(&mut self, ctx: &Ctx, input: &Input);

    fn indicators(&self) -> Vec<IndicatorSpec>;
}

#[derive(Debug)]
pub enum Input {
    Topic(Event),
    Pointer { indicator: String, pointer: Pointer },
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
    name: String,
    client: Client,
    events: relm4::Sender<Event>,
    sources: RefCell<Vec<SourceGuard>>,
}

impl Ctx {
    pub(crate) fn new(name: String, client: Client, events: relm4::Sender<Event>) -> Self {
        Self {
            name,
            client,
            events,
            sources: RefCell::default(),
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn shutdown(&self) {
        let stopped = self.sources.borrow_mut().drain(..).count();
        tracing::debug!(applet = self.name, stopped, "sources stopped");
    }

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

    pub fn subscribe<T: Message>(&self) {
        let client = self.client.clone();
        let events = self.events.clone();
        let applet = self.name.clone();
        let mut states = client.watch_state();

        let handle = relm4::spawn(async move {
            loop {
                match client.subscribe(T::NAME).await {
                    Ok(mut subscription) => {
                        let matched = subscription.matched();
                        if matched == 0 {
                            tracing::warn!(applet, topic = T::NAME, "no declared topic matched");
                        } else {
                            tracing::debug!(applet, topic = T::NAME, matched, "subscribed");
                        }
                        while let Some(event) = subscription.next().await {
                            if events.send(event).is_err() {
                                return;
                            }
                        }
                        return;
                    }
                    Err(error) => {
                        tracing::debug!(applet, topic = T::NAME, %error, "subscribe refused, waiting");
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
