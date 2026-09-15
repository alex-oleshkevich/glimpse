mod item;
mod menu;
mod watcher;

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use glimpse_dbus::dbusmenu::MenuNode;
use glimpse_dbus::status_notifier_item::TrayItem;
use glimpse_dbus::status_notifier_watcher::{Registry, WATCHER_NAME};

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, NoConfig, Service, ServiceError},
    subscription::Sub,
};

/// Every item the watcher knows about, in the order a bar should render them. The applet decides
/// which of them it shows; the service publishes all of them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrayItems {
    pub items: Vec<TrayItem>,
}

/// One declared source per thing being followed. Adding a key starts its follower on the next
/// reconcile and removing it drops the guard, which is what releases the match rule — there is no
/// teardown code and no second map.
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Watch {
    /// `NameOwnerChanged`, which is how a dead item actually leaves the bar: most applications
    /// never call `UnregisterStatusNotifierItem`.
    Names,
    /// The foreign watcher's own registration signals, followed only while hosting on someone
    /// else's watcher — when the name is ours, these are signals we emit.
    Foreign,
    Item(String),
    /// The path is in the key: an item that emits `NewMenu` with a different object path must tear
    /// the old follower down, or it keeps watching a path that no longer exists.
    Menu(String, String),
}

pub enum Command {
    Activate {
        key: String,
        x: i32,
        y: i32,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    SecondaryActivate {
        key: String,
        x: i32,
        y: i32,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    ContextMenu {
        key: String,
        x: i32,
        y: i32,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Scroll {
        key: String,
        delta: i32,
        orientation: String,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Menu {
        key: String,
        reply: oneshot::Sender<Result<MenuNode, CommandError>>,
    },
    MenuEvent {
        key: String,
        id: i32,
        event: String,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
}

pub enum Event {
    /// Take the watcher name, from inside the source that carries the retry.
    Claim,
    Registered(String),
    Unregistered(String),
    /// The outcome of a claim attempt, carried back from the task that did the bus work.
    Claimed(Result<(), String>),
    /// The items another watcher lists, after registering with it as a host. Also the answer to a
    /// `Resync`, so adoption has one path rather than two.
    Attached(Result<Vec<String>, String>),
    /// The foreign watcher's item set moved; read it again.
    Resync,
    /// `name` is whichever bus name moved; `new_owner` is `None` when it was released.
    NameOwnerChanged {
        name: String,
        new_owner: Option<String>,
    },
    ItemChanged(String, Box<TrayItem>),
    ItemGone(String),
    MenuChanged(String, u32),
    /// The menu's *contents* moved with no revision to compare against.
    MenuStale(String),
    MenuStatus(String, bool),
    MenuFetched(String, u32, Box<MenuNode>),
    Unavailable(String),
}

pub struct Tray {
    items: Publisher<TrayItems>,
    registry: watcher::Shared,
    claimed: bool,
    /// A claim is in flight. Its bus work runs off the handler, so without this a second trigger
    /// arriving before the outcome does would start a duplicate.
    claiming: bool,
    /// Registered as a host on someone else's watcher, because the name was already taken.
    hosting: bool,
    known: BTreeMap<String, TrayItem>,
    menus: HashMap<String, (u32, MenuNode)>,
    notices: HashMap<String, bool>,
}

#[derive(Clone)]
pub struct TrayHandle(crate::ServiceEndpoint<Tray>);

impl TrayHandle {
    pub fn snapshot(&self) -> TrayItems {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<TrayItems> {
        self.0.subscribe()
    }

    pub fn health(&self) -> tokio::sync::watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    pub async fn activate(&self, key: String, x: i32, y: i32) -> Result<(), CommandError> {
        self.call(|reply| Command::Activate { key, x, y, reply })
            .await
    }

    pub async fn secondary_activate(
        &self,
        key: String,
        x: i32,
        y: i32,
    ) -> Result<(), CommandError> {
        self.call(|reply| Command::SecondaryActivate { key, x, y, reply })
            .await
    }

    pub async fn context_menu(&self, key: String, x: i32, y: i32) -> Result<(), CommandError> {
        self.call(|reply| Command::ContextMenu { key, x, y, reply })
            .await
    }

    pub async fn scroll(
        &self,
        key: String,
        delta: i32,
        orientation: String,
    ) -> Result<(), CommandError> {
        self.call(|reply| Command::Scroll {
            key,
            delta,
            orientation,
            reply,
        })
        .await
    }

    /// Fetched on pointer-enter rather than on click: `AboutToShow` is where an application fills
    /// a menu in, and doing it on the click shows an empty one first.
    pub async fn menu(&self, key: String) -> Result<MenuNode, CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::Menu { key, reply })?;
        result.await.map_err(|_| stopped())?
    }

    pub async fn menu_event(
        &self,
        key: String,
        id: i32,
        event: String,
    ) -> Result<(), CommandError> {
        self.call(|reply| Command::MenuEvent {
            key,
            id,
            event,
            reply,
        })
        .await
    }

    async fn call(
        &self,
        command: impl FnOnce(oneshot::Sender<Result<(), CommandError>>) -> Command,
    ) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(command(reply))?;
        result.await.map_err(|_| stopped())?
    }
}

fn stopped() -> CommandError {
    CommandError::Unavailable("the tray stopped before completing the command".to_owned())
}

impl Service for Tray {
    const NAME: &'static str = "tray";
    type Config = NoConfig;
    type State = TrayItems;
    type Handle = TrayHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Watch;

    fn from_endpoint(endpoint: crate::ServiceEndpoint<Self>) -> Self::Handle {
        TrayHandle(endpoint)
    }

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let mut subs = vec![Sub::stream(Watch::Names, watcher::name_changes)];
        if self.hosting {
            subs.push(Sub::stream(Watch::Foreign, watcher::foreign_changes));
        }
        for key in self.keys() {
            let owned = key.clone();
            subs.push(Sub::stream(Watch::Item(key.clone()), move |ctx| {
                item::follow(ctx, owned)
            }));
            if let Some(path) = self.known.get(&key).and_then(|item| item.menu.clone()) {
                let owned = key.clone();
                let followed = path.clone();
                subs.push(Sub::stream(Watch::Menu(key, path), move |ctx| {
                    menu::follow(ctx, owned, followed)
                }));
            }
        }
        subs
    }

    async fn start(
        ctx: &Ctx<Self>,
        _config: Self::Config,
        _: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        // The claim is not made here: see `watcher::name_changes`. `start` returns before the
        // runtime installs any source, so claiming now would race the very signal that recovers it.
        let registry: watcher::Shared = Arc::new(Mutex::new(Registry::default()));
        let mut tray = Self {
            items: ctx.publisher(),
            registry,
            claimed: false,
            claiming: false,
            hosting: false,
            known: BTreeMap::new(),
            menus: HashMap::new(),
            notices: HashMap::new(),
        };
        tray.republish();
        Ok(tray)
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(command) => self.run(ctx, command),
            Input::Event(Event::Registered(key)) => {
                tracing::debug!(%key, "following a new tray item");
            }
            Input::Event(Event::Unregistered(key)) => {
                self.forget(&key);
                self.republish();
            }
            Input::Event(Event::ItemChanged(key, item)) => {
                // Following `PropertiesChanged` *and* every `New*` signal means a well-behaved
                // application delivers the same snapshot two or three times. Gate on the one item
                // that moved rather than deep-cloning every item to let the publisher find out.
                if self.known.get(&key).is_some_and(|held| held == &*item) {
                    return;
                }
                // A cached tree belongs to the path it came from. `NewMenu` can move an item's
                // menu to a path whose revision starts where the old one was, and the cache would
                // then answer with the previous menu forever.
                let moved = self
                    .known
                    .get(&key)
                    .is_some_and(|held| held.menu != item.menu);
                if moved {
                    self.menus.remove(&key);
                }
                self.known.insert(key, *item);
                self.republish();
            }
            Input::Event(Event::ItemGone(key)) => {
                if let Ok(mut registry) = self.registry.lock() {
                    registry.unregister_key(&key);
                }
                self.forget(&key);
                self.republish();
            }
            Input::Event(Event::MenuFetched(key, revision, node)) => {
                self.menus.insert(key, (revision, *node));
            }
            Input::Event(Event::MenuStatus(key, notice)) => {
                if self.notices.get(&key).copied().unwrap_or_default() != notice {
                    self.notices.insert(key, notice);
                    self.republish();
                }
            }
            Input::Event(Event::MenuStale(key)) => {
                self.menus.remove(&key);
            }
            Input::Event(Event::MenuChanged(key, revision)) => {
                if self
                    .menus
                    .get(&key)
                    .is_none_or(|(known, _)| *known != revision)
                {
                    self.menus.remove(&key);
                }
            }
            Input::Event(Event::Claim) => self.claim(ctx),
            Input::Event(Event::Claimed(outcome)) => {
                self.claiming = false;
                match outcome {
                    Ok(()) => {
                        self.claimed = true;
                        self.hosting = false;
                        ctx.running();
                        self.republish();
                    }
                    // Not ours is not the end of it: a session usually has a second shell component
                    // wanting the tray, and only one of them can be the watcher.
                    Err(reason) => {
                        tracing::info!(%reason, "the watcher is someone else's; registering with it as a host");
                        self.attach(ctx, reason);
                    }
                }
            }
            Input::Event(Event::Attached(outcome)) => match outcome {
                Ok(keys) => {
                    tracing::info!(
                        items = keys.len(),
                        "hosting on another process's tray watcher"
                    );
                    self.hosting = true;
                    self.adopt(keys);
                    ctx.running();
                    self.republish();
                }
                Err(reason) => {
                    tracing::warn!(
                        %reason,
                        "could not become the tray watcher, and the one that is would not have us as a host"
                    );
                    ctx.degraded(reason);
                }
            },
            Input::Event(Event::Resync) => {
                tracing::debug!("the foreign watcher's item set moved; reading it again");
                self.resync(ctx);
            }
            Input::Event(Event::NameOwnerChanged { name, new_owner }) => {
                self.name_moved(ctx, &name, new_owner.as_deref()).await;
            }
            Input::Event(Event::Unavailable(reason)) => {
                ctx.degraded(reason);
                self.items.set(TrayItems::default());
            }
            Input::Config(NoConfig) => {}
        }
    }
}

impl Tray {
    /// One match rule carries every name on the bus, so this is where the three cases are told
    /// apart: the watcher name freeing, our own ownership ending, and an item's owner vanishing.
    async fn name_moved(&mut self, ctx: &Ctx<Self>, name: &str, new_owner: Option<&str>) {
        if name == WATCHER_NAME {
            match (self.claimed, new_owner) {
                // One `claim` for every path — cold start, a name freeing, and our own loss.
                // Writing them as separate paths is how they drift.
                (false, None) => self.claim(ctx),
                (true, Some(_)) => {}
                (true, None) => {
                    // The signal that brought us here *was* the release, so waiting for another
                    // would wait forever: the name is free right now. Take it back.
                    self.claimed = false;
                    self.forget_everything();
                    self.claim(ctx);
                }
                // A watcher appeared and it is not us. Register with it rather than sit empty —
                // and re-register when it is replaced, since the new one knows nothing of us.
                (false, Some(_)) => {
                    if !self.claiming {
                        self.attach(ctx, format!("{WATCHER_NAME} is owned by another process"));
                    }
                }
            }
            return;
        }

        if new_owner.is_some() {
            return;
        }
        let lost = self
            .registry
            .lock()
            .map(|mut registry| registry.evict_owner(name))
            .unwrap_or_default();
        if !lost.is_empty() {
            tracing::debug!(
                owner = name,
                count = lost.len(),
                "a tray item's owner vanished"
            );
            // Unique bus names are never reused, so leaving these entries behind grows the maps
            // for the panel's lifetime as applications restart.
            for key in &lost {
                self.forget(key);
            }
            self.republish();
        }
    }

