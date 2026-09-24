mod paths;
mod sources;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use glimpse_utils::clean;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, Service, ServiceError},
    subscription::Sub,
};

use paths::Roots;
use sources::Event;

pub(crate) const ENTRIES: usize = 256;
const NAME_CAP: usize = 120;
const REASON: usize = 240;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Home,
    Desktop,
    Documents,
    Download,
    Music,
    Pictures,
    PublicShare,
    Templates,
    Videos,
    Other,
    Bookmark,
    Network,
}

impl Kind {
    pub fn icon_name(self) -> &'static str {
        match self {
            Self::Home => "user-home-symbolic",
            Self::Desktop => "user-desktop-symbolic",
            Self::Documents => "folder-documents-symbolic",
            Self::Download => "folder-download-symbolic",
            Self::Music => "folder-music-symbolic",
            Self::Pictures => "folder-pictures-symbolic",
            Self::PublicShare => "folder-publicshare-symbolic",
            Self::Templates => "folder-templates-symbolic",
            Self::Videos => "folder-videos-symbolic",
            Self::Other => "folder-symbolic",
            Self::Bookmark => "user-bookmarks-symbolic",
            Self::Network => "folder-remote-symbolic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub kind: Kind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlacesState {
    pub places: Vec<Place>,
    pub bookmarks: Vec<Place>,
    pub network: Vec<Place>,
    pub trash: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    trash: bool,
    network: bool,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            trash: document.places.trash,
            network: document.places.network,
        }
    }
}

#[derive(Clone)]
pub struct PlacesHandle(crate::ServiceEndpoint<Places>);

impl PlacesHandle {
    pub fn snapshot(&self) -> PlacesState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<PlacesState> {
        self.0.subscribe()
    }

    pub fn health(&self) -> tokio::sync::watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    pub async fn empty_trash(&self) -> Result<(), CommandError> {
        let (reply, result) = tokio::sync::oneshot::channel();
        self.0.command(Command::EmptyTrash { reply })?;
        result.await.map_err(|_| {
            CommandError::Unavailable("places stopped while emptying the trash".to_owned())
        })?
    }
}

pub enum Command {
    EmptyTrash {
        reply: tokio::sync::oneshot::Sender<Result<(), CommandError>>,
    },
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Watch {
    Xdg,
    Bookmarks,
    Network,
    Trash,
}

pub struct Places {
    state: Publisher<PlacesState>,
    roots: Roots,
    config: Config,
    failures: BTreeMap<&'static str, String>,
}

impl Service for Places {
    const NAME: &'static str = "places";
    type Config = Config;
    type State = PlacesState;
    type Handle = PlacesHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Watch;

    fn from_endpoint(endpoint: crate::ServiceEndpoint<Self>) -> Self::Handle {
        PlacesHandle(endpoint)
    }

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let mut subs = vec![
            Sub::stream(Watch::Xdg, {
                let config_dir = self.roots.config.clone();
                let home = self.roots.home.clone();
                move |_ctx| async move { sources::xdg_source(config_dir, home) }
            }),
            Sub::stream(Watch::Bookmarks, {
                let config_dir = self.roots.config.clone();
                move |_ctx| async move { sources::bookmarks_source(config_dir) }
            }),
        ];
        if self.config.trash {
            let trash_files = self.roots.trash_files.clone();
            subs.push(Sub::stream(Watch::Trash, move |_ctx| async move {
                sources::trash_source(trash_files)
            }));
        }
        if self.config.network {
            let gvfs = self.roots.gvfs.clone();
            subs.push(Sub::stream(Watch::Network, move |_ctx| async move {
                sources::network_source(gvfs)
            }));
        }
        subs
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        (): Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            state: ctx.publisher(),
            roots: Roots::resolve(),
            config,
            failures: BTreeMap::new(),
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(Command::EmptyTrash { reply }) => {
                let Some(root) = self.roots.trash_files.parent().map(Path::to_path_buf) else {
                    let _ = reply.send(Err(CommandError::Unavailable(
                        "there is no trash directory".to_owned(),
                    )));
                    return;
                };
                ctx.spawn_detached(move |_ctx| async move {
                    let outcome = tokio::task::spawn_blocking(move || empty_trash(&root))
                        .await
                        .map_err(|error| error.to_string())
                        .and_then(|result| result)
                        .map_err(|reason| CommandError::Internal(clean(&reason, REASON)));
                    let _ = reply.send(outcome);
                });
            }
            Input::Config(config) => self.config = config,
            Input::Event(Event::Xdg(result)) => {
                self.settle(ctx, "xdg", result, |state, places| state.places = places);
            }
            Input::Event(Event::Bookmarks(result)) => {
                self.settle(ctx, "bookmarks", result, |state, places| {
                    state.bookmarks = places;
                });
            }
            Input::Event(Event::Network(result)) => {
                self.settle(ctx, "network", result, |state, places| {
                    state.network = places;
                });
            }
            Input::Event(Event::Trash(result)) => {
                self.settle(ctx, "trash", result, |state, count| {
                    state.trash = Some(count);
                });
            }
        }
    }
}

