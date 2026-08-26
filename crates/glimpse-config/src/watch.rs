use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use futures_util::{Stream, StreamExt, stream};
use notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use tokio::sync::mpsc;

use crate::load::{load, watch_dirs};
use crate::schema::Config;

const DEBOUNCE: Duration = Duration::from_millis(250);
const BATCHES: usize = 64;

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
    stream::unfold(Watch::new(dir.into()), |mut watch| async move {
        watch.next().await.map(|update| (update, watch))
    })
}

pub fn watch_config(
    config_path: Option<PathBuf>,
    current: Config,
) -> impl Stream<Item = Config> + Send + 'static {
    let updates = stream::select_all(
        watch_dirs(config_path.as_deref())
            .into_iter()
            .map(|dir| Box::pin(watch(dir))),
    );
    let reader = Reader {
        updates: Box::pin(updates),
        path: config_path,
        current,
    };

    stream::unfold(reader, |mut reader| async move {
        reader.next().await.map(|config| (config, reader))
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
    updates: Pin<Box<dyn Stream<Item = Update> + Send>>,
    path: Option<PathBuf>,
    current: Config,
}

impl Reader {
    async fn next(&mut self) -> Option<Config> {
        while let Some(update) = self.updates.next().await {
            if let Update::Unavailable(reason) = update {
                tracing::warn!(reason, "the configuration is not being watched");
                continue;
            }

            if let Some(config) = reread(self.path.as_deref(), &self.current).await {
                self.current = config.clone();
                return Some(config);
            }
        }
        None
    }
}

struct Watch {
    wanted: PathBuf,
    armed: Option<Arm>,
    debouncer: Option<Debouncer<RecommendedWatcher, RecommendedCache>>,
    batches: mpsc::Receiver<Vec<PathBuf>>,
    pending: Option<Update>,
}

struct Arm {
    dir: PathBuf,
    inode: Option<u64>,
}

impl Watch {
    fn new(wanted: PathBuf) -> Self {
        let (batches_tx, batches) = mpsc::channel(BATCHES);
        let mut watch = Self {
            wanted,
            armed: None,
            debouncer: None,
            batches,
            pending: None,
        };

        match new_debouncer(DEBOUNCE, None, forward(batches_tx)) {
            Ok(debouncer) => {
                watch.debouncer = Some(debouncer);
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
            let paths = self.batches.recv().await?;

            if self.displaced() {
                return Some(match self.rearm() {
                    Ok(()) => Update::Rearmed,
                    Err(reason) => Update::Unavailable(reason),
                });
            }

            let touched: Vec<PathBuf> = paths
                .into_iter()
                .filter(|path| path.parent() == Some(self.wanted.as_path()))
                .collect();
            if !touched.is_empty() {
                return Some(Update::Changed(touched));
            }
        }
    }

    /// A watch is bound to an inode, not to a name, so `rm -rf` followed by a fresh clone leaves
    /// one armed on a directory nobody can reach: it reports nothing, forever, and looks identical
    /// to a directory where nothing happens. Comparing the inode is what tells those two apart.
    fn displaced(&self) -> bool {
        let Some(armed) = self.armed.as_ref() else {
            return true;
        };
        match nearest_existing(&self.wanted) {
            Some(dir) => dir != armed.dir || inode(&dir) != armed.inode,
            None => true,
        }
    }

    fn rearm(&mut self) -> Result<(), String> {
        let Some(debouncer) = self.debouncer.as_mut() else {
            return Err("no watcher".to_owned());
        };
        let dir = nearest_existing(&self.wanted)
            .ok_or_else(|| format!("nothing watchable above {}", self.wanted.display()))?;

        // The new watch is placed before the old one is dropped, so a kernel that refuses it
        // leaves the working watch in place instead of taking both. Descending from `/etc` to
        // `/etc/glimpse` otherwise loses `/etc` too, and nothing is ever noticed again.
        debouncer
            .watch(&dir, RecursiveMode::NonRecursive)
            .map_err(|error| error.to_string())?;

        // A replaced directory keeps its path, and `unwatch` addresses by path: dropping the old
        // watch by name here would drop the one just placed on the new inode.
        if let Some(previous) = self.armed.take()
            && previous.dir != dir
        {
            let _ = debouncer.unwatch(&previous.dir);
        }

        self.armed = Some(Arm {
            inode: inode(&dir),
            dir,
        });
        Ok(())
    }
}

/// Access events fire for every read in a watched directory, this process's own included, and say
/// nothing about what changed. Dropping them is what keeps the directory holding `config.toml`
/// quiet while anything else reads anything in it.
fn forward(
    batches: mpsc::Sender<Vec<PathBuf>>,
) -> impl FnMut(DebounceEventResult) + Send + 'static {
    move |result| {
        let Ok(batch) = result else { return };
        let paths: Vec<PathBuf> = batch
            .iter()
            .filter(|event| {
                matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                )
            })
            .flat_map(|event| event.paths.iter().cloned())
            .collect();

        // Dropping a batch when the queue is full loses nothing: every batch makes the consumer
        // re-read the files as they are now, and a full queue means one is already waiting to.
        if !paths.is_empty() {
            let _ = batches.try_send(paths);
        }
    }
}

/// `$HOME` and `/` are refused: both are noisy enough to cost more than the reload they would buy,
/// and every real case — an absent `config.d/`, an unconfigured machine's `glimpse/` — stops well
/// above either.
fn nearest_existing(dir: &Path) -> Option<PathBuf> {
    let home = dirs::home_dir();
    let mut candidate = dir;

    loop {
        if candidate.as_os_str().is_empty()
            || candidate == Path::new("/")
            || home.as_deref() == Some(candidate)
        {
            return None;
        }
        if candidate.is_dir() {
            return Some(candidate.to_path_buf());
        }
        candidate = candidate.parent()?;
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

    /// The ordinary first-run case: `config.d/` does not exist, so the watch sits on the parent
    /// until the directory shows up.
    #[tokio::test]
    async fn a_directory_that_appears_is_descended_into() {
        let root = tempfile::tempdir().expect("a temporary directory");
        let dropins = root.path().join("config.d");
        let mut updates = watching(&dropins);

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