    /// A watcher that no longer holds the name serves nothing rather than a stale list — and holds
    /// nothing either, or repeated loss-and-reclaim cycles grow these maps without bound.
    fn forget_everything(&mut self) {
        if let Ok(mut registry) = self.registry.lock() {
            *registry = Registry::default();
        }
        self.known.clear();
        self.menus.clear();
        self.notices.clear();
        self.items.set(TrayItems::default());
    }

    /// Taking the name means a name request, a host registration, a `ListNames` and a property
    /// probe per candidate. Handlers run serially on `&mut self`, so awaiting that here would put
    /// every other tray event behind one non-answering application's zbus timeout.
    fn claim(&mut self, ctx: &Ctx<Self>) {
        if self.claiming {
            return;
        }
        self.claiming = true;
        let registry = self.registry.clone();
        ctx.spawn_detached(move |ctx| async move {
            let outcome = watcher::claim(&ctx, &registry).await;
            if outcome.is_ok() {
                watcher::sweep(&ctx, &registry).await;
            }
            let _ = ctx
                .events()
                .send(Input::Event(Event::Claimed(outcome)))
                .await;
        });
    }

    /// Register with whoever holds the name and take their item list. Off the handler for the same
    /// reason a claim is: it is a name request, a registration and a lookup per item.
    fn attach(&mut self, ctx: &Ctx<Self>, reason: String) {
        ctx.spawn_detached(move |ctx| async move {
            let outcome = watcher::attach(&ctx)
                .await
                .map_err(|error| format!("{reason}, and it refused us as a host: {error}"));
            let _ = ctx
                .events()
                .send(Input::Event(Event::Attached(outcome)))
                .await;
        });
    }

