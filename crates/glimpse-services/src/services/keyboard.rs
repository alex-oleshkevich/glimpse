use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use futures_util::StreamExt;

use glimpse_compositors::{
    client::{
        CompositorState as Mirrored, Handle as ProviderHandle, KeyboardBarrier, KeyboardUpdate,
    },
    model::KeyboardLayouts as Served,
};
use glimpse_config::Remember;
use glimpse_dbus::PENDING;
use tokio::sync::oneshot;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, Service, ServiceEndpoint},
    services::{WindowInfo, cap},
    subscription::Sub,
};

use super::compositor::{NO_PROVIDER, command_error};
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
    pub restored: bool,
}

const NAME_CAP: usize = 128;
const BUILT_IN_CODES: &str = include_str!("../../../../data/language-codes.json");

pub enum Event {
    Provider(Box<Mirrored>),
    Closed,
    Switched(u64, Option<u8>),
    Restored(u64, bool),
    Watching(KeyboardBarrier),
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
        self.call("keyboard service stopped", |reply| Command::Switch {
            target,
            reply,
        })
        .await?
    }
}

pub struct Dependencies {
    pub provider: ProviderHandle,
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
pub enum StoreKey {
    Window(u64),
    App(String),
}

pub struct Keyboard {
    layouts: Publisher<KeyboardLayouts>,
    provider: ProviderHandle,
    served: Option<Served>,
    config: Config,
    language_codes: BTreeMap<String, String>,
    store: HashMap<StoreKey, u8>,
    focused: Option<WindowInfo>,
    operations: HashMap<u64, Option<StoreKey>>,
    operation_id: u64,
    barrier: Option<KeyboardBarrier>,
    pending_restore: Option<(u64, u8)>,
    restored: bool,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Watch {
    Provider,
}

impl Service for Keyboard {
    const NAME: &'static str = "keyboard";

