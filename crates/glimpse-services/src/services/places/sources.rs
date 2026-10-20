use std::path::{Path, PathBuf};

use futures_util::{Stream, StreamExt as _, stream, stream::BoxStream};
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

async fn count_entries(dir: &Path) -> Result<usize, String> {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(format!("its directory cannot be read: {}", error.kind())),
    };

    let mut count = 0;
    while let Ok(Some(_)) = entries.next_entry().await {
        count += 1;
    }
    Ok(count)
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
            if dir != home && exists(&dir).await {
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
    Event::Trash(count_entries(&trash_files).await)
}

fn follow(dir: &Path, depth: usize) -> impl Stream<Item = Update> + Send + 'static {
    glimpse_config::watch_all(
        dir.ancestors()
            .take(depth)
            .map(Path::to_path_buf)
            .collect::<Vec<_>>(),
    )
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
    let changes = follow(&bookmarks_dir, 2).then(move |update| {
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
    let changes = follow(&gvfs, 2).then(move |update| {
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
    let changes = follow(&trash_files, 3).then(move |update| {
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
mod tests;
