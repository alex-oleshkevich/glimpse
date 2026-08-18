use std::collections::HashSet;
use std::time::Duration;
use std::{fs, path::PathBuf};

use notify::EventKind;
use notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use tokio::sync::{mpsc, watch};

use crate::config::{AppletDirectoryScanner, DiscoveredApplets};

pub struct AppletWatcher;

impl AppletWatcher {
    pub fn start(scanner: AppletDirectoryScanner) -> watch::Receiver<DiscoveredApplets> {
        let initial = scanner.scan();
        log_discovered(&initial);
        let (state_tx, state_rx) = watch::channel(initial);

        let system_dir = scanner.system_dir.clone();
        let user_dir = scanner.user_dir.clone();

        tokio::spawn(async move {
            let (change_tx, mut change_rx) = mpsc::channel::<()>(1);
            let notif_tx = change_tx.clone();

            let mut debouncer = match new_debouncer(
                Duration::from_millis(200),
                None,
                move |res: DebounceEventResult| {
                    if let Ok(events) = res {
                        let relevant = events.iter().any(|e| {
                            matches!(
                                e.kind,
                                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                            )
                        });
                        if relevant {
                            let _ = notif_tx.try_send(());
                        }
                    }
                },
            ) {
                Ok(d) => d,
                Err(err) => {
                    tracing::error!(%err, "failed to create applet dirs watcher");
                    return;
                }
            };

            if !user_dir.as_os_str().is_empty() {
                if let Err(err) = fs::create_dir_all(&user_dir) {
                    tracing::warn!(dir = %user_dir.display(), %err, "could not create user applet dir");
                }
            }

            let mut watched_any = false;
            for dir in [&system_dir, &user_dir] {
                if !dir.as_os_str().is_empty() {
                    match debouncer.watch(dir, RecursiveMode::NonRecursive) {
                        Ok(()) => {
                            tracing::info!(dir = %dir.display(), "watching applet directory for changes");
                            watched_any = true;
                        }
                        Err(err) => {
                            tracing::warn!(dir = %dir.display(), %err, "could not watch applet dir");
                        }
                    }
                }
            }

            if !watched_any {
                return;
            }

            let mut watched_targets: HashSet<PathBuf> = HashSet::new();
            update_symlink_watches(
                &mut debouncer,
                &[&system_dir, &user_dir],
                &mut watched_targets,
            );

            while change_rx.recv().await.is_some() {
                tracing::info!("applet directory change detected; rescanning");
                let s = scanner.clone();
                let discovered = match tokio::task::spawn_blocking(move || s.scan()).await {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::error!(%e, "applet scan panicked; keeping previous state");
                        continue;
                    }
                };
                log_changes(&state_tx.borrow(), &discovered);
                update_symlink_watches(
                    &mut debouncer,
                    &[&system_dir, &user_dir],
                    &mut watched_targets,
                );
                state_tx.send_if_modified(|current| {
                    if *current == discovered {
                        false
                    } else {
                        *current = discovered;
                        true
                    }
                });
                if state_tx.is_closed() {
                    break;
                }
            }
        });

        state_rx
    }
}

type Debouncer = notify_debouncer_full::Debouncer<
    notify::RecommendedWatcher,
    notify_debouncer_full::RecommendedCache,
>;

fn update_symlink_watches(
    debouncer: &mut Debouncer,
    dirs: &[&PathBuf],
    watched: &mut HashSet<PathBuf>,
) {
    let mut current_targets: HashSet<PathBuf> = HashSet::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_symlink() {
                // Prefer the canonicalized path so inotify tracks the real
                // inode. Fall back to the raw link target's parent for broken
                // symlinks (target not yet created) so we still watch the
                // right directory and pick up the file when it appears.
                let target_dir = if let Ok(resolved) = fs::canonicalize(&path) {
                    // Resolved: `resolved` is the real file path.
                    resolved.parent().map(|p| p.to_path_buf())
                } else if let Ok(raw) = fs::read_link(&path) {
                    // Broken symlink: target doesn't exist yet; watch its parent
                    // so we notice when the file is created.
                    let abs = if raw.is_absolute() {
                        raw
                    } else {
                        dir.join(raw)
                    };
                    abs.parent().map(|p| p.to_path_buf())
                } else {
                    None
                };
                if let Some(d) = target_dir {
                    current_targets.insert(d);
                }
            }
        }
    }

    for target_dir in &current_targets {
        if !watched.contains(target_dir) {
            match debouncer.watch(target_dir, RecursiveMode::NonRecursive) {
                Ok(()) => {
                    tracing::info!(dir = %target_dir.display(), "watching symlink target directory for changes");
                    watched.insert(target_dir.clone());
                }
                Err(err) => {
                    tracing::warn!(dir = %target_dir.display(), %err, "could not watch symlink target dir");
                    // Do not insert into `watched` — retry on next rescan.
                }
            }
        }
    }

    let stale: Vec<PathBuf> = watched.difference(&current_targets).cloned().collect();
    for dir in stale {
        if let Err(err) = debouncer.unwatch(&dir) {
            tracing::warn!(dir = %dir.display(), %err, "could not unwatch stale symlink target dir");
        }
        watched.remove(&dir);
    }
}

fn log_discovered(d: &DiscoveredApplets) {
    tracing::info!(
        normal = d.normal.len(),
        dev = d.dev.len(),
        "applets discovered"
    );
    for id in d.normal.keys() {
        tracing::debug!(id, "discovered applet");
    }
    for id in d.dev.keys() {
        tracing::debug!(id, "discovered dev applet");
    }
}

fn log_changes(old: &DiscoveredApplets, new: &DiscoveredApplets) {
    for id in new.normal.keys() {
        if !old.normal.contains_key(id) {
            tracing::info!(id, "applet added");
        } else if old.normal[id].extends != new.normal[id].extends {
            tracing::info!(id, "applet type changed");
        }
    }
    for id in old.normal.keys() {
        if !new.normal.contains_key(id) {
            tracing::info!(id, "applet removed");
        }
    }
    for id in new.dev.keys() {
        if !old.dev.contains_key(id) {
            tracing::info!(id, "dev applet added");
        }
    }
    for id in old.dev.keys() {
        if !new.dev.contains_key(id) {
            tracing::info!(id, "dev applet removed");
        }
    }
}
