use std::path::{Path, PathBuf};

use futures_util::{StreamExt as _, stream, stream::BoxStream};
use glimpse_config::Update;
use glimpse_utils::clean;

use super::paths::{parse_bookmarks, parse_user_dirs};
use super::{ENTRIES, Kind, NAME_CAP, Place};

pub enum Event {
    Xdg(Result<Vec<Place>, String>),
    Bookmarks(Result<Vec<Place>, String>),
    Network(Result<Vec<Place>, String>),
    Trash(Result<usize, String>),
}

async fn read_text(path: &Path) -> Result<Option<String>, String> {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("its file cannot be read: {}", error.kind())),
    }
}

async fn read_dir_names(dir: &Path) -> Result<Vec<String>, String> {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("its directory cannot be read: {}", error.kind())),
    };

    let mut names = Vec::new();
    while names.len() < ENTRIES {
        let Ok(Some(entry)) = entries.next_entry().await else {
            break;
        };
        if let Ok(name) = entry.file_name().into_string() {
            names.push(name);
        }
    }
    Ok(names)
}

async fn exists(path: &Path) -> bool {
    tokio::fs::metadata(path).await.is_ok()
}

fn display_name(path: &Path, fallback: &str) -> String {
    let basename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(fallback);
    clean(basename, NAME_CAP)
}

fn kind_for(key: &str) -> Kind {
    match key {
        "desktop" => Kind::Desktop,
        "documents" => Kind::Documents,
        "download" => Kind::Download,
        "music" => Kind::Music,
        "pictures" => Kind::Pictures,
        "publicshare" => Kind::PublicShare,
        "templates" => Kind::Templates,
        "videos" => Kind::Videos,
        _ => Kind::Other,
    }
}

async fn xdg(config_dir: PathBuf, home: PathBuf) -> Event {
    let text = match read_text(&config_dir.join("user-dirs.dirs")).await {
        Ok(text) => text,
        Err(reason) => return Event::Xdg(Err(reason)),
    };

    let mut places = Vec::new();
    if exists(&home).await {
        places.push(Place {
            id: "home".to_owned(),
            name: "Home".to_owned(),
            path: home.clone(),
            kind: Kind::Home,
        });
    }

    if let Some(text) = text {
        for (key, dir) in parse_user_dirs(&text, &home) {
            if exists(&dir).await {
                places.push(Place {
                    id: key.clone(),
                    name: display_name(&dir, &key),
                    kind: kind_for(&key),
                    path: dir,
                });
            }
        }
    }

    Event::Xdg(Ok(places))
}

async fn bookmarks(bookmarks_dir: PathBuf) -> Event {
    let text = match read_text(&bookmarks_dir.join("bookmarks")).await {
        Ok(Some(text)) => text,
        Ok(None) => return Event::Bookmarks(Ok(Vec::new())),
        Err(reason) => return Event::Bookmarks(Err(reason)),
    };

    let mut places = Vec::new();
    for (uri, label, target) in parse_bookmarks(&text) {
        if !exists(&target).await {
            continue;
        }
        let name = label.unwrap_or_else(|| display_name(&target, &uri));
        places.push(Place {
            id: uri,
            name: clean(&name, NAME_CAP),
            path: target,
            kind: Kind::Bookmark,
        });
    }

    Event::Bookmarks(Ok(places))
}

async fn network(gvfs: PathBuf) -> Event {
    match read_dir_names(&gvfs).await {
        Ok(names) => {
            let places = names
                .into_iter()
                .map(|name| Place {
                    path: gvfs.join(&name),
                    name: clean(&name, NAME_CAP),
                    id: name,
                    kind: Kind::Network,
                })
                .collect();
            Event::Network(Ok(places))
        }
        Err(reason) => Event::Network(Err(reason)),
    }
}

async fn trash(trash_files: PathBuf) -> Event {
    match read_dir_names(&trash_files).await {
        Ok(names) => Event::Trash(Ok(names.len())),
        Err(reason) => Event::Trash(Err(reason)),
    }
}

fn unwatched(reason: String) -> String {
    format!("its directory is not being watched: {reason}")
}

pub(crate) fn xdg_source(config_dir: PathBuf, home: PathBuf) -> BoxStream<'static, Event> {
    let leading = stream::once(xdg(config_dir.clone(), home.clone()));
    let changes = glimpse_config::watch(config_dir.clone()).then(move |update| {
        let config_dir = config_dir.clone();
        let home = home.clone();
        async move {
            match update {
                Update::Changed(_) | Update::Rearmed => xdg(config_dir, home).await,
                Update::Unavailable(reason) => Event::Xdg(Err(unwatched(reason))),
            }
        }
    });
    leading.chain(changes).boxed()
}

