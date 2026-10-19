use std::sync::{Arc, Mutex};

use super::*;

const SETTLE: Duration = Duration::from_secs(10);

type Updates = Pin<Box<dyn Stream<Item = Update> + Send>>;

async fn next(updates: &mut Updates) -> Update {
    tokio::time::timeout(SETTLE, updates.next())
        .await
        .expect("an update within the settle window")
        .expect("the stream is still alive")
}

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

#[tokio::test]
async fn a_missing_directory_is_never_watched_through_a_parent_outside_the_set() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let mut updates = watching(&root.path().join("glimpse"));

    assert!(
        matches!(next(&mut updates).await, Update::Unavailable(_)),
        "the parent is not in the set, so there is nothing to fall back to"
    );
}

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
async fn directories_sharing_a_watch_survive_each_other_descending() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let base = root.path().join("glimpse");
    let dropins = base.join("config.d");
    std::fs::create_dir(&base).expect("creates");
    let mut updates: Updates = Box::pin(watch_all([base.clone(), dropins.clone()]));

    std::fs::write(base.join("config.toml"), "").expect("writes");
    let paths = next_changed(&mut updates).await;
    assert!(paths.iter().any(|path| path.ends_with("config.toml")));

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
async fn a_reread_skips_an_identical_rewrite_and_yields_a_changed_document() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    let original = "[night-light]\ntemperature = 4200\n";
    std::fs::write(&path, original).expect("writes");
    let current = load(Some(&path)).expect("loads");

    std::fs::write(&path, original).expect("rewrites");
    assert!(
        matches!(reread(Some(&path), &current).await, Ok(None)),
        "an identical rewrite must yield nothing"
    );

    std::fs::write(&path, "[night-light]\ntemperature = 3000\n").expect("changes");
    let changed = reread(Some(&path), &current).await;
    assert!(
        matches!(&changed, Ok(Some(config)) if config.night_light.temperature == 3000),
        "{changed:?}"
    );
}

#[tokio::test]
async fn sighup_rereads_a_change_no_filesystem_event_announced() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[night-light]\ntemperature = 4200\n").expect("writes");

    let current = load(Some(&path)).expect("loads");
    std::fs::write(&path, "[night-light]\ntemperature = 3000\n").expect("changes");

    let mut configs = Box::pin(watch_config(Some(path.clone()), current));
    hangup().await;

    let config = tokio::time::timeout(SETTLE, configs.next())
        .await
        .expect("a configuration within the settle window")
        .expect("the stream is still alive");
    assert_eq!(config.night_light.temperature, 3000);
}

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
    let mut reloads = Box::pin(watch_config_reporting(Some(path.clone()), current, None));

    std::fs::write(&path, "not toml [[[").expect("breaks it");
    let broken = tokio::time::timeout(SETTLE, reloads.next())
        .await
        .expect("a report within the settle window")
        .expect("the stream is still alive");
    assert!(broken.is_err(), "the broken write is dropped");

    std::fs::write(&path, "[night-light]\ntemperature = 3000\n").expect("fixes it");
    let fixed = tokio::time::timeout(SETTLE, reloads.next())
        .await
        .expect("a report within the settle window")
        .expect("the stream is still alive");
    assert!(
        matches!(&fixed, Ok(Some(config)) if config.night_light.temperature == 3000),
        "the next good write still arrives, got {fixed:?}"
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

#[tokio::test]
async fn repointing_a_config_symlink_watches_edits_in_the_new_target_directory() {
    let root = tempfile::tempdir().expect("temporary directory");
    let old = root.path().join("old");
    let new = root.path().join("new");
    std::fs::create_dir(&old).expect("creates");
    std::fs::create_dir(&new).expect("creates");
    let old_target = old.join("config.toml");
    let new_target = new.join("config.toml");
    std::fs::write(&old_target, good(4200)).expect("writes");
    std::fs::write(&new_target, good(3000)).expect("writes");
    let path = root.path().join("config.toml");
    std::os::unix::fs::symlink(&old_target, &path).expect("links");
    let current = load(Some(&path)).expect("loads");
    let mut configs = Box::pin(watch_config(Some(path.clone()), current));
    let replacement = root.path().join("replacement");
    std::os::unix::fs::symlink(&new_target, &replacement).expect("links");
    std::fs::rename(&replacement, &path).expect("repoints");
    let moved = tokio::time::timeout(SETTLE, configs.next())
        .await
        .expect("reloads")
        .expect("alive");
    assert_eq!(moved.night_light.temperature, 3000);
    std::fs::write(&new_target, good(2500)).expect("edits the new target");
    let edited = tokio::time::timeout(SETTLE, configs.next())
        .await
        .expect("reloads the new target")
        .expect("alive");
    assert_eq!(edited.night_light.temperature, 2500);
}
