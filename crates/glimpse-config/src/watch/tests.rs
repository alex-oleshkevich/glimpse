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
    Box::pin(watch_dirs([dir.to_path_buf()]))
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
    let mut updates: Updates = Box::pin(watch_dirs([relative.clone()]));

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
    let dropins = base.join("panel.d");
    std::fs::create_dir(&base).expect("creates");
    let mut updates: Updates = Box::pin(watch_dirs([base.clone(), dropins.clone()]));

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
    let dropins = base.join("panel.d");
    std::fs::create_dir(&base).expect("creates");
    let mut updates: Updates = Box::pin(watch_dirs([base.clone(), dropins.clone()]));

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

type Reloads = Pin<Box<dyn Stream<Item = Reload<Config>> + Send>>;

fn hub_over(dirs: Vec<PathBuf>) -> Hub {
    Hub::start(move || dirs.clone())
}

fn reloading(hub: &Hub, path: &Path, current: Config, failure: Option<String>) -> Reloads {
    let receiver = hub.subscribe();
    let files = vec![path.to_path_buf()];
    Box::pin(reloads(
        move || receiver,
        Arc::new(move || crate::load_files(&files)),
        current,
        failure,
        false,
    ))
}

async fn reloaded(reloads: &mut Reloads) -> Reload<Config> {
    tokio::time::timeout(SETTLE, reloads.next())
        .await
        .expect("a reload within the settle window")
        .expect("the stream is still alive")
}

fn temperature(reload: Reload<Config>) -> u32 {
    match reload {
        Reload::Changed(config) => config.night_light.temperature,
        other => panic!("expected Changed, got {other:?}"),
    }
}

fn written(dir: &Path, text: &str) -> (PathBuf, Config) {
    let path = dir.join("config.toml");
    std::fs::write(&path, text).expect("writes");
    let current = crate::load_files(std::slice::from_ref(&path)).expect("loads");
    (path, current)
}

#[tokio::test]
async fn an_identical_rewrite_yields_nothing_and_a_changed_document_yields_changed() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let (path, current) = written(dir.path(), START);
    let hub = hub_over(vec![dir.path().to_path_buf()]);
    let mut reloads = reloading(&hub, &path, current, None);

    std::fs::write(&path, START).expect("rewrites");
    assert!(
        tokio::time::timeout(Duration::from_secs(2), reloads.next())
            .await
            .is_err(),
        "an identical rewrite must yield nothing"
    );

    std::fs::write(&path, good(3000)).expect("changes");
    assert_eq!(temperature(reloaded(&mut reloads).await), 3000);
}

#[tokio::test]
async fn sighup_rereads_a_change_no_filesystem_event_announced() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let (path, current) = written(dir.path(), START);
    let hub = hub_over(vec![dir.path().join("nowhere")]);
    let mut reloads = reloading(&hub, &path, current, None);
    std::fs::write(&path, good(3000)).expect("changes");

    hangup().await;

    assert_eq!(temperature(reloaded(&mut reloads).await), 3000);
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
                .with_max_level(tracing::Level::INFO)
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
    let (path, current) = written(dir.path(), START);
    let logged = Logged::default();
    let _capture = logged.capture();
    let hub = hub_over(vec![dir.path().to_path_buf()]);
    let mut reloads = reloading(&hub, &path, current, None);

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
            match tokio::time::timeout(SETTLE, reloads.next()).await {
                Ok(Some(Reload::Changed(config))) => seen.push(config.night_light.temperature),
                Ok(Some(_)) => {}
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
async fn a_fixed_document_logs_one_recovery() {
    let (_, lines) = replay(&[("config.toml", BROKEN), ("config.toml", &good(3000))], 1).await;

    assert_eq!(
        lines.matches("configuration recovered").count(),
        1,
        "{lines}"
    );
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
async fn a_broken_document_is_rejected_once_and_the_next_good_write_changes() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let (path, current) = written(dir.path(), START);
    let hub = hub_over(vec![dir.path().to_path_buf()]);
    let mut reloads = reloading(&hub, &path, current, None);

    std::fs::write(&path, BROKEN).expect("breaks it");
    assert!(matches!(reloaded(&mut reloads).await, Reload::Rejected(_)));

    tokio::time::sleep(DEBOUNCE * 3).await;
    std::fs::write(&path, BROKEN).expect("breaks it the same way");
    tokio::time::sleep(DEBOUNCE * 3).await;
    std::fs::write(&path, good(3000)).expect("fixes it");
    assert_eq!(temperature(reloaded(&mut reloads).await), 3000);
}

#[tokio::test]
async fn a_failure_the_caller_already_knows_is_not_rejected_again() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, BROKEN).expect("writes");
    let failure = crate::load_files(std::slice::from_ref(&path))
        .expect_err("broken")
        .to_string();
    let hub = hub_over(vec![dir.path().to_path_buf()]);
    let mut reloads = reloading(&hub, &path, Config::default(), Some(failure));

    std::fs::write(&path, BROKEN).expect("rewrites it broken");
    assert!(
        tokio::time::timeout(Duration::from_secs(2), reloads.next())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_broken_document_undone_to_the_running_one_reports_the_recovery() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let (path, current) = written(dir.path(), START);
    let hub = hub_over(vec![dir.path().to_path_buf()]);
    let mut reloads = reloading(&hub, &path, current, None);

    std::fs::write(&path, BROKEN).expect("breaks it");
    assert!(matches!(reloaded(&mut reloads).await, Reload::Rejected(_)));

    std::fs::write(&path, START).expect("undoes it");
    assert_eq!(
        reloaded(&mut reloads).await,
        Reload::Recovered,
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
    let home = root.path().join("home");
    std::fs::create_dir(&home).expect("creates");
    let path = home.join("config.toml");
    std::os::unix::fs::symlink(&old_target, &path).expect("links");
    let current = crate::load_files(std::slice::from_ref(&path)).expect("loads");
    let dirs = vec![home.clone()];
    let hub = Hub::start(move || {
        hub_dirs(
            &dirs,
            Path::new("/nonexistent"),
            Path::new("/nonexistent"),
            None,
        )
    });
    let mut reloads = reloading(&hub, &path, current, None);

    let replacement = home.join("replacement");
    std::os::unix::fs::symlink(&new_target, &replacement).expect("links");
    std::fs::rename(&replacement, &path).expect("repoints");
    assert_eq!(temperature(reloaded(&mut reloads).await), 3000);

    std::fs::write(&new_target, good(2500)).expect("edits the new target");
    assert_eq!(temperature(reloaded(&mut reloads).await), 2500);
}

#[test]
fn the_set_is_each_writable_root_with_its_drop_ins_and_theme_directories() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let user = base.path().join("user");
    let system = base.path().join("etc");
    std::fs::create_dir_all(user.join("themes/nord")).expect("creates");
    std::fs::create_dir_all(user.join("themes/dusk")).expect("creates");
    std::fs::write(user.join("themes/notes.txt"), "").expect("writes");
    let data = base.path().join("data");
    std::fs::create_dir_all(data.join("themes/adwaita")).expect("creates");

    assert_eq!(
        hub_dirs(&[user.clone(), system.clone()], &system, &data, None),
        [
            user.clone(),
            user.join("panel.d"),
            user.join("themes"),
            user.join("themes/dusk"),
            user.join("themes/nord"),
            data.join("themes"),
            data.join("themes/adwaita"),
        ],
        "the system root is read but never watched"
    );
}

