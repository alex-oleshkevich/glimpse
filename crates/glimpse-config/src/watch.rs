use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use futures_util::{Stream, StreamExt, stream};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::{broadcast, mpsc};

use crate::error::ConfigError;
use crate::load::{
    CONFIG_FILE, DATA_DIR, Documents, PANEL_DROPINS, PANEL_FILE, SYSTEM_DIR, load, push,
    push_target_parent, roots,
};
use crate::schema::Config;
use crate::theme::{THEMES_DIR, themes_dir_from_env};

/// How long a watched directory has to go quiet before its events are reported. An editor saving a
/// file produces a write, a rename over the target, and sometimes a delete and a create.
const DEBOUNCE: Duration = Duration::from_millis(250);
const EVENTS: usize = 256;
const SIGNALS: usize = 16;

static HUB: OnceLock<Hub> = OnceLock::new();

#[derive(Debug, PartialEq, Eq)]
pub enum Update {
    Changed(Vec<PathBuf>),
    /// The watch moved. Whatever happened while it was elsewhere produced no events and cannot be
    /// inferred, so the only correct response is to read everything again.
    Rearmed,
    Unavailable(String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Reload<T> {
    Changed(T),
    Recovered,
    Rejected(String),
}

/// One inotify instance for the whole set, however many directories it holds. Its reader blocks on
/// the descriptor, so a session where nothing is edited costs nothing at all.
pub fn watch_dirs(
    dirs: impl IntoIterator<Item = PathBuf>,
) -> impl Stream<Item = Update> + Send + 'static {
    stream::unfold(
        Watch::new(dirs.into_iter().collect()),
        |mut watch| async move { watch.next().await.map(|update| (update, watch)) },
    )
}

pub fn watch_config(
    documents: Documents,
    current: Config,
    failure: Option<String>,
) -> impl Stream<Item = Reload<Config>> + Send + 'static {
    reloads(
        || shared().subscribe(),
        Arc::new(move || load(documents)),
        current,
        failure,
        false,
    )
    .inspect(|reload| {
        if let Reload::Changed(config) = reload {
            for problem in config.validate() {
                tracing::warn!(%problem, "configuration problem");
            }
        }
    })
}

pub(crate) type Loader<T> = Arc<dyn Fn() -> Result<T, ConfigError> + Send + Sync>;

pub(crate) fn shared() -> &'static Hub {
    HUB.get_or_init(|| Hub::start(process_dirs))
}

fn process_dirs() -> Vec<PathBuf> {
    hub_dirs(
        &roots(),
        Path::new(SYSTEM_DIR),
        Path::new(DATA_DIR),
        themes_dir_from_env(),
    )
}

fn hub_dirs(
    roots: &[PathBuf],
    system: &Path,
    data: &Path,
    themes: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for root in roots.iter().filter(|root| root.as_path() != system) {
        push(&mut dirs, root.clone());
        push(&mut dirs, root.join(PANEL_DROPINS));
        push_themes(&mut dirs, root.join(THEMES_DIR));
        push_target_parent(&mut dirs, &root.join(CONFIG_FILE));
        push_target_parent(&mut dirs, &root.join(PANEL_FILE));
    }
    push_themes(&mut dirs, data.join(THEMES_DIR));
    if let Some(themes) = themes {
        push_themes(&mut dirs, themes);
    }
    dirs
}

fn push_themes(dirs: &mut Vec<PathBuf>, themes: PathBuf) {
    let mut children: Vec<PathBuf> = std::fs::read_dir(&themes)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    children.sort();
    push(dirs, themes);
    for child in children {
        push(dirs, child);
    }
}

pub(crate) struct Hub {
    signals: broadcast::Sender<()>,
}

