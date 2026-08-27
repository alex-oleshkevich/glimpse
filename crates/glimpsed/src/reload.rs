use std::path::PathBuf;

use futures_util::StreamExt;
use glimpse_config::{Config, Update, watch_all, watch_dirs};
use tokio::signal::unix::Signal;
use tokio_util::sync::CancellationToken;

/// Hands one service its own slice of a freshly loaded document, and only when that slice moved.
/// Built where the concrete service type is still known, which is the one place `S::Config`'s
/// `From` and its `PartialEq` can be reached.
pub type ConfigSink = Box<dyn FnMut(&Config) + Send>;

/// Watching and reloading, run as one task of its own. `SIGHUP` and the filesystem are two
/// triggers for the same work, and neither replaces the other: a user editing with a tool that
/// defeats inotify still has a way to apply the change.
pub struct Reloader {
    path: Option<PathBuf>,
    config: Config,
    sinks: Vec<ConfigSink>,
}

impl Reloader {
    pub fn new(path: Option<PathBuf>, config: Config, sinks: Vec<ConfigSink>) -> Self {
        Self {
            path,
            config,
            sinks,
        }
    }

    pub async fn run(mut self, mut hangup: Signal, cancel: CancellationToken) {
        let mut updates = Box::pin(watch_all(watch_dirs(self.path.as_deref())));

        loop {
            tokio::select! {
                () = cancel.cancelled() => break,
                _ = hangup.recv() => self.reload().await,
                Some(update) = updates.next() => match update {
                    Update::Changed(_) | Update::Rearmed => self.reload().await,
                    Update::Unavailable(reason) => tracing::warn!(
                        reason,
                        "the configuration is not being watched; SIGHUP still reloads it"
                    ),
                },
            }
        }
    }

    async fn reload(&mut self) {
        let Some(config) = glimpse_config::reread(self.path.as_deref(), &self.config).await else {
            return;
        };

        tracing::info!("configuration reloaded");
        self.config = config;
        for sink in &mut self.sinks {
            sink(&self.config);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use super::*;

    /// Records the temperature each reload handed it, which is enough to tell "was not called"
    /// from "was called with the same value".
    fn recorder() -> (ConfigSink, Arc<Mutex<Vec<u32>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorded = seen.clone();
        let sink: ConfigSink = Box::new(move |config: &Config| {
            if let Ok(mut seen) = recorded.lock() {
                seen.push(config.night_light.temperature);
            }
        });
        (sink, seen)
    }

    fn write(path: &Path, body: &str) {
        std::fs::write(path, body).expect("writes");
    }

    fn reloader(path: &Path, sink: ConfigSink) -> Reloader {
        let config = glimpse_config::load(Some(path)).expect("loads");
        Reloader::new(Some(path.to_path_buf()), config, vec![sink])
    }

    #[tokio::test]
    async fn a_document_that_did_not_move_reaches_no_sink() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("config.toml");
        write(&path, "[night_light]\ntemperature = 4200\n");

        let (sink, seen) = recorder();
        let mut reloader = reloader(&path, sink);

        write(&path, "[night_light]\ntemperature = 4200\n");
        reloader.reload().await;

        assert!(
            seen.lock().expect("not poisoned").is_empty(),
            "an identical document must not be handed to anyone"
        );
    }

    #[tokio::test]
    async fn a_changed_document_reaches_every_sink_once() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("config.toml");
        write(&path, "[night_light]\ntemperature = 4200\n");

        let (sink, seen) = recorder();
        let mut reloader = reloader(&path, sink);

        write(&path, "[night_light]\ntemperature = 3000\n");
        reloader.reload().await;
        reloader.reload().await;

        assert_eq!(
            *seen.lock().expect("not poisoned"),
            [3000],
            "the second reload found nothing new"
        );
    }

    #[tokio::test]
    async fn a_document_that_does_not_parse_is_dropped_and_the_running_one_survives() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("config.toml");
        write(&path, "[night_light]\ntemperature = 4200\n");

        let (sink, seen) = recorder();
        let mut reloader = reloader(&path, sink);

        write(&path, "not toml [[[");
        reloader.reload().await;

        assert!(seen.lock().expect("not poisoned").is_empty());
        assert_eq!(
            reloader.config.night_light.temperature, 4200,
            "the broken read must not have displaced the running configuration"
        );

        write(&path, "[night_light]\ntemperature = 3000\n");
        reloader.reload().await;
        assert_eq!(*seen.lock().expect("not poisoned"), [3000]);
    }
}