#[test]
fn a_themes_directory_override_joins_the_set_with_its_subdirectories() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let themes = base.path().join("override");
    std::fs::create_dir_all(themes.join("x")).expect("creates");

    assert_eq!(
        hub_dirs(
            &[],
            Path::new("/etc/glimpse"),
            &base.path().join("data"),
            Some(themes.clone())
        ),
        [
            base.path().join("data/themes"),
            themes.clone(),
            themes.join("x")
        ]
    );
}

#[test]
fn a_symlinked_document_adds_the_directory_holding_its_target() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let store = root.path().join("dotfiles");
    let config = root.path().join("glimpse");
    std::fs::create_dir_all(&store).expect("creates");
    std::fs::create_dir_all(&config).expect("creates");
    std::fs::write(store.join("config.toml"), "").expect("writes");
    std::fs::write(store.join("panel.toml"), "").expect("writes");
    std::os::unix::fs::symlink(store.join("config.toml"), config.join("config.toml"))
        .expect("links");
    std::os::unix::fs::symlink(store.join("panel.toml"), config.join("panel.toml")).expect("links");

    let dirs = hub_dirs(
        std::slice::from_ref(&config),
        Path::new("/nonexistent"),
        Path::new("/nonexistent"),
        None,
    );

    assert_eq!(
        dirs,
        [
            config.clone(),
            config.join("panel.d"),
            config.join("themes"),
            store.canonicalize().expect("the target resolves"),
            PathBuf::from("/nonexistent/themes"),
        ]
    );
}

#[tokio::test]
async fn two_subscribers_on_one_hub_each_receive_one_signal_per_change() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let hub = hub_over(vec![dir.path().to_path_buf()]);
    let mut first = hub.subscribe();
    let mut second = hub.subscribe();

    std::fs::write(dir.path().join("config.toml"), "").expect("writes");

    for receiver in [&mut first, &mut second] {
        tokio::time::timeout(SETTLE, receiver.recv())
            .await
            .expect("a signal within the settle window")
            .expect("the hub is alive");
    }
    assert!(first.try_recv().is_err(), "one change, one signal");
    assert!(second.try_recv().is_err(), "one change, one signal");
}

#[tokio::test]
async fn a_subscriber_that_fell_behind_is_told_to_read_again_and_a_closed_hub_ends_it() {
    let (sender, receiver) = broadcast::channel(1);
    for _ in 0..3 {
        sender.send(()).expect("a receiver is listening");
    }
    let mut triggers = Box::pin(triggers(receiver));

    assert_eq!(triggers.next().await, Some(()), "lagged is a signal");
    drop(sender);
    while triggers.next().await.is_some() {}
}
