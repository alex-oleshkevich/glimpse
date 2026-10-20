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
async fn the_network_listing_stops_at_the_entry_cap() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let gvfs = root.path().join("gvfs");
    tokio::fs::create_dir_all(&gvfs).await.expect("gvfs dir");
    for index in 0..(ENTRIES + 20) {
        tokio::fs::write(gvfs.join(format!("f{index}")), b"")
            .await
            .expect("a share");
    }

    let Event::Network(Ok(places)) = network(gvfs).await else {
        panic!("expected a successful, capped read");
    };
    assert_eq!(places.len(), ENTRIES);
}

#[tokio::test]
async fn trash_counts_every_entry_past_the_listing_cap() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let files = root.path().join("files");
    tokio::fs::create_dir_all(&files).await.expect("files dir");
    for index in 0..300 {
        tokio::fs::write(files.join(format!("f{index}")), b"")
            .await
            .expect("a trashed file");
    }

    assert!(matches!(trash(files).await, Event::Trash(Ok(300))));
}

#[tokio::test]
async fn an_unreadable_trash_path_reports_a_failure() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let blocked = root.path().join("files");
    tokio::fs::write(&blocked, b"not a directory")
        .await
        .expect("a file blocking the trash folder");

    assert!(matches!(trash(blocked).await, Event::Trash(Err(_))));
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

#[tokio::test]
async fn a_missing_user_dirs_file_yields_only_home() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let home = root.path().join("alex");
    let config = root.path().join("config");
    tokio::fs::create_dir_all(&home).await.expect("home");
    tokio::fs::create_dir_all(&config).await.expect("config");

    let Event::Xdg(Ok(places)) = xdg(config, home).await else {
        panic!("expected a successful read");
    };
    assert_eq!(places.len(), 1);
}

#[tokio::test]
async fn a_missing_trash_folder_is_not_a_failure_and_is_followed_once_created() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let files = root.path().join("Trash").join("files");
    let mut source = trash_source(files.clone());

    let Some(Event::Trash(first)) = source.next().await else {
        panic!("a leading read");
    };
    assert_eq!(first, Ok(0));

    tokio::fs::create_dir_all(&files)
        .await
        .expect("trash files");
    tokio::fs::write(files.join("a"), "")
        .await
        .expect("a trashed file");

    let next = tokio::time::timeout(Duration::from_secs(3), source.next()).await;
    match next {
        Ok(Some(Event::Trash(result))) => assert_eq!(result, Ok(1), "got {result:?}"),
        Ok(_) => panic!("stream ended"),
        Err(_) => panic!("no event after the folder was created"),
    }
}

#[tokio::test]
async fn a_disabled_xdg_directory_pointing_at_home_is_not_a_second_home() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let home = root.path().join("alex");
    let config = root.path().join("config");
    tokio::fs::create_dir_all(&home).await.expect("home");
    tokio::fs::create_dir_all(&config).await.expect("config");
    tokio::fs::write(
        config.join("user-dirs.dirs"),
        "XDG_TEMPLATES_DIR=\"$HOME/\"\nXDG_PUBLICSHARE_DIR=\"$HOME/\"\n",
    )
    .await
    .expect("user-dirs.dirs");

    let Event::Xdg(Ok(places)) = xdg(config, home.clone()).await else {
        panic!("expected a successful read");
    };
    let at_home: Vec<_> = places
        .iter()
        .filter(|place| place.path == home)
        .map(|place| (place.id.clone(), place.name.clone()))
        .collect();
    assert_eq!(at_home.len(), 1, "places at home: {at_home:?}");
}
