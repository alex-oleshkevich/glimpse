mod paths;
mod sources;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use glimpse_utils::clean;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, Service},
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

pub type PlacesHandle = crate::ServiceEndpoint<Places>;

impl crate::ServiceEndpoint<Places> {
    pub async fn empty_trash(&self) -> Result<(), CommandError> {
        self.call("places stopped while emptying the trash", |reply| {
            Command::EmptyTrash { reply }
        })
        .await?
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
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Watch;

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

    async fn start(ctx: &Ctx<Self>, config: Self::Config, (): Self::Dependencies) -> Self {
        Self {
            state: ctx.publisher(),
            roots: Roots::resolve(),
            config,
            failures: BTreeMap::new(),
        }
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
mod tests;
