use std::{fs, path::PathBuf, time::Duration};

use notify::EventKind;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use tokio::sync::mpsc;

pub async fn watch_config_file<T, W, F>(
    config_file: PathBuf,
    sender: mpsc::Sender<T>,
    label: &'static str,
    mut watch_files: W,
    mut load_event: F,
) where
    T: Send + 'static,
    W: FnMut(&std::path::Path) -> Vec<PathBuf> + Send + 'static,
    F: FnMut(&std::path::Path) -> T + Send + 'static,
{
    let watch_file = config_file
        .canonicalize()
        .unwrap_or_else(|_| config_file.clone());
    let Some(config_dir) = config_file.parent().map(PathBuf::from) else {
        tracing::error!("{label} config file has no parent directory");
        return;
    };
    if let Err(err) = fs::create_dir_all(&config_dir) {
        tracing::error!("failed to create {label} config directory: {err}");
        return;
    }

    let mut watched_files = normalize_watch_files(&watch_file, watch_files(&watch_file));
    let target_dir = watch_file.parent().map(PathBuf::from);
    let mut watched_dirs = Vec::new();

    tracing::info!(
        config_file = %watch_file.display(),
        "watching {label} config file for changes"
    );
    for file in &watched_files {
        tracing::debug!(
            config_file = %watch_file.display(),
            watched_file = %file.display(),
            "watching {label} config dependency"
        );
    }

    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<Vec<PathBuf>>();
    let mut debouncer = match new_debouncer(
        Duration::from_millis(200),
        None,
        move |res: DebounceEventResult| {
            let events = match res {
                Ok(events) => events,
                Err(_) => return,
            };

            for event in events {
                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                        let _ = event_tx.send(event.paths.clone());
                    }
                    _ => {}
                }
            }
        },
    ) {
        Ok(debouncer) => debouncer,
        Err(err) => {
            tracing::error!("failed to create {label} config watcher: {err}");
            return;
        }
    };

    watch_new_dirs(
        &mut debouncer,
        &mut watched_dirs,
        watch_dirs(config_dir.clone(), target_dir.clone(), &watched_files),
        label,
    );
    if watched_dirs.is_empty() {
        return;
    }

    loop {
        tokio::select! {
            _ = sender.closed() => break,
            event_paths = event_rx.recv() => {
                let Some(event_paths) = event_paths else {
                    break;
                };
                if event_paths.iter().any(|path| watched_files.iter().any(|file| path_matches(path, file))) {
                    let event = load_event(&watch_file);
                    if let Err(err) = sender.try_send(event) {
                        tracing::error!(
                            "failed to broadcast {label} config change to the app: {err}"
                        );
                    }

                    watched_files = normalize_watch_files(&watch_file, watch_files(&watch_file));
                    for file in &watched_files {
                        tracing::debug!(
                            config_file = %watch_file.display(),
                            watched_file = %file.display(),
                            "watching {label} config dependency"
                        );
                    }
                    watch_new_dirs(
                        &mut debouncer,
                        &mut watched_dirs,
                        watch_dirs(config_dir.clone(), target_dir.clone(), &watched_files),
                        label,
                    );
                }
            }
        }
    }
}

fn normalize_watch_files(root: &std::path::Path, files: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut normalized = vec![root.to_path_buf()];
    for file in files {
        let file = file.canonicalize().unwrap_or(file);
        if !normalized.iter().any(|existing| existing == &file) {
            normalized.push(file);
        }
    }
    normalized
}

fn watch_new_dirs<W, C>(
    debouncer: &mut notify_debouncer_full::Debouncer<W, C>,
    watched_dirs: &mut Vec<PathBuf>,
    dirs: Vec<PathBuf>,
    label: &'static str,
) where
    W: notify::Watcher,
    C: notify_debouncer_full::FileIdCache,
{
    for dir in dirs {
        if watched_dirs.iter().any(|watched| watched == &dir) {
            continue;
        }
        match debouncer.watch(&dir, notify::RecursiveMode::NonRecursive) {
            Ok(()) => watched_dirs.push(dir),
            Err(err) => {
                tracing::error!(
                    config_dir = %dir.display(),
                    "failed to watch {label} config directory: {err}"
                );
            }
        }
    }
}

fn watch_dirs(
    config_dir: PathBuf,
    target_dir: Option<PathBuf>,
    watched_files: &[PathBuf],
) -> Vec<PathBuf> {
    let mut dirs = vec![config_dir];
    if let Some(target_dir) = target_dir {
        if !dirs.iter().any(|dir| dir == &target_dir) {
            dirs.push(target_dir);
        }
    }
    for file in watched_files {
        if let Some(dir) = file.parent().map(PathBuf::from) {
            if !dirs.iter().any(|existing| existing == &dir) {
                dirs.push(dir);
            }
        }
    }
    dirs
}