    /// The foreign watcher said its set moved. Read it whole rather than applying the one signal —
    /// `Registry::resync` carries why.
    fn resync(&self, ctx: &Ctx<Self>) {
        ctx.spawn_detached(|ctx| async move {
            let Ok(connection) = ctx.session_bus().cloned() else {
                return;
            };
            let outcome = watcher::list_items(&connection).await;
            let _ = ctx
                .events()
                .send(Input::Event(Event::Attached(outcome)))
                .await;
        });
    }

    /// Take a foreign watcher's list as the whole truth, dropping what it no longer lists.
    fn adopt(&mut self, keys: Vec<String>) {
        let gone = self
            .registry
            .lock()
            .map(|mut registry| registry.resync(&keys))
            .unwrap_or_default();
        for key in gone {
            self.forget(&key);
        }
    }

    fn keys(&self) -> Vec<String> {
        self.registry
            .lock()
            .map(|registry| registry.items().to_vec())
            .unwrap_or_default()
    }

    fn forget(&mut self, key: &str) {
        self.known.remove(key);
        self.menus.remove(key);
        self.notices.remove(key);
    }

    /// Registration order, and only items that have actually answered: a key with no snapshot yet
    /// would render as a blank chip that fills in a frame later.
    fn republish(&mut self) {
        let items = self
            .keys()
            .into_iter()
            .filter_map(|key| {
                let mut item = self.known.get(&key).cloned()?;
                item.notice = self.notices.get(&key).copied().unwrap_or_default();
                Some(item)
            })
            .collect();
        self.items.set(TrayItems { items });
    }

