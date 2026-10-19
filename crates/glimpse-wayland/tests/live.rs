use std::time::{Duration, Instant};

use futures_util::StreamExt as _;
use glimpse_wayland::{capture, selection};

#[test]
#[ignore = "needs a Wayland compositor"]
fn every_output_is_captured_and_one_can_be_named() {
    let deadline = Instant::now() + Duration::from_secs(3);
    capture::capabilities(deadline).unwrap();

    let frames = capture::outputs(deadline, false).unwrap();
    assert!(!frames.is_empty());
    for frame in &frames {
        assert_eq!(
            frame.pixels.len(),
            frame.width as usize * frame.height as usize * 4
        );
    }

    let named = capture::output(&frames[0].connector, deadline, true).unwrap();
    assert_eq!(named.connector, frames[0].connector);
    assert_eq!(
        (named.width, named.height),
        (frames[0].width, frames[0].height)
    );
    assert!(matches!(
        capture::output("no-such-output", deadline, false),
        Err(capture::CaptureError::NoSuchOutput(name)) if name == "no-such-output"
    ));
}

#[test]
#[ignore = "needs a Wayland compositor"]
fn the_selection_is_read_without_being_changed() {
    let changes = selection::Changes::start().unwrap();
    let first = selection::snapshot(Duration::from_secs(1)).unwrap();
    let second = selection::snapshot(Duration::from_secs(1)).unwrap();
    assert_eq!(first.entries, second.entries);
    assert_eq!(first.skipped, second.skipped);
    assert!(changes.since().is_empty());
}

#[tokio::test]
#[ignore = "needs a Wayland compositor"]
async fn watching_announces_itself() {
    let mut watched = Box::pin(selection::watch(|| true));

    let first = tokio::time::timeout(Duration::from_secs(5), watched.next()).await;

    assert!(matches!(first, Ok(Some(selection::Watched::Watching))));
}

fn copy(mime: &str, data: &[u8]) {
    use std::io::Write as _;
    let mut copying = std::process::Command::new("wl-copy")
        .args(["--type", mime])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    copying.stdin.take().unwrap().write_all(data).unwrap();
    assert!(copying.wait().unwrap().success());
}

fn restore(before: &selection::Snapshot) {
    let utf8 = before
        .entries
        .iter()
        .find(|entry| entry.mime == "text/plain;charset=utf-8");
    match utf8.or(before.entries.first()) {
        Some(entry) => copy(&entry.mime, &entry.data),
        None => {
            let cleared = std::process::Command::new("wl-copy")
                .arg("--clear")
                .status()
                .unwrap();
            assert!(cleared.success());
        }
    }
}

#[tokio::test]
#[ignore = "needs a Wayland compositor and wl-copy; replaces the clipboard, then restores it"]
async fn a_watch_nobody_wants_reads_no_selection() {
    let before = selection::snapshot(Duration::from_secs(1)).unwrap();
    assert!(
        before.skipped.is_empty(),
        "refusing to replace a clipboard that cannot be restored in full: {:?}",
        before.skipped
    );
    let mut watched = Box::pin(selection::watch(|| false));
    let first = tokio::time::timeout(Duration::from_secs(5), watched.next()).await;
    assert!(matches!(first, Ok(Some(selection::Watched::Watching))));

    copy("text/plain;charset=utf-8", b"glimpse-wayland live test");
    let read = tokio::time::timeout(Duration::from_secs(1), async {
        while let Some(event) = watched.next().await {
            if let selection::Watched::Incoming(incoming) = event
                && incoming.entry.is_some()
            {
                return true;
            }
        }
        false
    })
    .await;
    restore(&before);

    assert!(
        !matches!(read, Ok(true)),
        "a selection was read with nobody wanting it"
    );
}