    type Config = Config;
    type State = KeyboardLayouts;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = Watch;

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let provider = self.provider.clone();
        vec![Sub::stream(Watch::Provider, move |ctx| async move {
            let (barrier, updates) = provider.keyboard_updates();
            let _ = ctx
                .events()
                .send(Input::Event(Event::Watching(barrier)))
                .await;
            updates
                .map(|update| match update {
                    KeyboardUpdate::State(state) => Event::Provider(state),
                    KeyboardUpdate::Barrier(id) => Event::Restored(id, true),
                })
                .chain(futures_util::stream::once(async { Event::Closed }))
        })]
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> Self {
        tracing::debug!("starting keyboard service");
        Self {
            layouts: ctx.publisher(),
            provider: dependencies.provider,
            served: None,
            config,
            language_codes: load_codes().await,
            store: HashMap::new(),
            focused: None,
            operations: HashMap::new(),
            operation_id: 0,
            barrier: None,
            pending_restore: None,
            restored: false,
        }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Provider(state)) => match state.unavailable.as_deref() {
                None => {
                    if ctx.is_degraded() {
                        ctx.running();
                    }
                    self.followed(ctx, &state);
                }
                Some(PENDING) => {}
                Some(reason) => self.lost(ctx, reason),
            },
            Input::Event(Event::Closed) => {
                self.lost(ctx, "the compositor provider closed");
            }
            Input::Event(Event::Switched(id, index)) => {
                if let Some(index) = index {
                    if let Some(Some(key)) = self.operations.get(&id) {
                        self.store.insert(key.clone(), index);
                    }
                } else {
                    self.finished(ctx, id, false);
                }
            }
            Input::Event(Event::Restored(id, succeeded)) => {
                self.finished(ctx, id, succeeded);
            }
            Input::Event(Event::Watching(barrier)) => self.barrier = Some(barrier),
            Input::Command(Command::Switch { target, reply }) => {
                self.switch(ctx, target, reply);
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
    fn lost(&mut self, ctx: &Ctx<Self>, reason: &str) {
        self.store.clear();
        self.focused = None;
        self.operations.clear();
        self.pending_restore = None;
        if self.served.is_some() {
            ctx.degraded(reason);
        }
    }

    fn published(&self) -> KeyboardLayouts {
        self.served.as_ref().map_or_else(
            || KeyboardLayouts {
                restored: self.restored,
                ..KeyboardLayouts::default()
            },
            |served| {
                payload(
                    served,
                    &self.config.labels,
                    &self.language_codes,
                    self.restored,
                )
            },
        )
    }

    fn publish(&mut self) {
        self.layouts.set(self.published());
    }

    fn followed(&mut self, ctx: &Ctx<Self>, state: &Mirrored) {
        let was = self.served.replace(state.keyboard.clone());
        let focused = focused_of(state);
        let moved = self.focused.as_ref().map(|window| window.id)
            != focused.as_ref().map(|window| window.id);
        self.focused = focused;
        self.store.retain(|key, _| match key {
            StoreKey::App(_) => true,
            StoreKey::Window(id) => state.windows.iter().any(|window| window.id.0 == *id),
        });

        let switched = was.is_some_and(|was| was.current != state.keyboard.current);
        if let Some(index) = state
            .keyboard
            .current
            .filter(|_| switched && !moved && self.operations.is_empty())
        {
            self.remember(index);
        }
        if let Some(index) = state
            .keyboard
            .current
            .and_then(|index| u8::try_from(index).ok())
            && let Some(key) = remember_key(self.config.remember, self.focused.as_ref())
        {
            self.store.entry(key).or_insert(index);
        }
        if switched {
            let new_current = state
                .keyboard
                .current
                .and_then(|index| u8::try_from(index).ok());
            self.restored =
                matches!(self.pending_restore, Some((_, index)) if Some(index) == new_current);
            if self.restored {
                self.pending_restore = None;
            }
        }
        self.publish();
        if moved {
            self.restore(ctx);
        }
    }

    fn remember(&mut self, index: u32) {
        let Ok(index) = u8::try_from(index) else {
            return;
        };
        if let Some(key) = remember_key(self.config.remember, self.focused.as_ref()) {
            self.store.insert(key, index);
        }
    }

    fn restore(&mut self, ctx: &Ctx<Self>) {
        if !self.operations.is_empty() {
            return;
        }
        let Some(key) = remember_key(self.config.remember, self.focused.as_ref()) else {
            return;
        };
        let Some(&index) = self.store.get(&key) else {
            return;
        };
        if usize::from(index) >= self.published().layouts.len() {
            self.store.remove(&key);
            return;
        }
        if self.published().current == Some(index) {
            return;
        }
        self.operation_id += 1;
        let id = self.operation_id;
        self.operations.insert(id, Some(key));
        self.pending_restore = Some((id, index));
        let provider = self.provider.clone();
        let barrier = self.barrier.clone();
        ctx.spawn_detached(move |ctx| async move {
            let succeeded = provider
                .switch_keyboard_layout(u32::from(index))
                .await
                .is_ok();
            if succeeded && let Some(barrier) = barrier {
                if barrier.request(id).await.is_ok() {
                    return;
                }
                let _ = ctx
                    .events()
                    .send(Input::Event(Event::Restored(id, false)))
                    .await;
                return;
            }
            let _ = ctx
                .events()
                .send(Input::Event(Event::Restored(id, succeeded)))
                .await;
        });
    }

    fn switch(
        &mut self,
        ctx: &Ctx<Self>,
        target: LayoutRef,
        reply: oneshot::Sender<Result<(), CommandError>>,
    ) {
        let served = self.served.is_some();
        let remembered = remember_key(self.config.remember, self.focused.as_ref());
        self.operation_id += 1;
        let id = self.operation_id;
        self.operations.insert(id, remembered);
        let provider = self.provider.clone();
        let barrier = self.barrier.clone();

        ctx.spawn_detached(move |ctx| async move {
            let result = if served {
                match target {
                    LayoutRef::Index { index } => provider
                        .switch_keyboard_layout(u32::from(index))
                        .await
                        .map(|()| u32::from(index)),
                    LayoutRef::Next | LayoutRef::Prev => {
                        provider
                            .step_keyboard_layout(target == LayoutRef::Next)
                            .await
                    }
                }
                .map_err(|error| command_error(&error))
            } else {
                Err(CommandError::Unsupported(NO_PROVIDER.to_owned()))
            };
            let index = result
                .as_ref()
                .ok()
                .and_then(|index| u8::try_from(*index).ok());
            let _ = ctx
                .events()
                .send(Input::Event(Event::Switched(id, index)))
                .await;
            let succeeded = result.is_ok();
            let _ = reply.send(result.map(|_| ()));
            if succeeded {
                if let Some(barrier) = barrier {
                    if barrier.request(id).await.is_ok() {
                        return;
                    }
                    let _ = ctx
                        .events()
                        .send(Input::Event(Event::Restored(id, false)))
                        .await;
                } else {
                    let _ = ctx
                        .events()
                        .send(Input::Event(Event::Restored(id, true)))
                        .await;
                }
            }
        });
    }

    fn finished(&mut self, ctx: &Ctx<Self>, id: u64, succeeded: bool) {
        if self
            .pending_restore
            .is_some_and(|(pending, _)| pending == id)
        {
            self.pending_restore = None;
        }
        let Some(key) = self.operations.remove(&id) else {
            return;
        };
        if !self.operations.is_empty() {
            return;
        }
        if remember_key(self.config.remember, self.focused.as_ref()) != key {
            self.restore(ctx);
        } else if succeeded
            && let Some(index) = self.served.as_ref().and_then(|served| served.current)
        {
            self.remember(index);
        }
    }
}

fn focused_of(state: &Mirrored) -> Option<WindowInfo> {
    let id = state.focus.window?;
    let window = state.windows.iter().find(|window| window.id == id)?;
    Some(WindowInfo {
        id: id.0,
        title: None,
        app_id: window.app_id.clone(),
        workspace: None,
        focused: true,
        floating: false,
        urgent: false,
        order: None,
    })
}

fn payload(
    layouts: &Served,
    labels: &BTreeMap<String, String>,
    codes: &BTreeMap<String, String>,
    restored: bool,
) -> KeyboardLayouts {
    let count = layouts.names.len().max(layouts.codes.len());
    let items = (0..count)
        .map(|at| {
            let code = layouts.codes.get(at).filter(|code| !code.is_empty());
            let name = layouts.names.get(at).or(code).map_or("", String::as_str);
            let code = code.map_or(name, String::as_str);
            KeyboardLayout {
                code: cap(&badge(labels, code, name, codes), NAME_CAP),
                name: cap(name, NAME_CAP),
            }
        })
        .collect::<Vec<_>>();
    let current = layouts
        .current
        .and_then(|index| usize::try_from(index).ok())
        .filter(|&index| index < items.len())
        .and_then(|index| u8::try_from(index).ok());
    KeyboardLayouts {
        layouts: items,
        current,
        restored,
    }
}

fn badge(
    labels: &BTreeMap<String, String>,
    code: &str,
    name: &str,
    codes: &BTreeMap<String, String>,
) -> String {
    let derived = layout_code(code, codes);
    let code_key = code.to_lowercase();
    let derived_key = derived.to_lowercase();
    let name_key = name.to_lowercase();
    labels
        .iter()
        .find(|(key, _)| {
            let key = key.to_lowercase();
            key == code_key || key == derived_key || key == name_key
        })
        .map(|(_, label)| label.as_str())
        .filter(|label| !label.is_empty())
        .map_or(derived, str::to_owned)
}

fn layout_code(layout: &str, codes: &BTreeMap<String, String>) -> String {
    let first_word = layout.split_whitespace().next().unwrap_or(layout);
    if let Some(code) = codes.get(&first_word.to_lowercase()) {
        return code.clone();
    }
    if !layout.contains(' ') {
        return layout.to_uppercase();
    }
    first_word
        .chars()
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

async fn load_codes() -> BTreeMap<String, String> {
    let path = match tokio::task::spawn_blocking(|| {
        glimpse_config::find(Path::new("language-codes.json"))
    })
    .await
    {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(%error, "could not find the language codes");
            None
        }
    };
    if let Some(path) = path {
        match tokio::fs::read_to_string(&path).await {
            Ok(text) => match parse_codes(&text) {
                Ok(codes) => {
                    tracing::debug!(path = %path.display(), entries = codes.len(), "read language codes");
                    return codes;
                }
                Err(error) => {
                    tracing::warn!(path = %path.display(), %error, "ignored malformed language codes");
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "could not read the language codes");
            }
        }
    }
    parse_codes(BUILT_IN_CODES).unwrap_or_default()
}

fn parse_codes(text: &str) -> Result<BTreeMap<String, String>, serde_json::Error> {
    let codes: BTreeMap<String, String> = serde_json::from_str(text)?;
    Ok(codes
        .into_iter()
        .map(|(language, code)| (language.to_lowercase(), code))
        .collect())
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

#[cfg(test)]
mod tests;