    fn run(&mut self, ctx: &Ctx<Self>, command: Command) {
        let Ok(connection) = ctx.session_bus().cloned() else {
            reject(command, "there is no session bus");
            return;
        };
        let menu_path = |key: &str| self.known.get(key).and_then(|item| item.menu.clone());
        let known_revision = |key: &str| self.menus.get(key).map(|(revision, _)| *revision);

        match command {
            Command::Activate { key, x, y, reply } => {
                deadline(ctx, reply, async move {
                    item_proxy(&connection, &key)
                        .await?
                        .activate(x, y)
                        .await
                        .map_err(unavailable)
                });
            }
            Command::SecondaryActivate { key, x, y, reply } => {
                deadline(ctx, reply, async move {
                    let item = item_proxy(&connection, &key).await?;
                    match item.secondary_activate(x, y).await {
                        Ok(()) => Ok(()),
                        // An Ayatana item may offer only the timestamped form, and answers
                        // `UnknownMethod` to the standard one. The proxy member exists for this.
                        Err(zbus::Error::MethodError(name, _, _))
                            if name.as_str() == "org.freedesktop.DBus.Error.UnknownMethod" =>
                        {
                            item.x_ayatana_secondary_activate(seconds())
                                .await
                                .map_err(unavailable)
                        }
                        Err(error) => Err(unavailable(error)),
                    }
                });
            }
            Command::ContextMenu { key, x, y, reply } => {
                deadline(ctx, reply, async move {
                    item_proxy(&connection, &key)
                        .await?
                        .context_menu(x, y)
                        .await
                        .map_err(unavailable)
                });
            }
            Command::Scroll {
                key,
                delta,
                orientation,
                reply,
            } => {
                deadline(ctx, reply, async move {
                    item_proxy(&connection, &key)
                        .await?
                        .scroll(delta, &orientation)
                        .await
                        .map_err(unavailable)
                });
            }
            Command::Menu { key, reply } => {
                let Some(path) = menu_path(&key) else {
                    let _ = reply.send(Err(CommandError::Unavailable(
                        "the item offers no menu".to_owned(),
                    )));
                    return;
                };
                let cached = self.menus.get(&key).map(|(_, node)| node.clone());
                let revision = known_revision(&key);
                let events = ctx.events();
                deadline(ctx, reply, async move {
                    match menu::fetch(&connection, &key, &path, revision).await? {
                        Some((revision, node)) => {
                            let _ = events.try_send(Input::Event(Event::MenuFetched(
                                key,
                                revision,
                                Box::new(node.clone()),
                            )));
                            Ok(node)
                        }
                        None => cached.ok_or_else(|| {
                            CommandError::Unavailable("the menu is not loaded".to_owned())
                        }),
                    }
                });
            }
            Command::MenuEvent {
                key,
                id,
                event,
                reply,
            } => {
                let Some(path) = menu_path(&key) else {
                    let _ = reply.send(Err(CommandError::Unavailable(
                        "the item offers no menu".to_owned(),
                    )));
                    return;
                };
                deadline(ctx, reply, async move {
                    menu::event(&connection, &key, &path, id, &event).await
                });
            }
        }
    }
}

/// Handlers run serially on `&mut self`, so a command that reaches an application is answered from
/// a detached task under a deadline. One hung application otherwise freezes every other item — a
/// failure that looks like the whole tray being broken.
fn deadline<T: Send + 'static>(
    ctx: &Ctx<Tray>,
    reply: oneshot::Sender<Result<T, CommandError>>,
    work: impl Future<Output = Result<T, CommandError>> + Send + 'static,
) {
    ctx.spawn_detached(move |_ctx| async move {
        let outcome = match tokio::time::timeout(glimpse_dbus::DEADLINE, work).await {
            Ok(result) => result,
            Err(_) => Err(CommandError::Unavailable(
                "the application did not answer".to_owned(),
            )),
        };
        let _ = reply.send(outcome);
    });
}

