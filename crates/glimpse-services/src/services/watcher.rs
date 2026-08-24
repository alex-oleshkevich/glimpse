use std::{path::PathBuf, time};

use notify::EventKind;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use serde::Deserialize;

use crate::{
    context::{Ctx, SourceGuard},
    service::{Input, Service, StartError},
};

const DEBOUNCE: time::Duration = time::Duration::from_millis(250);

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Config {
    paths: Vec<String>,
}

enum Event {
    Changed(PathBuf), // add raeson
}

pub struct Watcher {
    paths: Vec<String>,
    _handle: SourceGuard,
}

impl Service for Watcher {
    type Config = Config;
    type Command = ();
    type Event = Event;

    async fn start(ctx: &Ctx<Self>, config: Self::Config) -> Result<Self, StartError> {
        let events = ctx.events();

        let debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
            let Ok(batch) = result else { return };
            let touched = batch.iter().any(|event| {
                matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                )
            });
            if touched {
                events.try_send(Event::Changed(event.path.to_path_buf()));
            }
        });

        Ok(Self {
            paths: config.paths,
            _handle: ctx.spawn(async move {}),
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Changed(_)) => {}
            Input::Command(_) => {}
            Input::Config(config) => self.paths = config.paths,
        }
    }
}

async fn watch(paths: Vec<PathBuf>) {}
