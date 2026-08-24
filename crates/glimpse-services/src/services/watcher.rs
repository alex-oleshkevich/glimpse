use std::path::PathBuf;

use serde::Deserialize;

use crate::{
    context::{Ctx, SourceGuard},
    service::{Input, Service, StartError},
};

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
