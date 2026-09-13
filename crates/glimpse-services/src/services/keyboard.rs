use std::collections::{BTreeMap, HashMap};

use futures_util::{StreamExt, stream};
use glimpse_compositors::{
    Compositor as Backend, CompositorError, Event as Change, KeyboardLayouts as BackendLayouts,
    LayoutTarget, Resync, detect_compositor, layout_code,
};
use glimpse_config::Remember;
use glimpse_contracts::{
    CompositorWindows, KeyboardLayout, KeyboardLayouts, LayoutRef, WindowInfo,
};
use tokio::sync::oneshot;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, Service, ServiceEndpoint, ServiceError},
    subscription::Sub,
};

use super::compositor::CompositorHandle;

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

#[derive(Clone)]
pub struct KeyboardHandle(ServiceEndpoint<Keyboard>);

impl KeyboardHandle {
    pub fn snapshot(&self) -> KeyboardLayouts {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<KeyboardLayouts> {
        self.0.subscribe()
    }

    pub fn health(&self) -> tokio::sync::watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    pub async fn switch(&self, target: LayoutRef) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::Switch { target, reply })?;
        result
            .await
            .map_err(|_| CommandError::Unavailable("keyboard service stopped".to_owned()))?
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
    type Handle = KeyboardHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = Watch;

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
        KeyboardHandle(endpoint)
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
    ) -> Result<Self, ServiceError> {
        tracing::debug!("starting keyboard service");
        Ok(Self {
            backend: detect_compositor(),
            layouts: ctx.publisher(),
            compositor: dependencies.compositor,
            state: BackendLayouts::default(),
            config,
            store: HashMap::new(),
            focused: None,
            attempt: 0,
        })
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
            Input::Event(Event::Changed(change)) => match apply_keyboard(&mut self.state, change) {
                Apply::Publish => self.publish(),
                Apply::Refetch => self.attempt += 1,
                Apply::Ignore => {}
            },
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
            code: cap(&badge(labels, code, name)),
            name: cap(name),
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

fn cap(text: &str) -> String {
    text.chars().take(NAME_CAP).collect()
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend(names: &[&str], codes: &[&str], current: Option<usize>) -> BackendLayouts {
        BackendLayouts {
            names: names.iter().map(|name| (*name).to_owned()).collect(),
            codes: codes.iter().map(|code| (*code).to_owned()).collect(),
            current,
        }
    }

    fn window(id: u64, app_id: &str, focused: bool) -> WindowInfo {
        WindowInfo {
            id,
            title: None,
            app_id: Some(app_id.to_owned()),
            workspace: None,
            focused,
            floating: false,
            urgent: false,
            order: None,
        }
    }

    #[test]
    fn two_layouts_publish_uppercase_codes() {
        let published = payload(
            &backend(&["English (US)", "Russian"], &["us", "ru"], Some(1)),
            &BTreeMap::new(),
        );
        assert_eq!(
            published.layouts,
            [
                KeyboardLayout {
                    code: "US".to_owned(),
                    name: "English (US)".to_owned(),
                },
                KeyboardLayout {
                    code: "RU".to_owned(),
                    name: "Russian".to_owned(),
                },
            ]
        );
        assert_eq!(published.current, Some(1));
    }

    #[test]
    fn a_label_overrides_the_badge() {
        let mut labels = BTreeMap::new();
        labels.insert("us".to_owned(), "EN".to_owned());
        let published = payload(&backend(&["English (US)"], &["us"], Some(0)), &labels);
        assert_eq!(published.layouts[0].code, "EN");
    }

    #[test]
    fn an_empty_list_has_no_current() {
        let published = payload(&BackendLayouts::default(), &BTreeMap::new());
        assert!(published.layouts.is_empty());
        assert_eq!(published.current, None);
    }

    #[test]
    fn a_current_past_the_end_is_dropped() {
        let published = payload(
            &backend(&["English (US)"], &["us"], Some(4)),
            &BTreeMap::new(),
        );
        assert_eq!(published.current, None);
    }

    #[test]
    fn names_are_capped_by_characters() {
        let long = "я".repeat(NAME_CAP + 8);
        let published = payload(&backend(&[&long], &["ru"], Some(0)), &BTreeMap::new());
        assert_eq!(published.layouts[0].name.chars().count(), NAME_CAP);
    }

    #[test]
    fn a_layout_switch_updates_the_current_index() {
        let mut state = backend(&["English (US)", "Russian"], &["us", "ru"], Some(0));
        assert!(matches!(
            apply_keyboard(
                &mut state,
                Change::KeyboardLayoutSwitched {
                    idx: 1,
                    name: Some("Russian".to_owned()),
                }
            ),
            Apply::Publish
        ));
        assert_eq!(state.current, Some(1));
    }

    #[test]
    fn a_keyboard_resync_asks_for_a_new_snapshot() {
        let mut state = BackendLayouts::default();
        assert!(matches!(
            apply_keyboard(&mut state, Change::Resync(Resync::Keyboard)),
            Apply::Refetch
        ));
        assert!(matches!(
            apply_keyboard(&mut state, Change::Resync(Resync::Structure)),
            Apply::Ignore
        ));
    }

    #[test]
    fn global_remember_has_no_key() {
        assert_eq!(
            remember_key(Remember::Global, Some(&window(1, "firefox", true))),
            None
        );
    }

    #[test]
    fn window_remember_keys_on_the_focused_id() {
        assert_eq!(
            remember_key(Remember::Window, Some(&window(7, "firefox", true))),
            Some(StoreKey::Window(7))
        );
    }

    #[test]
    fn app_remember_keys_on_the_app_id() {
        assert_eq!(
            remember_key(Remember::App, Some(&window(7, "firefox", true))),
            Some(StoreKey::App("firefox".to_owned()))
        );
        assert_eq!(
            remember_key(
                Remember::App,
                Some(&WindowInfo {
                    app_id: None,
                    ..window(7, "", true)
                })
            ),
            None
        );
    }

    #[test]
    fn next_and_prev_wrap() {
        assert_eq!(stepped(1, LayoutRef::Next, 2), Some(0));
        assert_eq!(stepped(0, LayoutRef::Prev, 2), Some(1));
    }
}