fn reject(command: Command, reason: &str) {
    let reason = CommandError::Unavailable(reason.to_owned());
    match command {
        Command::Activate { reply, .. }
        | Command::SecondaryActivate { reply, .. }
        | Command::ContextMenu { reply, .. }
        | Command::Scroll { reply, .. }
        | Command::MenuEvent { reply, .. } => {
            let _ = reply.send(Err(reason));
        }
        Command::Menu { reply, .. } => {
            let _ = reply.send(Err(reason));
        }
    }
}

async fn item_proxy(
    connection: &zbus::Connection,
    key: &str,
) -> Result<glimpse_dbus::status_notifier_item::StatusNotifierItemProxy<'static>, CommandError> {
    let (owner, path) = glimpse_dbus::status_notifier_watcher::split_key(key)
        .ok_or_else(|| CommandError::Unavailable("that item is not on the bar".to_owned()))?;
    glimpse_dbus::status_notifier_item::StatusNotifierItemProxy::builder(connection)
        .destination(owner.to_owned())
        .and_then(|builder| builder.path(path.to_owned()))
        .map_err(unavailable)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .map_err(unavailable)
}

fn seconds() -> u32 {
    chrono::Utc::now().timestamp() as u32
}

fn unavailable(error: zbus::Error) -> CommandError {
    CommandError::Unavailable(error.to_string())
}

#[cfg(test)]
mod tests {
    use glimpse_dbus::Buses;
    use tokio_util::sync::CancellationToken;

    use super::*;