impl Hub {
    pub(crate) fn start(dirs: impl Fn() -> Vec<PathBuf> + Send + 'static) -> Self {
        let (signals, _) = broadcast::channel(SIGNALS);
        let sender = signals.clone();
        let mut changes = Box::pin(changes(dirs));
        tokio::spawn(async move {
            while changes.next().await.is_some() {
                let _ = sender.send(());
            }
        });
        Self { signals }
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<()> {
        self.signals.subscribe()
    }
}

fn changes(
    dirs: impl Fn() -> Vec<PathBuf> + Send + 'static,
) -> impl Stream<Item = ()> + Send + 'static {
    let watched = stream::unfold((Watch::new(dirs()), dirs), |(mut watch, dirs)| async move {
        let update = watch.next().await?;
        if matches!(update, Update::Changed(_) | Update::Rearmed) {
            let wanted: Vec<PathBuf> = dirs().into_iter().map(absolute).collect();
            if wanted != watch.allowed {
                watch = Watch::new(wanted);
            }
        }
        Some((update, (watch, dirs)))
    })
    .filter_map(|update| async move {
        match update {
            Update::Changed(_) | Update::Rearmed => Some(()),
            Update::Unavailable(reason) => {
                tracing::warn!(
                    reason,
                    "the configuration is not being watched; SIGHUP still reloads it"
                );
                None
            }
        }
    });
    stream::select(Box::pin(watched), Box::pin(hangups()))
}

pub(crate) fn hangups() -> impl Stream<Item = ()> + Send + 'static {
    let hangup = signal(SignalKind::hangup())
        .inspect_err(|error| tracing::warn!(%error, "SIGHUP will not reload the configuration"))
        .ok();

    stream::unfold(hangup, |hangup| async move {
        let mut hangup = hangup?;
        hangup.recv().await?;
        Some(((), Some(hangup)))
    })
}

fn triggers(receiver: broadcast::Receiver<()>) -> impl Stream<Item = ()> + Send + 'static {
    stream::unfold(receiver, |mut receiver| async move {
        match receiver.recv().await {
            Err(broadcast::error::RecvError::Closed) => None,
            _ => Some(((), receiver)),
        }
    })
}

pub(crate) fn reloads<T>(
    subscribe: impl FnOnce() -> broadcast::Receiver<()> + Send + 'static,
    load: Loader<T>,
    current: T,
    failure: Option<String>,
    leading: bool,
) -> impl Stream<Item = Reload<T>> + Send + 'static
where
    T: Clone + PartialEq + Send + 'static,
{
    stream::once(async move {
        let reader = Reader {
            triggers: Box::pin(triggers(subscribe())),
            load,
            current,
            failure,
            leading,
        };
        stream::unfold(reader, |mut reader| async move {
            reader.next().await.map(|reload| (reload, reader))
        })
    })
    .flatten()
}

struct Reader<T> {
    triggers: Pin<Box<dyn Stream<Item = ()> + Send>>,
    load: Loader<T>,
    current: T,
    /// The last failure reported. A watched directory goes on producing events while a document
    /// stays broken — an editor's swap and backup files land right beside it — and without this
    /// every one of them reproduces the same line.
    failure: Option<String>,
    leading: bool,
}

impl<T: Clone + PartialEq + Send + 'static> Reader<T> {
    async fn next(&mut self) -> Option<Reload<T>> {
        let mut read = std::mem::take(&mut self.leading);
        loop {
            if !read && self.triggers.next().await.is_none() {
                return None;
            }
            read = false;
            let load = Arc::clone(&self.load);
            let loaded = match tokio::task::spawn_blocking(move || load()).await {
                Ok(loaded) => loaded.map_err(|error| error.to_string()),
                Err(error) => Err(format!("the configuration reader panicked: {error}")),
            };
            match loaded {
                // A document that loads is one that is no longer broken, whether or not it moved,
                // so the next failure is news again.
                Ok(loaded) => {
                    let recovered = self.failure.take().is_some();
                    if recovered {
                        tracing::info!("configuration recovered");
                    }
                    if loaded != self.current {
                        self.current = loaded;
                        return Some(Reload::Changed(self.current.clone()));
                    }
                    if recovered {
                        return Some(Reload::Recovered);
                    }
                }
                Err(reason) => {
                    if self.report(&reason) {
                        return Some(Reload::Rejected(reason));
                    }
                }
            }
        }
    }

    /// The same failure again is one document nobody has fixed yet, not a second problem.
    fn report(&mut self, reason: &str) -> bool {
        if self.failure.as_deref() == Some(reason) {
            tracing::debug!(reason, "dropped a reload, unchanged since the last");
            return false;
        }
        tracing::error!(
            reason,
            "dropped a reload, keeping the running configuration"
        );
        self.failure = Some(reason.to_owned());
        true
    }
}

