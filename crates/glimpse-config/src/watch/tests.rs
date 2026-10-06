use std::sync::{Arc, Mutex};

use super::*;

/// Generous, because it covers an inotify round trip plus the debounce window; a correct
/// implementation answers in well under it and a broken one waits the whole time either way.
const SETTLE: Duration = Duration::from_secs(10);

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

#[tokio::test]
async fn a_directory_named_relatively_reports_the_same_as_one_named_absolutely() {
    let dir = tempfile::TempDir::new_in(".").expect("a temporary directory below this one");
    let relative = PathBuf::from(dir.path().file_name().expect("a name"));
    let mut updates: Updates = Box::pin(watch(relative.clone()));

    std::fs::write(relative.join("config.toml"), "").expect("writes");

    assert!(
        next_changed(&mut updates)
            .await
            .iter()
            .any(|path| path.ends_with("config.toml"))
    );
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
    let original = "[night-light]\ntemperature = 4200\n";
    std::fs::write(&path, original).expect("writes");

    let current = load(Some(&path)).expect("loads");
    let mut configs = Box::pin(watch_config(Some(path.clone()), current));

    std::fs::write(&path, original).expect("rewrites");
    tokio::time::sleep(DEBOUNCE * 2).await;
    std::fs::write(&path, "[night-light]\ntemperature = 3000\n").expect("changes");

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
    std::fs::write(&path, "[night-light]\ntemperature = 4200\n").expect("writes");

    let current = load(Some(&path)).expect("loads");
    std::fs::write(&path, "[night-light]\ntemperature = 3000\n").expect("changes");

    // Registers the handler. Raising SIGHUP before this point kills the test binary.
    let mut configs = Box::pin(watch_config(Some(path.clone()), current));
    hangup().await;

    let config = tokio::time::timeout(SETTLE, configs.next())
        .await
        .expect("a configuration within the settle window")
        .expect("the stream is still alive");
    assert_eq!(config.night_light.temperature, 3000);
}

/// Captures what the crate logs on this thread. `set_default` is thread-local and the test
/// runtime is current-thread, so everything awaited below reports into this buffer.
#[derive(Clone, Default)]
struct Logged(Arc<Mutex<Vec<u8>>>);

impl Logged {
    fn lines(&self) -> String {
        let bytes = self.0.lock().expect("not poisoned").clone();
        String::from_utf8(bytes).expect("tracing writes utf-8")
    }

    fn capture(&self) -> tracing::subscriber::DefaultGuard {
        let sink = self.clone();
        let guard = tracing::subscriber::set_default(
            tracing_subscriber::fmt()
                .with_writer(move || sink.clone())
                .with_max_level(tracing::Level::ERROR)
                .without_time()
                .finish(),
        );
        tracing::callsite::rebuild_interest_cache();
        guard
    }
}

impl std::io::Write for Logged {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("not poisoned").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

const START: &str = "[night-light]\ntemperature = 4200\n";
const BROKEN: &str = "not toml [[[";
const MISTYPED: &str = "[night-light]\ntemperature = \"warm\"\n";

fn good(temperature: u32) -> String {
    format!("[night-light]\ntemperature = {temperature}\n")
}

/// Drives `watch_config` through a sequence of writes, polling the stream throughout — it is
/// lazy, so the writes and the polling have to run together — and answers with the documents it
/// yielded and everything it logged at `ERROR`. `wanted` is how many documents the sequence
/// ends up producing; the collector stops there rather than waiting out the settle window.
async fn replay(writes: &[(&str, &str)], wanted: usize) -> (Vec<u32>, String) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, START).expect("writes");

    let current = load(Some(&path)).expect("loads");
    let logged = Logged::default();
    let _capture = logged.capture();
    let mut configs = Box::pin(watch_config(Some(path), current));

    let churn = async {
        tokio::time::sleep(DEBOUNCE * 2).await;
        for (name, text) in writes {
            std::fs::write(dir.path().join(name), text).expect("writes");
            tokio::time::sleep(DEBOUNCE * 4).await;
        }
    };
    let collect = async {
        let mut seen = Vec::new();
        while seen.len() < wanted {
            match tokio::time::timeout(SETTLE, configs.next()).await {
                Ok(Some(config)) => seen.push(config.night_light.temperature),
                _ => break,
            }
        }
        seen
    };

    let (seen, ()) = futures_util::future::join(collect, churn).await;
    (seen, logged.lines())
}

fn complaints(lines: &str) -> usize {
    lines.matches("dropped a reload").count()
}

