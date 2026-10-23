use super::*;
use futures_util::FutureExt;

fn camera_event(event: Event) -> Vec<Fact> {
    match event {
        Event::Camera(facts) => facts,
        _ => panic!("expected camera facts"),
    }
}

async fn next_event(events: &mut Events) -> Event {
    tokio::time::timeout(Duration::from_secs(5), events.next())
        .await
        .expect("camera event before timeout")
        .expect("camera source remains alive")
}

#[test]
fn a_process_holding_the_camera_is_named() {
    let dir = tempfile::tempdir().expect("temporary proc root");
    fake_proc(dir.path(), 4242, "ffmpeg", Some("/dev/video0"));
    let facts = holders(dir.path());
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].app.as_deref(), Some("ffmpeg"));
}

#[test]
fn pipewire_and_wireplumber_are_the_daemon_not_the_app() {
    assert_eq!(app_from_comm("pipewire"), None);
    assert_eq!(app_from_comm("wireplumber"), None);
    assert_eq!(app_from_comm("chrome"), Some("chrome".to_owned()));
}

#[test]
fn a_process_with_no_camera_fd_is_ignored() {
    let dir = tempfile::tempdir().expect("temporary proc root");
    fake_proc(dir.path(), 4242, "sshd", None);
    assert!(holders(dir.path()).is_empty());
}

#[tokio::test]
async fn no_nodes_emits_one_empty_event_and_stays_idle() {
    let dev = tempfile::tempdir().expect("temporary dev root");
    let proc = tempfile::tempdir().expect("temporary proc root");
    let mut events = camera(dev.path().to_owned(), proc.path().to_owned());
    assert!(camera_event(next_event(&mut events).await).is_empty());
    assert!(events.next().now_or_never().is_none());
}

#[tokio::test]
async fn readonly_open_is_immediate_and_close_is_delayed() {
    let dev = tempfile::tempdir().expect("temporary dev root");
    let proc = tempfile::tempdir().expect("temporary proc root");
    let node = dev.path().join("video0");
    std::fs::write(&node, []).expect("camera node");
    let mut events = camera(dev.path().to_owned(), proc.path().to_owned());
    assert!(camera_event(next_event(&mut events).await).is_empty());
    fake_proc(proc.path(), 42, "ffmpeg", Some("/dev/video0"));
    let fd = std::fs::File::open(node).expect("read-only open");
    let facts = camera_event(next_event(&mut events).await);
    assert_eq!(facts[0].app.as_deref(), Some("ffmpeg"));
    std::fs::remove_file(proc.path().join("42/fd/3")).expect("remove fake holder");
    let closed = Instant::now();
    drop(fd);
    assert!(camera_event(next_event(&mut events).await).is_empty());
    assert!(closed.elapsed() >= Duration::from_millis(500));
}

#[tokio::test(start_paused = true)]
async fn reopen_cancels_the_close_deadline_and_invalidates_empty_results() {
    let mut schedule = CameraSchedule::default();
    schedule.event(EventMask::OPEN);
    let scanning = schedule.generation;
    schedule.pending = false;
    schedule.event(EventMask::CLOSE_NOWRITE);
    assert!(!schedule.publish(scanning, &[]));
    tokio::time::advance(Duration::from_millis(250)).await;
    schedule.event(EventMask::OPEN);
    assert!(schedule.close_at.is_none());
    assert!(schedule.pending);
    assert!(!schedule.publish(scanning, &[]));
    assert!(schedule.publish(schedule.generation, &[]));
}

#[tokio::test(start_paused = true)]
async fn closes_replace_one_deadline_and_overflow_reconciles_immediately() {
    let mut schedule = CameraSchedule::default();
    schedule.event(EventMask::CLOSE_WRITE);
    let first = schedule.close_at.expect("close deadline");
    tokio::time::advance(Duration::from_millis(200)).await;
    schedule.event(EventMask::CLOSE_NOWRITE);
    assert_eq!(schedule.close_at, Some(first + Duration::from_millis(200)));
    schedule.event(EventMask::Q_OVERFLOW);
    assert!(schedule.pending);
    assert!(schedule.close_at.is_none());
    assert_eq!(schedule.generation, 1);
}

