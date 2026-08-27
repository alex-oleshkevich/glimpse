use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use futures_util::{Stream, StreamExt, stream};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;

use crate::load::{load, watch_dirs};
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
    let watched = watch_all(watch_dirs(config_path.as_deref())).filter_map(|update| async move {
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

    let reader = Reader {
        triggers: Box::pin(stream::select(Box::pin(watched), Box::pin(hangups()))),
        path: config_path,
        current,
    };

    stream::unfold(reader, |mut reader| async move {
        reader.next().await.map(|config| (config, reader))
    })
}

fn hangups() -> impl Stream<Item = ()> + Send + 'static {
    let hangup = signal(SignalKind::hangup())
        .inspect_err(|error| tracing::warn!(%error, "SIGHUP will not reload the configuration"))
        .ok();

    stream::unfold(hangup, |hangup| async move {
        let mut hangup = hangup?;
        hangup.recv().await?;
        Some(((), Some(hangup)))
    })
}

/// The next document, or nothing: a read that failed, or one that came back equal, both leave the
/// caller running what it already has. Every consumer wants that same answer, so the logging and
/// the equality gate live here rather than once per binary.
pub async fn reread(config_path: Option<&Path>, current: &Config) -> Option<Config> {
    let path = config_path.map(Path::to_path_buf);

    match tokio::task::spawn_blocking(move || load(path.as_deref())).await {
        Ok(Ok(config)) => (config != *current).then_some(config),
        Ok(Err(error)) => {
            tracing::error!(%error, "dropped a reload, keeping the running configuration");
            None
        }
        Err(error) => {
            tracing::error!(%error, "the configuration reader panicked");
            None
        }
    }
}

struct Reader {
    triggers: Pin<Box<dyn Stream<Item = ()> + Send>>,
    path: Option<PathBuf>,
    current: Config,
}