/// One broken document and a directory that keeps producing events is one problem, not four.
/// Editors drop swap files right beside `config.toml`, and before the failure was gated the way
/// a success already was, every one of them reproduced the whole error line.
#[tokio::test]
async fn a_failure_that_has_not_changed_is_reported_once() {
    let (seen, lines) = replay(
        &[
            ("config.toml", BROKEN),
            (".config.toml.sw0", "x"),
            (".config.toml.sw1", "x"),
            (".config.toml.sw2", "x"),
            ("config.toml", &good(3000)),
        ],
        1,
    )
    .await;

    assert_eq!(
        seen,
        [3000],
        "the good write still arrives after the broken one"
    );
    assert_eq!(complaints(&lines), 1, "one broken document, got:\n{lines}");
}

/// The gate is keyed on the message, so it must not swallow a different way of being broken.
#[tokio::test]
async fn a_failure_that_changed_is_reported_again() {
    let (_, lines) = replay(
        &[
            ("config.toml", BROKEN),
            ("config.toml", MISTYPED),
            ("config.toml", &good(3000)),
        ],
        1,
    )
    .await;

    assert_eq!(
        complaints(&lines),
        2,
        "a parse failure and a type failure are two problems, got:\n{lines}"
    );
}

/// The gate has to reopen on a good load, or a document broken, fixed, and broken again the
/// same way reports the first break and stays quiet for the life of the process.
#[tokio::test]
async fn the_same_failure_after_a_good_load_is_reported_again() {
    let (seen, lines) = replay(
        &[
            ("config.toml", BROKEN),
            ("config.toml", &good(3000)),
            ("config.toml", BROKEN),
            ("config.toml", &good(2000)),
        ],
        2,
    )
    .await;

    assert_eq!(seen, [3000, 2000], "both good documents arrived");
    assert_eq!(
        complaints(&lines),
        2,
        "the second break is news, got:\n{lines}"
    );
}

/// Undoing the edit that broke it is a good load that yields nothing, because the document is
/// back to what is already running. It still has to reopen the gate — the fix is that a
/// document *loads*, not that it moved.
#[tokio::test]
async fn a_good_load_reopens_the_gate_even_when_the_document_did_not_move() {
    let (seen, lines) = replay(
        &[
            ("config.toml", BROKEN),
            ("config.toml", START),
            ("config.toml", BROKEN),
            ("config.toml", &good(3000)),
        ],
        1,
    )
    .await;

    assert_eq!(
        seen,
        [3000],
        "the identical rewrite yielded nothing of its own"
    );
    assert_eq!(
        complaints(&lines),
        2,
        "the break after the undo is news, got:\n{lines}"
    );
}

#[tokio::test]
async fn watch_config_keeps_the_running_document_when_a_reload_does_not_parse() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[night-light]\ntemperature = 4200\n").expect("writes");

    let current = load(Some(&path)).expect("loads");
    let mut configs = Box::pin(watch_config(Some(path.clone()), current));

    std::fs::write(&path, "not toml [[[").expect("breaks it");
    tokio::time::sleep(DEBOUNCE * 2).await;
    std::fs::write(&path, "[night-light]\ntemperature = 3000\n").expect("fixes it");

    let config = tokio::time::timeout(SETTLE, configs.next())
        .await
        .expect("a configuration within the settle window")
        .expect("the stream is still alive");
    assert_eq!(
        config.night_light.temperature, 3000,
        "the broken write is dropped, and the next good one still arrives"
    );
}

#[tokio::test]
async fn a_broken_document_undone_to_the_running_one_reports_the_recovery() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    let good = "[night-light]\ntemperature = 4200\n";
    std::fs::write(&path, good).expect("writes");

    let current = load(Some(&path)).expect("loads");
    let mut reloads = Box::pin(watch_config_reporting(Some(path.clone()), current, None));

    std::fs::write(&path, "not toml [[[").expect("breaks it");
    let broken = tokio::time::timeout(SETTLE, reloads.next())
        .await
        .expect("a report within the settle window")
        .expect("the stream is still alive");
    assert!(broken.is_err(), "the break is reported");

    std::fs::write(&path, good).expect("undoes it");
    let recovered = tokio::time::timeout(SETTLE, reloads.next())
        .await
        .expect("a report within the settle window")
        .expect("the stream is still alive");
    assert_eq!(
        recovered,
        Ok(None),
        "the running document is unchanged, and the break is over"
    );
}