fn path_matches(path: &std::path::Path, expected: &std::path::Path) -> bool {
    path == expected
        || path
            .canonicalize()
            .map(|path| path == expected)
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tokio::time::{Duration, timeout};

    #[test]
    fn watch_dirs_includes_config_and_symlink_target_dirs() {
        let config_dir = PathBuf::from("/config");
        let target_dir = PathBuf::from("/target");

        assert_eq!(
            watch_dirs(config_dir.clone(), Some(target_dir.clone()), &[]),
            vec![config_dir, target_dir]
        );
    }

    #[test]
    fn watch_dirs_deduplicates_matching_config_and_target_dirs() {
        let config_dir = PathBuf::from("/config");

        assert_eq!(
            watch_dirs(config_dir.clone(), Some(config_dir.clone()), &[]),
            vec![config_dir]
        );
    }

    #[test]
    fn watch_dirs_includes_include_file_parent_dirs() {
        let config_dir = PathBuf::from("/config");
        let target_dir = PathBuf::from("/target");
        let include = PathBuf::from("/config/parts/theme.toml");
        let external = PathBuf::from("/shared/glimpse/base.toml");

        assert_eq!(
            watch_dirs(
                config_dir.clone(),
                Some(target_dir.clone()),
                &[include, external]
            ),
            vec![
                config_dir,
                target_dir,
                PathBuf::from("/config/parts"),
                PathBuf::from("/shared/glimpse")
            ]
        );
    }

    #[tokio::test]
    async fn watch_config_file_notices_changes_to_included_file() {
        let temp = TestDir::new("watch-included-file");
        let config = temp.file("config.toml");
        let include = temp.file("parts/theme.toml");
        fs::create_dir_all(include.parent().unwrap()).unwrap();
        fs::write(&config, "include = [\"parts/theme.toml\"]\n").unwrap();
        fs::write(&include, "theme = \"adwaita\"\n").unwrap();

        let include_for_watcher = include.clone();
        let (tx, mut rx) = mpsc::channel(1);
        let task = tokio::spawn(watch_config_file(
            config.clone(),
            tx,
            "test",
            move |path| vec![path.to_path_buf(), include_for_watcher.clone()],
            |path| path.to_path_buf(),
        ));

        tokio::time::sleep(Duration::from_millis(300)).await;
        fs::write(&include, "theme = \"rosepine\"\n").unwrap();

        let event = timeout(Duration::from_secs(3), rx.recv())
            .await
            .expect("watcher should emit config change")
            .expect("watcher channel should stay open");
        assert_eq!(event, config.canonicalize().unwrap());

        drop(rx);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn watch_config_file_updates_include_dependencies_after_reload() {
        let temp = TestDir::new("watch-dynamic-includes");
        let config = temp.file("config.toml");
        let include = temp.file("parts/theme.toml");
        fs::create_dir_all(include.parent().unwrap()).unwrap();
        fs::write(&config, "theme = \"adwaita\"\n").unwrap();
        fs::write(&include, "theme = \"rosepine\"\n").unwrap();

        let include_for_watcher = include.clone();
        let (tx, mut rx) = mpsc::channel(4);
        let task = tokio::spawn(watch_config_file(
            config.clone(),
            tx,
            "test",
            move |path| {
                let content = fs::read_to_string(path).unwrap_or_default();
                let mut files = vec![path.to_path_buf()];
                if content.contains("parts/theme.toml") {
                    files.push(include_for_watcher.clone());
                }
                files
            },
            |path| path.to_path_buf(),
        ));

        tokio::time::sleep(Duration::from_millis(300)).await;
        fs::write(&config, "include = [\"parts/theme.toml\"]\n").unwrap();

        let event = timeout(Duration::from_secs(3), rx.recv())
            .await
            .expect("root config change should emit reload")
            .expect("watcher channel should stay open");
        assert_eq!(event, config.canonicalize().unwrap());

        fs::write(&include, "theme = \"adwaita\"\n").unwrap();
        let event = timeout(Duration::from_secs(3), rx.recv())
            .await
            .expect("newly included file change should emit reload")
            .expect("watcher channel should stay open");
        assert_eq!(event, config.canonicalize().unwrap());

        drop(rx);
        task.await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn watch_config_file_notices_changes_to_symlink_target() {
        let temp = TestDir::new("watch-symlink-target");
        let target = temp.file("target/config.toml");
        let link = temp.file("xdg/glimpse/config.toml");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        fs::write(&target, "first").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let (tx, mut rx) = mpsc::channel(1);
        let task = tokio::spawn(watch_config_file(
            link,
            tx,
            "test",
            |path| vec![path.to_path_buf()],
            |path| path.to_path_buf(),
        ));

        tokio::time::sleep(Duration::from_millis(300)).await;
        fs::write(&target, "second").unwrap();

        let event = timeout(Duration::from_secs(3), rx.recv())
            .await
            .expect("watcher should emit config change")
            .expect("watcher channel should stay open");
        assert_eq!(event, target);

        drop(rx);
        task.await.unwrap();
    }

    struct TestDir {
        root: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("glimpse-{name}-{suffix}"));
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn file(&self, relative: &str) -> PathBuf {
            self.root.join(relative)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
