use std::path::PathBuf;

use futures_util::StreamExt;
use glimpse_config::{Config, watch_config};
use tokio_util::sync::CancellationToken;

/// Hands one service its own slice of a freshly loaded document, and only when that slice moved.
/// Built where the concrete service type is still known, which is the one place `S::Config`'s
/// `From` and its `PartialEq` can be reached.
pub type ConfigSink = Box<dyn FnMut(&Config) + Send>;

/// Fans a reloaded document out to every service, run as one task of its own. The triggers —
/// `SIGHUP` and the filesystem — belong to `watch_config`, which every binary reloads through.
pub async fn run(
    path: Option<PathBuf>,
    config: Config,
    mut sinks: Vec<ConfigSink>,
    cancel: CancellationToken,
) {
    let mut configs = Box::pin(watch_config(path, config));

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            config = configs.next() => {
                let Some(config) = config else {
                    tracing::error!("the configuration is no longer being watched");
                    break;
                };
                tracing::info!("configuration reloaded");
                for sink in &mut sinks {
                    sink(&config);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::*;

    /// Generous, because it covers an inotify round trip plus the watch's debounce window; a
    /// correct implementation answers in well under it and a broken one waits the whole time.
    const SETTLE: Duration = Duration::from_secs(5);

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

    /// The equality gate and the drop of a document that does not parse are `watch_config`'s and
    /// are tested there. What is this task's alone is that a document which did move reaches every
    /// sink it holds.
    #[tokio::test]
    async fn a_changed_document_reaches_every_sink() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[night_light]\ntemperature = 4200\n").expect("writes");

        let (first, seen_first) = recorder();
        let (second, seen_second) = recorder();
        let config = glimpse_config::load(Some(&path)).expect("loads");
        let cancel = CancellationToken::new();
        let running = tokio::spawn(run(
            Some(path.clone()),
            config,
            vec![first, second],
            cancel.clone(),
        ));

        // The task arms its watch on its first poll, and a write before that is a write inotify
        // never saw.
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        std::fs::write(&path, "[night_light]\ntemperature = 3000\n").expect("changes");
        tokio::time::timeout(SETTLE, async {
            while seen_first.lock().expect("not poisoned").is_empty() {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("the changed document within the settle window");

        cancel.cancel();
        running.await.expect("the task joins");

        assert_eq!(*seen_first.lock().expect("not poisoned"), [3000]);
        assert_eq!(
            *seen_second.lock().expect("not poisoned"),
            [3000],
            "a document must reach every sink, not just the first"
        );
    }
}