#[tokio::test]
async fn a_burst_drains_while_one_scan_is_in_flight() {
    let dev = tempfile::tempdir().expect("temporary dev root");
    let proc = tempfile::tempdir().expect("temporary proc root");
    let node = dev.path().join("video0");
    std::fs::write(&node, []).expect("camera node");
    fake_proc(proc.path(), 42, "ffmpeg", Some("/dev/video0"));
    let mut source =
        CameraSource::new(dev.path().to_owned(), proc.path().to_owned()).expect("source");
    let (release, blocked) = std::sync::mpsc::channel();
    source.scan = Some(tokio::task::spawn_blocking(move || {
        blocked
            .recv_timeout(Duration::from_secs(5))
            .expect("release scan");
        Vec::new()
    }));
    source.schedule.pending = false;
    for _ in 0..50 {
        let previous = source.schedule.generation;
        drop(std::fs::File::open(&node).expect("open camera"));
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                assert!(source.next().now_or_never().is_none());
                if source.schedule.generation != previous && source.schedule.close_at.is_some() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("drain kernel open and close events");
    }
    assert!(source.scan.is_some());
    assert!(source.schedule.pending);
    assert!(source.schedule.close_at.is_some());
    release.send(()).expect("release scan");
    let facts = camera_event(
        tokio::time::timeout(Duration::from_secs(5), source.next())
            .await
            .expect("pending scan")
            .expect("camera event"),
    );
    assert_eq!(facts[0].app.as_deref(), Some("ffmpeg"));
    std::fs::remove_file(proc.path().join("42/fd/3")).expect("remove holder");
    assert!(
        camera_event(
            tokio::time::timeout(Duration::from_secs(5), source.next())
                .await
                .expect("delayed scan")
                .expect("camera event")
        )
        .is_empty()
    );
    assert!(source.next().now_or_never().is_none());
}

#[tokio::test]
async fn unplug_while_open_clears_when_the_holder_closes() {
    let dev = tempfile::tempdir().expect("temporary dev root");
    let proc = tempfile::tempdir().expect("temporary proc root");
    let node = dev.path().join("video0");
    std::fs::write(&node, []).expect("camera node");
    let mut source =
        CameraSource::new(dev.path().to_owned(), proc.path().to_owned()).expect("source");
    assert!(
        camera_event(
            tokio::time::timeout(Duration::from_secs(5), source.next())
                .await
                .expect("initial scan")
                .expect("camera event")
        )
        .is_empty()
    );
    fake_proc(proc.path(), 42, "ffmpeg", Some("/dev/video0"));
    let fd = std::fs::File::open(&node).expect("hold the camera");
    assert_eq!(
        camera_event(
            tokio::time::timeout(Duration::from_secs(5), source.next())
                .await
                .expect("open scan")
                .expect("camera event")
        )[0]
        .app
        .as_deref(),
        Some("ffmpeg")
    );
    std::fs::remove_file(&node).expect("unplug");
    source.rearm();
    assert_eq!(
        camera_event(
            tokio::time::timeout(Duration::from_secs(5), source.next())
                .await
                .expect("scan while the unplugged node is still held")
                .expect("camera event")
        )[0]
        .app
        .as_deref(),
        Some("ffmpeg")
    );
    std::fs::remove_file(proc.path().join("42/fd/3")).expect("holder exits");
    drop(fd);
    assert!(
        camera_event(
            tokio::time::timeout(Duration::from_secs(5), source.next())
                .await
                .expect("close after unplug")
                .expect("camera event")
        )
        .is_empty()
    );
    assert!(source.next().now_or_never().is_none());
}

#[tokio::test]
async fn a_recreated_node_is_watched_while_the_previous_inode_is_open() {
    let dev = tempfile::tempdir().expect("temporary dev root");
    let proc = tempfile::tempdir().expect("temporary proc root");
    let node = dev.path().join("video0");
    std::fs::write(&node, []).expect("camera node");
    let mut source =
        CameraSource::new(dev.path().to_owned(), proc.path().to_owned()).expect("source");
    assert!(
        camera_event(
            tokio::time::timeout(Duration::from_secs(5), source.next())
                .await
                .expect("initial scan")
                .expect("camera event")
        )
        .is_empty()
    );
    fake_proc(proc.path(), 42, "ffmpeg", Some("/dev/video0"));
    let old = std::fs::File::open(&node).expect("hold the original node");
    assert_eq!(
        camera_event(
            tokio::time::timeout(Duration::from_secs(5), source.next())
                .await
                .expect("open scan")
                .expect("camera event")
        )[0]
        .app
        .as_deref(),
        Some("ffmpeg")
    );
    std::fs::remove_file(&node).expect("unplug");
    source.rearm();
    assert_eq!(
        camera_event(
            tokio::time::timeout(Duration::from_secs(5), source.next())
                .await
                .expect("held after unplug")
                .expect("camera event")
        )[0]
        .app
        .as_deref(),
        Some("ffmpeg")
    );
    std::fs::write(&node, []).expect("recreate the node");
    source.rearm();
    let _ = camera_event(
        tokio::time::timeout(Duration::from_secs(5), source.next())
            .await
            .expect("rescan after recreate")
            .expect("camera event"),
    );
    std::fs::remove_file(proc.path().join("42/fd/3")).expect("old holder record exits");
    drop(old);
    assert!(
        camera_event(
            tokio::time::timeout(Duration::from_secs(5), source.next())
                .await
                .expect("original inode closes")
                .expect("camera event")
        )
        .is_empty()
    );
    fake_proc(proc.path(), 43, "obs", Some("/dev/video0"));
    let _new = std::fs::File::open(&node).expect("open the recreated node");
    assert_eq!(
        camera_event(
            tokio::time::timeout(Duration::from_secs(5), source.next())
                .await
                .expect("recreated node is watched")
                .expect("camera event")
        )[0]
        .app
        .as_deref(),
        Some("obs")
    );
}