fn empty_trash(root: &Path) -> Result<(), String> {
    for folder in ["files", "info", "expunged"] {
        let entries = match std::fs::read_dir(root.join(folder)) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("{folder}: {error}")),
        };
        for entry in entries {
            let path = entry.map_err(|error| error.to_string())?.path();
            let removed = match std::fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(&path),
                Ok(_) => std::fs::remove_file(&path),
                Err(error) => Err(error),
            };
            removed.map_err(|error| format!("{}: {error}", path.display()))?;
        }
    }
    match std::fs::remove_file(root.join("directorysizes")) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
        _ => Ok(()),
    }
}

impl Places {
    fn settle<T>(
        &mut self,
        ctx: &Ctx<Self>,
        source: &'static str,
        result: Result<T, String>,
        apply: impl FnOnce(&mut PlacesState, T),
    ) {
        match result {
            Ok(value) => {
                self.failures.remove(source);
                self.state.update(move |state| apply(state, value));
            }
            Err(reason) => {
                self.failures.insert(source, reason);
            }
        }
        self.report(ctx);
    }

    fn report(&self, ctx: &Ctx<Self>) {
        if self.failures.is_empty() {
            ctx.running();
            return;
        }
        let reasons = self
            .failures
            .iter()
            .map(|(source, reason)| format!("{source}: {reason}"))
            .collect::<Vec<_>>()
            .join("; ");
        ctx.degraded(clean(&reasons, REASON));
    }
}

#[cfg(test)]
mod tests {
    use glimpse_dbus::Buses;
    use tokio_util::sync::CancellationToken;

    use super::*;