    async fn tray() -> (
        Tray,
        Ctx<Tray>,
        tokio::sync::watch::Receiver<TrayItems>,
        tokio::sync::watch::Receiver<crate::ServiceState>,
    ) {
        let cancel = CancellationToken::new();
        let (events, _inbox) = tokio::sync::mpsc::channel(4);
        let (state, state_rx) = tokio::sync::watch::channel(TrayItems::default());
        let (health, health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
        let ctx = Ctx::<Tray>::new(
            events,
            &cancel,
            state,
            health,
            Buses::unavailable("no bus in tests"),
        );
        let tray = Tray::start(&ctx, NoConfig, ()).await.expect("starts");
        (tray, ctx, state_rx, health_rx)
    }

    #[tokio::test]
    async fn a_claim_that_failed_reaches_for_the_watcher_that_won_before_it_degrades() {
        let (mut tray, ctx, state, health) = tray().await;

        // The claim's bus work runs off the handler — awaiting name requests, `ListNames` and a
        // probe per candidate here would put every other tray event behind one hung application —
        // so the handler's contract is the *outcome*, not the attempt.
        tray.handle(
            &ctx,
            Input::Event(Event::Claimed(Err("no bus in tests".to_owned()))),
        )
        .await;

        assert!(
            !matches!(&*health.borrow(), crate::ServiceState::Degraded { .. }),
            "a taken name is not a verdict: the host registration has not been tried yet"
        );

        // Only when nobody will have us as a host either is the tray actually out of options.
        tray.handle(
            &ctx,
            Input::Event(Event::Attached(Err("no bus in tests".to_owned()))),
        )
        .await;

        assert_eq!(*state.borrow(), TrayItems::default());
        assert!(
            matches!(&*health.borrow(), crate::ServiceState::Degraded { reason } if reason.contains("no bus")),
            "the reason names why there is no bus, rather than saying the tray is broken"
        );
    }

    #[tokio::test]
    async fn a_foreign_watchers_list_replaces_ours_and_drops_what_it_stopped_listing() {
        let (mut tray, ctx, state, health) = tray().await;

        tray.handle(
            &ctx,
            Input::Event(Event::Attached(Ok(vec![
                ":1.1/StatusNotifierItem".to_owned(),
                ":1.2/StatusNotifierItem".to_owned(),
            ]))),
        )
        .await;
        assert_eq!(
            *health.borrow(),
            crate::ServiceState::Running,
            "hosting on someone else's watcher is a working tray, not a degraded one"
        );

        // Nothing has answered a `GetAll` yet, so the bar stays empty: a key with no snapshot
        // behind it would render as a blank chip that fills in a frame later.
        assert_eq!(*state.borrow(), TrayItems::default());

        tray.handle(
            &ctx,
            Input::Event(Event::Attached(Ok(vec![
                ":1.2/StatusNotifierItem".to_owned(),
            ]))),
        )
        .await;
        assert_eq!(
            tray.keys(),
            [":1.2/StatusNotifierItem"],
            "the foreign list is the whole truth, so a dropped entry leaves"
        );
    }

    #[tokio::test]
    async fn a_claim_is_requested_rather_than_awaited_so_one_hung_item_cannot_block_the_queue() {
        let (mut tray, ctx, _state, health) = tray().await;

        tray.handle(&ctx, Input::Event(Event::Claim)).await;

        assert!(
            !matches!(&*health.borrow(), crate::ServiceState::Degraded { .. }),
            "the handler returns immediately; the outcome arrives as its own event"
        );
    }

    #[tokio::test]
    async fn losing_the_backend_empties_the_bar_rather_than_leaving_stale_chips() {
        let (mut tray, ctx, mut state, health) = tray().await;
        tray.items.set(TrayItems {
            items: vec![glimpse_dbus::status_notifier_item::TrayItem {
                key: ":1.1/StatusNotifierItem".to_owned(),
                ..Default::default()
            }],
        });
        assert_eq!(state.borrow_and_update().items.len(), 1);

        tray.handle(
            &ctx,
            Input::Event(Event::Unavailable("the watcher name was taken".to_owned())),
        )
        .await;

        assert_eq!(state.borrow().items, Vec::new());
        assert!(
            matches!(&*health.borrow(), crate::ServiceState::Degraded { reason } if reason.contains("taken")),
        );
    }
}