struct Watch {
    arms: Vec<Arm>,
    /// The floor for the ancestor walk: a watch may fall back onto a directory only if that
    /// directory is itself one we were asked to watch.
    allowed: Vec<PathBuf>,
    watcher: Option<RecommendedWatcher>,
    events: mpsc::Receiver<Event>,
    pending: Option<Update>,
}

struct Arm {
    wanted: PathBuf,
    dir: Option<PathBuf>,
    inode: Option<u64>,
}

impl Arm {
    /// A watch is bound to an inode, not to a name, so `rm -rf` followed by a fresh clone leaves
    /// one armed on a directory nobody can reach: it reports nothing, forever, and looks identical
    /// to a directory where nothing happens. Comparing the inode tells those two apart unless the
    /// filesystem hands the new directory the freed inode, as ext4 does; the kernel's removal of
    /// the watched directory itself covers that case by clearing `dir`.
    ///
    /// Watching nothing is a settled state, not a displaced one. An arm with no watchable ancestor
    /// — the ordinary case for `/etc/glimpse/` on a machine with no system layer — would otherwise
    /// count as displaced on every event, and every event would re-arm instead of being reported.
    fn displaced(&self, allowed: &[PathBuf]) -> bool {
        match (nearest_existing(&self.wanted, allowed), self.dir.as_ref()) {
            (None, None) => false,
            (Some(current), Some(dir)) => current != *dir || inode(&current) != self.inode,
            _ => true,
        }
    }
}

impl Watch {
    fn new(wanted: Vec<PathBuf>) -> Self {
        let wanted: Vec<PathBuf> = wanted.into_iter().map(absolute).collect();
        let (events_tx, events) = mpsc::channel(EVENTS);
        let mut watch = Self {
            allowed: wanted.clone(),
            arms: wanted
                .into_iter()
                .map(|wanted| Arm {
                    wanted,
                    dir: None,
                    inode: None,
                })
                .collect(),
            watcher: None,
            events,
            pending: None,
        };

        match notify::recommended_watcher(forward(events_tx)) {
            Ok(watcher) => {
                watch.watcher = Some(watcher);
                watch.pending = watch.rearm().err().map(Update::Unavailable);
            }
            Err(error) => watch.pending = Some(Update::Unavailable(error.to_string())),
        }
        watch
    }

    async fn next(&mut self) -> Option<Update> {
        if let Some(update) = self.pending.take() {
            return Some(update);
        }

        loop {
            // Coalesce until the directory goes quiet, rather than flushing on a fixed window: a
            // rewrite in progress is read once it has finished, not partway through.
            let mut batch = vec![self.events.recv().await?];
            while let Ok(more) = tokio::time::timeout(DEBOUNCE, self.events.recv()).await {
                match more {
                    Some(more) => batch.push(more),
                    None => break,
                }
            }

            for event in batch.iter().filter(|event| event.kind.is_remove()) {
                for arm in &mut self.arms {
                    if arm
                        .dir
                        .as_ref()
                        .is_some_and(|dir| event.paths.contains(dir))
                    {
                        arm.dir = None;
                        arm.inode = None;
                    }
                }
            }

            if self.arms.iter().any(|arm| arm.displaced(&self.allowed)) {
                return Some(match self.rearm() {
                    Ok(()) => Update::Rearmed,
                    Err(reason) => Update::Unavailable(reason),
                });
            }

            let touched: Vec<PathBuf> = batch
                .into_iter()
                .flat_map(|event| event.paths)
                .filter(|path| {
                    self.arms
                        .iter()
                        .any(|arm| path.parent() == Some(arm.wanted.as_path()))
                })
                .collect();
            if !touched.is_empty() {
                return Some(Update::Changed(touched));
            }
        }
    }