#[tokio::test]
async fn an_unreadable_dev_root_is_unavailable() {
    let dev = tempfile::tempdir().expect("temporary dev root");
    let proc = tempfile::tempdir().expect("temporary proc root");
    let mut events = camera(dev.path().join("missing"), proc.path().to_owned());
    assert!(matches!(
        next_event(&mut events).await,
        Event::Unavailable(Resource::Camera, _)
    ));
}

#[tokio::test]
async fn failed_watches_are_skipped_but_all_failed_is_unavailable() {
    use std::os::unix::fs::PermissionsExt;
    let dev = tempfile::tempdir().expect("temporary dev root");
    let proc = tempfile::tempdir().expect("temporary proc root");
    let denied = dev.path().join("video0");
    std::fs::write(&denied, []).expect("denied node");
    std::fs::set_permissions(&denied, std::fs::Permissions::from_mode(0o0)).expect("deny access");
    if std::fs::File::open(&denied).is_ok() {
        return;
    }
    let mut events = camera(dev.path().to_owned(), proc.path().to_owned());
    assert!(matches!(
        next_event(&mut events).await,
        Event::Unavailable(Resource::Camera, _)
    ));
    drop(events);
    let readable = dev.path().join("video1");
    std::fs::write(&readable, []).expect("readable node");
    let mut events = camera(dev.path().to_owned(), proc.path().to_owned());
    assert!(camera_event(next_event(&mut events).await).is_empty());
    fake_proc(proc.path(), 42, "ffmpeg", Some("/dev/video1"));
    let _fd = std::fs::File::open(readable).expect("open readable camera");
    assert_eq!(
        camera_event(next_event(&mut events).await)[0]
            .app
            .as_deref(),
        Some("ffmpeg")
    );
}

#[tokio::test]
async fn dropping_the_source_closes_its_inotify_descriptor() {
    use std::os::unix::fs::MetadataExt;
    let dev = tempfile::tempdir().expect("temporary dev root");
    let proc = tempfile::tempdir().expect("temporary proc root");
    let node = dev.path().join("video0");
    std::fs::write(&node, []).expect("camera node");
    let inode = format!("ino:{:x} ", node.metadata().expect("node metadata").ino());
    let source = CameraSource::new(dev.path().to_owned(), proc.path().to_owned()).expect("source");
    let descriptors = || {
        std::fs::read_dir("/proc/self/fdinfo")
            .expect("fdinfo")
            .flatten()
            .filter(|entry| {
                std::fs::read_to_string(entry.path()).is_ok_and(|info| info.contains(&inode))
            })
            .count()
    };
    assert_eq!(descriptors(), 1);
    drop(source);
    assert_eq!(descriptors(), 0);
}

#[tokio::test]
#[ignore = "needs an explicitly selected real camera node; opens read-only without capturing"]
async fn real_camera_open_and_close_are_observed_without_capturing() {
    let node =
        PathBuf::from(std::env::var_os("GLIMPSE_TEST_CAMERA_NODE").expect("explicit camera node"));
    assert_eq!(node.parent(), Some(Path::new("/dev")));
    assert!(
        node.file_name()
            .expect("node name")
            .to_string_lossy()
            .starts_with("video")
    );
    let mut events = camera(PathBuf::from("/dev"), PathBuf::from("/proc"));
    let initial = camera_event(next_event(&mut events).await);
    assert!(
        initial.is_empty(),
        "live camera must be idle before the check"
    );
    let fd = std::fs::File::open(node).expect("read-only camera open");
    assert!(!camera_event(next_event(&mut events).await).is_empty());
    let closed = Instant::now();
    drop(fd);
    assert!(camera_event(next_event(&mut events).await).is_empty());
    assert!(closed.elapsed() >= Duration::from_millis(500));
}
