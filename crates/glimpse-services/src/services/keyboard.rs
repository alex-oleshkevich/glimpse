use crate::services::cap;
use std::collections::{BTreeMap, HashMap};

use crate::services::{CompositorWindows, WindowInfo};
use futures_util::{StreamExt, stream};
use glimpse_compositors::{
    Compositor as Backend, CompositorError, Event as Change, KeyboardLayouts as BackendLayouts,
    LayoutTarget, Resync, detect_compositor, layout_code,
};
use glimpse_config::Remember;
use tokio::sync::oneshot;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, Service, ServiceEndpoint},
    subscription::Sub,
};

use super::compositor::CompositorHandle;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum LayoutRef {
    Next,
    Prev,
    Index { index: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardLayout {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KeyboardLayouts {
    pub layouts: Vec<KeyboardLayout>,
    pub current: Option<u8>,
}

const NAME_CAP: usize = 128;

pub enum Event {
    Snapshot(BackendLayouts),
    Changed(Change),
    Windows(Option<CompositorWindows>),
    Failed(String),
}

#[derive(Debug)]
pub enum Command {
    Switch {
        target: LayoutRef,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
}

pub type KeyboardHandle = ServiceEndpoint<Keyboard>;

impl ServiceEndpoint<Keyboard> {
    pub async fn switch(&self, target: LayoutRef) -> Result<(), CommandError> {
        self.call(|reply| Command::Switch { target, reply }).await?
    }
}

pub struct Dependencies {
    pub compositor: CompositorHandle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    remember: Remember,
    labels: BTreeMap<String, String>,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            remember: document.keyboard.remember,
            labels: document.keyboard.labels.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum StoreKey {
    Window(u64),
    App(String),
}

pub struct Keyboard {
    backend: Backend,
    layouts: Publisher<KeyboardLayouts>,
    compositor: CompositorHandle,
    state: BackendLayouts,
    config: Config,
    store: HashMap<StoreKey, u8>,
    focused: Option<WindowInfo>,
    attempt: u64,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Watch {
    Events,
    Fetch { attempt: u64 },
    Windows,
}

impl Service for Keyboard {
    const NAME: &'static str = "keyboard";

    type Config = Config;
    type State = KeyboardLayouts;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = Watch;

    fn command_stopped_reason(_command: &Self::Command) -> &'static str {
        "keyboard service stopped"
    }

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let follow = self.backend.clone();
        let read = self.backend.clone();
        let mut subs = vec![
            Sub::stream(Watch::Events, move |_ctx| async move {
                match follow.events().await {
                    Ok(events) => events
                        .map(Event::Changed)
                        .chain(stream::once(async {
                            Event::Failed("the compositor closed its event stream".to_owned())
                        }))
                        .boxed(),
                    Err(error) => {
                        stream::once(async move { Event::Failed(error.to_string()) }).boxed()
                    }
                }
            }),
            Sub::stream(
                Watch::Fetch {
                    attempt: self.attempt,
                },
                move |_ctx| async move {
                    stream::once(async move {
                        match read.snapshot().await {
                            Ok(snapshot) => Event::Snapshot(snapshot.keyboard),
                            Err(error) => Event::Failed(error.to_string()),
                        }
                    })
                },
            ),
        ];
        if self.config.remember != Remember::Global {
            subs.push(Sub::watch(
                Watch::Windows,
                self.compositor.subscribe(),
                |state| Event::Windows(state.windows),
                Event::Windows(None),
            ));
        }
        subs
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> Self {
        tracing::debug!("starting keyboard service");
        Self {
            backend: detect_compositor(),
            layouts: ctx.publisher(),
            compositor: dependencies.compositor,
            state: BackendLayouts::default(),
            config,
            store: HashMap::new(),
            focused: None,
            attempt: 0,
        }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Snapshot(state)) => {
                self.state = state;
                if ctx.is_degraded() {
                    ctx.running();
                }
                self.publish();
            }
            Input::Event(Event::Changed(change)) => {
                if let Change::KeyboardLayoutSwitched { idx, .. } = &change {
                    self.remember(*idx);
                }
                match apply_keyboard(&mut self.state, change) {
                    Apply::Publish => self.publish(),
                    Apply::Refetch => self.attempt += 1,
                    Apply::Ignore => {}
                }
            }
            Input::Event(Event::Windows(Some(windows))) => self.windows_changed(windows).await,
            Input::Event(Event::Windows(None)) => self.focused = None,
            Input::Event(Event::Failed(reason)) => ctx.degraded(reason),
            Input::Command(Command::Switch { target, reply }) => {
                self.switch(target, reply).await;
            }
            Input::Config(config) => {
                let labels_moved = config.labels != self.config.labels;
                self.config = config;
                if labels_moved {
                    self.publish();
                }
            }
        }
    }
}

impl Keyboard {
    fn publish(&mut self) {
        self.layouts.set(payload(&self.state, &self.config.labels));
    }

    fn remember(&mut self, idx: usize) {
        let Ok(index) = u8::try_from(idx) else {
            return;
        };
        if let Some(key) = remember_key(self.config.remember, self.focused.as_ref()) {
            self.store.insert(key, index);
        }
    }

    async fn switch(
        &mut self,
        target: LayoutRef,
        reply: oneshot::Sender<Result<(), CommandError>>,
    ) {
        match self
            .backend
            .switch_keyboard_layout(layout_target(target))
            .await
        {
            Ok(()) => {
                let index = match target {
                    LayoutRef::Index { index } => Some(index),
                    LayoutRef::Next | LayoutRef::Prev => payload(&self.state, &self.config.labels)
                        .current
                        .and_then(|current| stepped(current, target, self.state.names.len())),
                };
                if let (Some(index), Some(key)) = (
                    index,
                    remember_key(self.config.remember, self.focused.as_ref()),
                ) {
                    self.store.insert(key, index);
                }
                let _ = reply.send(Ok(()));
            }
            Err(error) => {
                let _ = reply.send(Err(command_error(&error)));
            }
        }
    }

    async fn windows_changed(&mut self, windows: CompositorWindows) {
        self.store.retain(|key, _| match key {
            StoreKey::App(_) => true,
            StoreKey::Window(id) => windows.windows.iter().any(|window| window.id == *id),
        });

        let focused = windows.windows.into_iter().find(|window| window.focused);
        let changed = match (&self.focused, &focused) {
            (Some(was), Some(now)) => was.id != now.id,
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (None, None) => false,
        };
        self.focused = focused;
        if !changed {
            return;
        }
        let Some(key) = remember_key(self.config.remember, self.focused.as_ref()) else {
            return;
        };
        let Some(&index) = self.store.get(&key) else {
            return;
        };
        let current = payload(&self.state, &self.config.labels).current;
        if current == Some(index) {
            return;
        }
        let _ = self
            .backend
            .switch_keyboard_layout(LayoutTarget::Index(index))
            .await;
    }
}

enum Apply {
    Publish,
    Refetch,
    Ignore,
}

fn apply_keyboard(state: &mut BackendLayouts, change: Change) -> Apply {
    match change {
        Change::KeyboardLayoutsChanged(layouts) => {
            *state = layouts;
            Apply::Publish
        }
        Change::KeyboardLayoutSwitched { idx, .. } => {
            state.current = (idx < state.names.len()).then_some(idx);
            Apply::Publish
        }
        Change::Resync(Resync::Keyboard) => Apply::Refetch,
        _ => Apply::Ignore,
    }
}

fn payload(layouts: &BackendLayouts, labels: &BTreeMap<String, String>) -> KeyboardLayouts {
    let n = layouts.names.len().min(layouts.codes.len());
    let items = layouts
        .names
        .iter()
        .zip(layouts.codes.iter())
        .take(n)
        .map(|(name, code)| KeyboardLayout {
            code: cap(&badge(labels, code, name), NAME_CAP),
            name: cap(name, NAME_CAP),
        })
        .collect::<Vec<_>>();
    let current = layouts
        .current
        .filter(|&index| index < items.len())
        .and_then(|index| u8::try_from(index).ok());
    KeyboardLayouts {
        layouts: items,
        current,
    }
}

fn badge(labels: &BTreeMap<String, String>, code: &str, name: &str) -> String {
    let code_key = code.to_lowercase();
    let name_key = name.to_lowercase();
    labels
        .iter()
        .find(|(key, _)| {
            let key = key.to_lowercase();
            key == code_key || key == name_key
        })
        .map(|(_, label)| label.as_str())
        .filter(|label| !label.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if code.is_empty() {
                layout_code(name)
            } else {
                layout_code(code)
            }
        })
}

fn remember_key(remember: Remember, focused: Option<&WindowInfo>) -> Option<StoreKey> {
    let focused = focused?;
    match remember {
        Remember::Global => None,
        Remember::Window => Some(StoreKey::Window(focused.id)),
        Remember::App => focused
            .app_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(|id| StoreKey::App(id.to_owned())),
    }
}

fn layout_target(target: LayoutRef) -> LayoutTarget {
    match target {
        LayoutRef::Next => LayoutTarget::Next,
        LayoutRef::Prev => LayoutTarget::Prev,
        LayoutRef::Index { index } => LayoutTarget::Index(index),
    }
}

fn stepped(current: u8, target: LayoutRef, len: usize) -> Option<u8> {
    let n = u8::try_from(len).ok().filter(|&n| n > 0)?;
    Some(match target {
        LayoutRef::Next => (current + 1) % n,
        LayoutRef::Prev => current.checked_sub(1).unwrap_or(n - 1),
        LayoutRef::Index { index } => index,
    })
}

fn command_error(error: &CompositorError) -> CommandError {
    match error {
        CompositorError::Unsupported(reason) | CompositorError::Unavailable(reason) => {
            CommandError::Unsupported(reason.to_string())
        }
        CompositorError::Connect { .. } | CompositorError::Closed => {
            CommandError::Unavailable(error.to_string())
        }
        CompositorError::Refused(reason) => CommandError::InvalidArgument(reason.to_string()),
        CompositorError::Protocol(reason) => CommandError::Internal(reason.to_string()),
        CompositorError::NoFile(_)
        | CompositorError::InvalidPath(_)
        | CompositorError::Watch { .. } => CommandError::Internal(error.to_string()),
    }
}

#[cfg(test)]
mod tests;