    /// Fails only when the whole set is unwatchable. One directory the kernel refused is a warning
    /// and the rest keep working, because reporting the set dead would throw away watches that are
    /// still delivering.
    fn rearm(&mut self) -> Result<(), String> {
        let Self {
            arms,
            allowed,
            watcher,
            ..
        } = self;
        let Some(watcher) = watcher.as_mut() else {
            return Err("no watcher".to_owned());
        };
        let previous: Vec<PathBuf> = arms.iter().filter_map(|arm| arm.dir.clone()).collect();
        let mut failure = None;

        for arm in &mut *arms {
            if !arm.displaced(allowed) {
                continue;
            }
            let Some(dir) = nearest_existing(&arm.wanted, allowed) else {
                failure = failure
                    .or_else(|| Some(format!("nothing watchable above {}", arm.wanted.display())));
                arm.dir = None;
                arm.inode = None;
                continue;
            };

            // The new watch is placed before the old one is released below, so a kernel that
            // refuses it leaves the working watch in place instead of costing both.
            match watcher.watch(&dir, RecursiveMode::NonRecursive) {
                Ok(()) => {
                    arm.inode = inode(&dir);
                    arm.dir = Some(dir);
                }
                Err(error) => {
                    tracing::warn!(dir = %dir.display(), %error, "could not watch");
                    failure = failure.or_else(|| Some(error.to_string()));
                }
            }
        }

        // Release only what nothing is armed on any more. Two directories missing from the same
        // parent share one watch, and `unwatch` addresses by path, so releasing one by name would
        // take the other's watch with it.
        for dir in previous {
            if !arms.iter().any(|arm| arm.dir.as_ref() == Some(&dir)) {
                let _ = watcher.unwatch(&dir);
            }
        }

        match arms.iter().all(|arm| arm.dir.is_none()) {
            true => Err(failure.unwrap_or_else(|| "nothing watchable".to_owned())),
            false => Ok(()),
        }
    }
}

/// Access events fire for every read in a watched directory, this process's own included, and say
/// nothing about what changed. Dropping them is what keeps the directory holding `config.toml`
/// quiet while anything else reads anything in it.
fn forward(events: mpsc::Sender<Event>) -> impl FnMut(notify::Result<Event>) + Send + 'static {
    move |result| {
        let Ok(event) = result else { return };
        if !matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        ) {
            return;
        }

        // Dropping one when the queue is full loses nothing: every event makes the consumer re-read
        // the files as they are now, and a full queue means one is already waiting to.
        if !event.paths.is_empty() {
            let _ = events.try_send(event);
        }
    }
}

fn nearest_existing(dir: &Path, allowed: &[PathBuf]) -> Option<PathBuf> {
    let mut candidate = dir;

    loop {
        if candidate.is_dir() {
            return Some(candidate.to_path_buf());
        }
        candidate = candidate.parent()?;
        if !allowed.iter().any(|floor| floor == candidate) {
            return None;
        }
    }
}

fn absolute(dir: PathBuf) -> PathBuf {
    std::path::absolute(&dir).unwrap_or(dir)
}

fn inode(dir: &Path) -> Option<u64> {
    std::fs::metadata(dir).ok().map(|metadata| metadata.ino())
}

#[cfg(test)]
mod tests;