impl Reader {
    async fn next(&mut self) -> Option<Config> {
        while self.triggers.next().await.is_some() {
            if let Some(config) = reread(self.path.as_deref(), &self.current).await {
                self.current = config.clone();
                return Some(config);
            }
        }
        None
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

fn inode(dir: &Path) -> Option<u64> {
    std::fs::metadata(dir).ok().map(|metadata| metadata.ino())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generous, because it covers an inotify round trip plus the debounce window; a correct
    /// implementation answers in well under it and a broken one waits the whole time either way.
    const SETTLE: Duration = Duration::from_secs(5);

    type Updates = Pin<Box<dyn Stream<Item = Update> + Send>>;

    async fn next(updates: &mut Updates) -> Update {
        tokio::time::timeout(SETTLE, updates.next())
            .await
            .expect("an update within the settle window")
            .expect("the stream is still alive")
    }

    /// A re-arm may arrive on its own before the change that followed it, because the debounce
    /// window does not have to contain both. Skipping them is what keeps these tests from
    /// depending on that timing.
    async fn next_changed(updates: &mut Updates) -> Vec<PathBuf> {
        loop {
            match next(updates).await {
                Update::Changed(paths) => return paths,
                Update::Rearmed => continue,
                other => panic!("expected Changed, got {other:?}"),
            }
        }
    }

    fn watching(dir: &Path) -> Updates {
        Box::pin(watch(dir.to_path_buf()))
    }

    async fn hangup() {
        let _installed = signal(SignalKind::hangup()).expect("registers a handler");
        let status = tokio::process::Command::new("kill")
            .args(["-HUP", &std::process::id().to_string()])
            .status()
            .await
            .expect("kill runs");
        assert!(status.success(), "kill did not signal this process");
    }

    #[tokio::test]
    async fn a_file_written_in_the_directory_is_reported() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let mut updates = watching(dir.path());

        std::fs::write(dir.path().join("config.toml"), "").expect("writes");

        match next(&mut updates).await {
            Update::Changed(paths) => {
                assert!(paths.iter().any(|path| path.ends_with("config.toml")));
            }
            other => panic!("expected Changed, got {other:?}"),
        }
    }

    /// The walk stops at the set it was given, so a directory that is missing and has no watched
    /// parent beside it in the set is not reachable from above. `$XDG_CONFIG_HOME` and `/etc` are
    /// written constantly by unrelated software; the configuration directory appearing is the one
    /// moment a restart costs nothing, because nothing was configured to reload.
    #[tokio::test]
    async fn a_missing_directory_is_never_watched_through_a_parent_outside_the_set() {
        let root = tempfile::tempdir().expect("a temporary directory");
        let mut updates = watching(&root.path().join("glimpse"));

        assert!(
            matches!(next(&mut updates).await, Update::Unavailable(_)),
            "the parent is not in the set, so there is nothing to fall back to"
        );
    }

    /// The ordinary case: the configuration directory exists and `config.d/` does not, so the
    /// drop-in arm falls back onto the directory beside it in the set.
    #[tokio::test]
    async fn a_dropin_directory_that_appears_is_descended_into() {
        let root = tempfile::tempdir().expect("a temporary directory");
        let base = root.path().join("glimpse");
        let dropins = base.join("config.d");
        std::fs::create_dir(&base).expect("creates");
        let mut updates: Updates = Box::pin(watch_all([base.clone(), dropins.clone()]));

        std::fs::create_dir(&dropins).expect("creates");
        assert_eq!(next(&mut updates).await, Update::Rearmed);

        std::fs::write(dropins.join("10-laptop.toml"), "").expect("writes");
        let paths = next_changed(&mut updates).await;
        assert!(paths.iter().any(|path| path.ends_with("10-laptop.toml")));
    }

    /// What a dotfile manager does. The directory has the same name and a different inode, so a
    /// watcher comparing names alone would report nothing ever again and look healthy doing it.
    #[tokio::test]
    async fn a_recreated_directory_is_watched_again() {
        let root = tempfile::tempdir().expect("a temporary directory");
        let dir = root.path().join("glimpse");
        std::fs::create_dir(&dir).expect("creates");
        let mut updates = watching(&dir);

        std::fs::remove_dir_all(&dir).expect("removes");
        std::fs::create_dir(&dir).expect("recreates");
        assert_eq!(next(&mut updates).await, Update::Rearmed);

        std::fs::write(dir.join("config.toml"), "").expect("writes");
        let paths = next_changed(&mut updates).await;
        assert!(paths.iter().any(|path| path.ends_with("config.toml")));
    }

    /// Both directories start missing and collapse onto the same ancestor, so they share one
    /// watch. As each appears the set descends — and releasing the ancestor by path must not take
    /// the watch the other arm is still using.
    #[tokio::test]
    async fn directories_sharing_a_watch_survive_each_other_descending() {
        let root = tempfile::tempdir().expect("a temporary directory");
        let base = root.path().join("glimpse");
        let dropins = base.join("config.d");
        std::fs::create_dir(&base).expect("creates");
        let mut updates: Updates = Box::pin(watch_all([base.clone(), dropins.clone()]));

        std::fs::write(base.join("config.toml"), "").expect("writes");
        let paths = next_changed(&mut updates).await;
        assert!(paths.iter().any(|path| path.ends_with("config.toml")));

        // The drop-in directory now descends off the base, which is still armed for the other arm.
        std::fs::create_dir(&dropins).expect("creates");
        assert_eq!(next(&mut updates).await, Update::Rearmed);

        std::fs::write(base.join("config.toml"), "changed").expect("writes");
        let paths = next_changed(&mut updates).await;
        assert!(
            paths.iter().any(|path| path.ends_with("config.toml")),
            "the base directory lost its watch when the drop-in directory descended"
        );

        std::fs::write(dropins.join("10-laptop.toml"), "").expect("writes");
        let paths = next_changed(&mut updates).await;
        assert!(paths.iter().any(|path| path.ends_with("10-laptop.toml")));
    }

    #[tokio::test]
    async fn watch_config_yields_a_changed_document_and_skips_an_identical_rewrite() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("config.toml");
        let original = "[night_light]\ntemperature = 4200\n";
        std::fs::write(&path, original).expect("writes");

        let current = load(Some(&path)).expect("loads");
        let mut configs = Box::pin(watch_config(Some(path.clone()), current));

        std::fs::write(&path, original).expect("rewrites");
        tokio::time::sleep(DEBOUNCE * 2).await;
        std::fs::write(&path, "[night_light]\ntemperature = 3000\n").expect("changes");

        let config = tokio::time::timeout(SETTLE, configs.next())
            .await
            .expect("a configuration within the settle window")
            .expect("the stream is still alive");
        assert_eq!(
            config.night_light.temperature, 3000,
            "the identical rewrite must not have been yielded first"
        );
    }

    /// The change lands before the watch is armed, so inotify never reports it and `SIGHUP` is the
    /// only thing that can produce a document here. That is the whole point of the second trigger:
    /// an editor whose write the watch missed still has a way to apply the change.
    #[tokio::test]
    async fn sighup_rereads_a_change_no_filesystem_event_announced() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[night_light]\ntemperature = 4200\n").expect("writes");

        let current = load(Some(&path)).expect("loads");
        std::fs::write(&path, "[night_light]\ntemperature = 3000\n").expect("changes");

        // Registers the handler. Raising SIGHUP before this point kills the test binary.
        let mut configs = Box::pin(watch_config(Some(path.clone()), current));
        hangup().await;

        let config = tokio::time::timeout(SETTLE, configs.next())
            .await
            .expect("a configuration within the settle window")
            .expect("the stream is still alive");
        assert_eq!(config.night_light.temperature, 3000);
    }

    #[tokio::test]
    async fn watch_config_keeps_the_running_document_when_a_reload_does_not_parse() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[night_light]\ntemperature = 4200\n").expect("writes");

        let current = load(Some(&path)).expect("loads");
        let mut configs = Box::pin(watch_config(Some(path.clone()), current));

        std::fs::write(&path, "not toml [[[").expect("breaks it");
        tokio::time::sleep(DEBOUNCE * 2).await;
        std::fs::write(&path, "[night_light]\ntemperature = 3000\n").expect("fixes it");

        let config = tokio::time::timeout(SETTLE, configs.next())
            .await
            .expect("a configuration within the settle window")
            .expect("the stream is still alive");
        assert_eq!(
            config.night_light.temperature, 3000,
            "the broken write is dropped, and the next good one still arrives"
        );
    }
}