pub(crate) fn bookmarks_source(config_dir: PathBuf) -> BoxStream<'static, Event> {
    let bookmarks_dir = config_dir.join("gtk-3.0");
    let leading = stream::once(bookmarks(bookmarks_dir.clone()));
    let changes = glimpse_config::watch(bookmarks_dir.clone()).then(move |update| {
        let bookmarks_dir = bookmarks_dir.clone();
        async move {
            match update {
                Update::Changed(_) | Update::Rearmed => bookmarks(bookmarks_dir).await,
                Update::Unavailable(reason) => Event::Bookmarks(Err(unwatched(reason))),
            }
        }
    });
    leading.chain(changes).boxed()
}

pub(crate) fn network_source(gvfs: PathBuf) -> BoxStream<'static, Event> {
    let leading = stream::once(network(gvfs.clone()));
    let changes = glimpse_config::watch(gvfs.clone()).then(move |update| {
        let gvfs = gvfs.clone();
        async move {
            match update {
                Update::Changed(_) | Update::Rearmed => network(gvfs).await,
                Update::Unavailable(reason) => Event::Network(Err(unwatched(reason))),
            }
        }
    });
    leading.chain(changes).boxed()
}

pub(crate) fn trash_source(trash_files: PathBuf) -> BoxStream<'static, Event> {
    let leading = stream::once(trash(trash_files.clone()));
    let changes = glimpse_config::watch(trash_files.clone()).then(move |update| {
        let trash_files = trash_files.clone();
        async move {
            match update {
                Update::Changed(_) | Update::Rearmed => trash(trash_files).await,
                Update::Unavailable(reason) => Event::Trash(Err(unwatched(reason))),
            }
        }
    });
    leading.chain(changes).boxed()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn an_xdg_directory_absent_on_disk_is_dropped_and_home_and_a_non_standard_key_survive() {
        let root = tempfile::tempdir().expect("a scratch directory");
        let home = root.path().join("home");
        let config = root.path().join("config");
        tokio::fs::create_dir_all(&home).await.expect("home");
        tokio::fs::create_dir_all(&config).await.expect("config");
        tokio::fs::create_dir_all(home.join("Desktop"))
            .await
            .expect("an existing desktop directory");
        tokio::fs::create_dir_all(home.join("Projects"))
            .await
            .expect("an existing, non-standard directory");

        let text = "XDG_DESKTOP_DIR=\"$HOME/Desktop\"\n\
                     XDG_DOCUMENTS_DIR=\"$HOME/Documents\"\n\
                     XDG_PROJECTS_DIR=\"$HOME/Projects\"\n";
        tokio::fs::write(config.join("user-dirs.dirs"), text)
            .await
            .expect("a user-dirs.dirs fixture");

        let Event::Xdg(Ok(places)) = xdg(config, home.clone()).await else {
            panic!("expected a successful read");
        };

        assert!(places.iter().any(|place| place.id == "home"));
        assert!(places.iter().any(|place| place.id == "desktop"));
        assert!(
            places
                .iter()
                .any(|place| place.id == "projects" && place.kind == Kind::Other),
            "a non-standard key must still be published"
        );
        assert!(
            !places.iter().any(|place| place.id == "documents"),
            "a configured directory absent on disk must never be published"
        );
    }

    #[tokio::test]
    async fn a_bookmark_uses_its_label_falls_back_to_the_basename_and_drops_a_missing_target() {
        let root = tempfile::tempdir().expect("a scratch directory");
        let gtk3 = root.path().join("gtk-3.0");
        tokio::fs::create_dir_all(&gtk3).await.expect("gtk-3.0 dir");

        let downloads = root.path().join("Downloads");
        let projects = root.path().join("projects");
        tokio::fs::create_dir_all(&downloads)
            .await
            .expect("downloads");
        tokio::fs::create_dir_all(&projects)
            .await
            .expect("projects");
        let gone = root.path().join("gone");

        let text = format!(
            "file://{downloads} Downloads\nfile://{projects}\nfile://{gone}\n",
            downloads = downloads.display(),
            projects = projects.display(),
            gone = gone.display(),
        );
        tokio::fs::write(gtk3.join("bookmarks"), text)
            .await
            .expect("a bookmarks fixture");

        let Event::Bookmarks(Ok(places)) = bookmarks(gtk3).await else {
            panic!("expected a successful read");
        };

        assert_eq!(places.len(), 2, "the missing target must be dropped");
        assert_eq!(
            places
                .iter()
                .find(|place| place.path == downloads)
                .map(|place| place.name.as_str()),
            Some("Downloads")
        );
        assert_eq!(
            places
                .iter()
                .find(|place| place.path == projects)
                .map(|place| place.name.as_str()),
            Some("projects"),
            "a line with no label falls back to the target's basename"
        );
    }

    #[tokio::test]
    async fn a_bookmark_label_of_400_multibyte_characters_is_capped_by_chars_not_bytes() {
        let root = tempfile::tempdir().expect("a scratch directory");
        let gtk3 = root.path().join("gtk-3.0");
        tokio::fs::create_dir_all(&gtk3).await.expect("gtk-3.0 dir");
        let target = root.path().join("target");
        tokio::fs::create_dir_all(&target).await.expect("target");

        let label: String = std::iter::repeat_n('é', 400).collect();
        let text = format!("file://{} {label}\n", target.display());
        tokio::fs::write(gtk3.join("bookmarks"), text)
            .await
            .expect("a bookmarks fixture");

        let Event::Bookmarks(Ok(places)) = bookmarks(gtk3).await else {
            panic!("expected a successful read");
        };

        assert_eq!(places.len(), 1);
        assert!(
            places[0].name.chars().count() <= NAME_CAP + 1,
            "a multibyte label must be capped by character count, not by byte length"
        );
    }

    #[tokio::test]
    async fn trash_counts_files_and_never_the_more_numerous_info_sidecars() {
        let root = tempfile::tempdir().expect("a scratch directory");
        let files = root.path().join("files");
        let info = root.path().join("info");
        tokio::fs::create_dir_all(&files).await.expect("files dir");
        tokio::fs::create_dir_all(&info).await.expect("info dir");
        for index in 0..3 {
            tokio::fs::write(files.join(format!("f{index}")), b"")
                .await
                .expect("a trashed file");
        }
        for index in 0..7 {
            tokio::fs::write(info.join(format!("f{index}.trashinfo")), b"")
                .await
                .expect("an info sidecar");
        }

        let Event::Trash(Ok(count)) = trash(files).await else {
            panic!("expected a successful read");
        };
        assert_eq!(
            count, 3,
            "info/ carries more entries and must never be the source of the count"
        );
    }

    #[tokio::test]
    async fn an_absent_gvfs_directory_publishes_no_shares_and_is_not_a_failure() {
        let root = tempfile::tempdir().expect("a scratch directory");
        let gvfs = root.path().join("gvfs");

        let Event::Network(Ok(places)) = network(gvfs).await else {
            panic!("an absent mount point must not be reported as a failure");
        };
        assert!(places.is_empty());
    }

    #[tokio::test]
    async fn an_empty_gvfs_directory_publishes_no_shares() {
        let root = tempfile::tempdir().expect("a scratch directory");
        let gvfs = root.path().join("gvfs");
        tokio::fs::create_dir_all(&gvfs)
            .await
            .expect("an empty mount point");

        let Event::Network(Ok(places)) = network(gvfs).await else {
            panic!("expected a successful, empty read");
        };
        assert!(places.is_empty());
    }

    #[tokio::test]
    async fn a_path_that_cannot_be_listed_as_a_directory_reports_a_failure() {
        let root = tempfile::tempdir().expect("a scratch directory");
        let blocked = root.path().join("gvfs");
        tokio::fs::write(&blocked, b"not a directory")
            .await
            .expect("a file blocking the mount point");

        let Event::Network(Err(reason)) = network(blocked).await else {
            panic!("a path that exists but is not a directory must fail, not read as empty");
        };
        assert!(!reason.is_empty());
    }

    #[tokio::test]
    async fn the_directory_read_stops_at_the_entry_cap() {
        let root = tempfile::tempdir().expect("a scratch directory");
        let files = root.path().join("files");
        tokio::fs::create_dir_all(&files).await.expect("files dir");
        for index in 0..(ENTRIES + 20) {
            tokio::fs::write(files.join(format!("f{index}")), b"")
                .await
                .expect("a trashed file");
        }

        let Event::Trash(Ok(count)) = trash(files).await else {
            panic!("expected a successful, capped read");
        };
        assert_eq!(
            count, ENTRIES,
            "the read must stop at the cap rather than growing unbounded"
        );
    }

    #[tokio::test]
    async fn the_first_value_arrives_without_waiting_for_a_watch_event() {
        let root = tempfile::tempdir().expect("a scratch directory");
        let home = root.path().join("home");
        let config = root.path().join("config");
        tokio::fs::create_dir_all(&home).await.expect("home");
        tokio::fs::create_dir_all(&config).await.expect("config");

        let mut stream = xdg_source(config, home);
        let first = tokio::time::timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("the first value must not wait for an inotify event")
            .expect("the stream is not empty");

        assert!(matches!(first, Event::Xdg(Ok(_))));
    }
}