    async fn places(
        config: Config,
    ) -> (
        Places,
        Ctx<Places>,
        tokio::sync::watch::Receiver<PlacesState>,
        tokio::sync::watch::Receiver<crate::ServiceState>,
    ) {
        let cancel = CancellationToken::new();
        let (events, _inbox) = tokio::sync::mpsc::channel(8);
        let (state, state_rx) = tokio::sync::watch::channel(PlacesState::default());
        let (health, health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
        let ctx = Ctx::<Places>::new(
            events,
            &cancel,
            state,
            health,
            Buses::unavailable("no bus in tests"),
        );
        let service = Places::start(&ctx, config, ()).await.expect("starts");
        (service, ctx, state_rx, health_rx)
    }

    fn enabled() -> Config {
        Config {
            trash: true,
            network: true,
        }
    }

    #[tokio::test]
    async fn trash_and_network_are_neither_read_nor_watched_when_turned_off() {
        let (service, ..) = places(Config {
            trash: false,
            network: false,
        })
        .await;

        let declared = service.subscriptions();
        let keys: Vec<&Watch> = declared.iter().map(Sub::key).collect();

        assert!(keys.contains(&&Watch::Xdg));
        assert!(keys.contains(&&Watch::Bookmarks));
        assert!(
            !keys.contains(&&Watch::Trash),
            "trash must not be declared when turned off"
        );
        assert!(
            !keys.contains(&&Watch::Network),
            "network must not be declared when turned off"
        );
    }

    #[tokio::test]
    async fn trash_and_network_are_declared_when_turned_on() {
        let (service, ..) = places(enabled()).await;

        let declared = service.subscriptions();
        let keys: Vec<&Watch> = declared.iter().map(Sub::key).collect();

        assert!(keys.contains(&&Watch::Trash));
        assert!(keys.contains(&&Watch::Network));
    }

    #[tokio::test]
    async fn a_failed_source_publishes_nothing_and_degrades_health() {
        let (mut service, ctx, state, health) = places(enabled()).await;

        service
            .handle(
                &ctx,
                Input::Event(Event::Network(Ok(vec![Place {
                    id: "share".to_owned(),
                    name: "share".to_owned(),
                    path: PathBuf::from("/run/user/1000/gvfs/share"),
                    kind: Kind::Network,
                }]))),
            )
            .await;
        assert_eq!(state.borrow().network.len(), 1);

        service
            .handle(
                &ctx,
                Input::Event(Event::Network(Err(
                    "its directory cannot be read: permission denied".to_owned(),
                ))),
            )
            .await;

        assert_eq!(
            state.borrow().network.len(),
            1,
            "a failed read must not overwrite the last known state"
        );
        assert!(matches!(
            &*health.borrow(),
            crate::ServiceState::Degraded { .. }
        ));
    }

    #[tokio::test]
    async fn health_recovers_once_every_failing_source_succeeds_again() {
        let (mut service, ctx, _state, health) = places(enabled()).await;

        service
            .handle(
                &ctx,
                Input::Event(Event::Trash(Err(
                    "its directory cannot be read: other".to_owned()
                ))),
            )
            .await;
        assert!(matches!(
            &*health.borrow(),
            crate::ServiceState::Degraded { .. }
        ));

        service
            .handle(&ctx, Input::Event(Event::Trash(Ok(0))))
            .await;
        assert!(matches!(&*health.borrow(), crate::ServiceState::Running));
    }

    #[test]
    fn emptying_the_trash_clears_files_and_their_sidecars_but_keeps_the_folders() {
        let root = tempfile::tempdir().expect("a scratch directory");
        let trash = root.path();
        for dir in ["files/folder/inner", "info"] {
            std::fs::create_dir_all(trash.join(dir)).expect("a trash folder");
        }
        std::fs::write(trash.join("files/a.txt"), "a").expect("a trashed file");
        std::fs::write(trash.join("files/folder/inner/b"), "b").expect("a nested file");
        std::fs::write(trash.join("info/a.txt.trashinfo"), "[Trash Info]").expect("a sidecar");
        std::fs::write(trash.join("directorysizes"), "").expect("a size cache");
        let outside = root.path().join("outside");
        std::fs::write(&outside, "keep").expect("a file outside the trash");
        std::os::unix::fs::symlink(&outside, trash.join("files/link")).expect("a symlink");

        empty_trash(trash).expect("emptied");

        for dir in ["files", "info"] {
            assert_eq!(
                std::fs::read_dir(trash.join(dir)).expect(dir).count(),
                0,
                "{dir} is emptied"
            );
        }
        assert!(!trash.join("directorysizes").exists());
        assert!(
            outside.exists(),
            "a trashed symlink is removed, never followed"
        );
    }

    #[tokio::test]
    async fn the_state_before_any_event_has_arrived_is_empty_and_the_service_is_not_degraded() {
        let (_service, _ctx, state, health) = places(enabled()).await;

        assert_eq!(*state.borrow(), PlacesState::default());
        assert!(!matches!(
            &*health.borrow(),
            crate::ServiceState::Degraded { .. }
        ));
    }
}
