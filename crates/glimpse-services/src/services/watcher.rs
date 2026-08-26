use std::{path::PathBuf, time};

use notify::EventKind;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use serde::Deserialize;

use crate::{
    context::{Ctx, SourceGuard},
    service::{Input, Service, ServiceError},
};

const DEBOUNCE: time::Duration = time::Duration::from_millis(250);

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Config {
    paths: Vec<String>,
}

pub enum Event {
    Changed(PathBuf), // add raeson
}

pub struct Watcher {
    paths: Vec<String>,
    _handle: SourceGuard,
}

impl Service for Watcher {
    const NAME: &'static str = "watcher";
    const TOPICS: &'static [&'static str] = &[];

    type Config = Config;
    type Command = ();
    type Event = Event;

    async fn start(ctx: &Ctx<Self>, config: Self::Config) -> Result<Self, ServiceError> {
        let _events = ctx.events();

        let _debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
            let Ok(batch) = result else { return };
            let touched = batch.iter().any(|event| {
                matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                )
            });
            if touched {
                // events.try_send(Event::Changed(event.path.to_path_buf()));
            }
        });

        Ok(Self {
            paths: config.paths,
            // The debouncer delivers on a thread of its own through `ctx.events()`, so there is no
            // source here yet — this holds the slot until it is wired.
            _handle: ctx.spawn(|_ctx| std::future::pending()),
        })
    }

    async fn handle(&mut self, _ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Changed(_)) => {}
            Input::Command((), responder) => responder.ok(()),
            Input::Config(config) => self.paths = config.paths,
        }
    }

    fn peek_config(_config: &glimpse_config::Config) -> Self::Config {
        Self::Config { paths: vec![] }
    }
}
