use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use futures_util::{Stream, StreamExt, stream};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;

use crate::load::{SCREENSHOT_FILE_NAME, load, watch_dirs};
use crate::schema::Config;

/// How long a watched directory has to go quiet before its events are reported. An editor saving a
/// file produces a write, a rename over the target, and sometimes a delete and a create.
const DEBOUNCE: Duration = Duration::from_millis(250);
const EVENTS: usize = 256;

#[derive(Debug, PartialEq, Eq)]
pub enum Update {
    Changed(Vec<PathBuf>),
    /// The watch moved. Whatever happened while it was elsewhere produced no events and cannot be
    /// inferred, so the only correct response is to read everything again.
    Rearmed,
    Unavailable(String),
}

/// Watches one directory, non-recursively, for as long as the stream lives.
pub fn watch(dir: impl Into<PathBuf>) -> impl Stream<Item = Update> + Send + 'static {
    watch_all([dir.into()])
}

/// One inotify instance for the whole set, however many directories it holds. Its reader blocks on
/// the descriptor, so a session where nothing is edited costs nothing at all.
pub fn watch_all(
    dirs: impl IntoIterator<Item = PathBuf>,
) -> impl Stream<Item = Update> + Send + 'static {
    stream::unfold(
        Watch::new(dirs.into_iter().collect()),
        |mut watch| async move { watch.next().await.map(|update| (update, watch)) },
    )
}

pub fn watch_config(
    config_path: Option<PathBuf>,
    current: Config,
) -> impl Stream<Item = Config> + Send + 'static {
    watch_config_reporting(config_path, current, None)
        .filter_map(|reloaded| std::future::ready(reloaded.ok().flatten()))
}

pub fn watch_config_reporting(
    config_path: Option<PathBuf>,
    current: Config,
    failure: Option<String>,
) -> impl Stream<Item = Result<Option<Config>, String>> + Send + 'static {
    let watched = watch_all(watch_dirs(config_path.as_deref())).filter_map(|update| async move {
        match update {
            Update::Changed(paths)
                if !paths.is_empty()
                    && paths
                        .iter()
                        .all(|path| path.file_name() == Some(SCREENSHOT_FILE_NAME.as_ref())) =>
            {
                None
            }
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

    let reader = Reader {
        triggers: Box::pin(stream::select(Box::pin(watched), Box::pin(hangups()))),
        path: config_path,
        current,
        failure,
    };

    stream::unfold(reader, |mut reader| async move {
        reader.next().await.map(|config| (config, reader))
    })
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

/// Loads the stack off the runtime threads: the new document if it parsed and differs, `None` if it
/// parsed and did not, and the reason if it did not parse. The failure is returned rather than
/// logged here, because the same one repeated is a document nobody has fixed yet and only the
/// caller knows whether it has already said so.
async fn reread(config_path: Option<&Path>, current: &Config) -> Result<Option<Config>, String> {
    let path = config_path.map(Path::to_path_buf);

    match tokio::task::spawn_blocking(move || load(path.as_deref())).await {
        Ok(Ok(config)) => Ok((config != *current).then_some(config)),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(format!("the configuration reader panicked: {error}")),
    }
}

struct Reader {
    triggers: Pin<Box<dyn Stream<Item = ()> + Send>>,
    path: Option<PathBuf>,
    current: Config,
    /// The last failure reported. A watched directory goes on producing events while a document
    /// stays broken — an editor's swap and backup files land right beside it — and without this
    /// every one of them reproduces the same line.
    failure: Option<String>,
}

impl Reader {
    async fn next(&mut self) -> Option<Result<Option<Config>, String>> {
        while self.triggers.next().await.is_some() {
            match reread(self.path.as_deref(), &self.current).await {
                // A document that loads is one that is no longer broken, whether or not it moved,
                // so the next failure is news again.
                Ok(loaded) => {
                    let recovered = self.failure.take().is_some();
                    match loaded {
                        Some(config) => {
                            self.current = config.clone();
                            return Some(Ok(Some(config)));
                        }
                        None if recovered => return Some(Ok(None)),
                        None => {}
                    }
                }
                Err(reason) => {
                    if self.report(&reason) {
                        return Some(Err(reason));
                    }
                }
            }
        }
        None
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
    events: mpsc::Receiver<Vec<PathBuf>>,
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
    /// to a directory where nothing happens. Comparing the inode is what tells those two apart.
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
            let mut paths = self.events.recv().await?;
            while let Ok(more) = tokio::time::timeout(DEBOUNCE, self.events.recv()).await {
                match more {
                    Some(more) => paths.extend(more),
                    None => break,
                }
            }

            if self.arms.iter().any(|arm| arm.displaced(&self.allowed)) {
                return Some(match self.rearm() {
                    Ok(()) => Update::Rearmed,
                    Err(reason) => Update::Unavailable(reason),
                });
            }

            let touched: Vec<PathBuf> = paths
                .into_iter()
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
fn forward(
    events: mpsc::Sender<Vec<PathBuf>>,
) -> impl FnMut(notify::Result<Event>) + Send + 'static {
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
            let _ = events.try_send(event.paths);
        }
    }
}

/// The deepest existing directory at or above `dir`, never leaving `allowed`.
///
/// The walk stops at the set it was given rather than at a list of directories to avoid, so a
/// missing `config.d/` still falls back onto the `glimpse/` beside it in the set, and a missing
/// `glimpse/` falls back onto nothing. `/etc` and `$XDG_CONFIG_HOME` are written constantly by
/// software with no connection to this session, and a watch on either wakes us for every one of
/// those writes to report a file that did not change. What is given up is noticing the
/// configuration directory being created while the session runs, which is the one time a restart
/// is no imposition — there was nothing configured to reload.
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
