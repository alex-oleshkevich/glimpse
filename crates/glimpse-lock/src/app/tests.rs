use super::{ArtState, ArtTaken, ArtWorker, AvatarState, AvatarTaken, focused_output, reports};
use glimpse_services::{CompositorOutputs, CompositorState, OutputInfo};
use glimpse_widgets::{Artwork, Formats, Stamp, cached_artwork};
use gtk4::{gdk, glib, prelude::*};
use std::cell::RefCell;
use std::time::{Duration, Instant};
use tokio::sync::watch;

fn output(connector: &str, focused: bool) -> OutputInfo {
    OutputInfo {
        connector: connector.to_owned(),
        label: None,
        built_in: false,
        focused,
        make: None,
        model: None,
        current_mode: None,
        logical: None,
        enabled: true,
    }
}

#[test]
fn the_focused_output_comes_from_the_compositor_outputs() {
    assert_eq!(
        focused_output(&CompositorState::default()),
        None,
        "no outputs reported yet says nothing, rather than clearing focus"
    );
    let state = CompositorState {
        outputs: Some(CompositorOutputs {
            outputs: vec![output("eDP-1", false), output("DP-2", true)],
        }),
        ..Default::default()
    };
    assert_eq!(focused_output(&state), Some(Some("DP-2".to_owned())));
    let unfocused = CompositorState {
        outputs: Some(CompositorOutputs {
            outputs: vec![output("eDP-1", false)],
        }),
        ..Default::default()
    };
    assert_eq!(focused_output(&unfocused), Some(None));
}

#[tokio::test]
async fn reports_skip_the_placeholder_and_keep_a_value_published_before_subscribing() {
    let (publisher, held) = watch::channel(0);
    let seen = RefCell::new(Vec::new());
    let early = held.clone();
    publisher.send_replace(1);
    let late = held.clone();
    drop(held);
    drop(publisher);
    reports(early, |value| seen.borrow_mut().push(value)).await;
    reports(late, |value| seen.borrow_mut().push(value)).await;
    assert_eq!(
        seen.into_inner(),
        vec![1, 1],
        "a receiver cloned from the never-read original sees a publish from either side"
    );

    let (publisher, held) = watch::channel(0);
    let seen = RefCell::new(Vec::new());
    drop(publisher);
    reports(held, |value| seen.borrow_mut().push(value)).await;
    assert!(
        seen.into_inner().is_empty(),
        "nothing published means nothing reported, so no state is invented"
    );
}

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str, bytes: &[u8]) -> Self {
        let path =
            std::env::temp_dir().join(format!("glimpse-lock-app-{}-{name}", std::process::id()));
        std::fs::write(&path, bytes).expect("written");
        Self(path)
    }

    fn stamp(&self) -> Stamp {
        match cached_artwork(&self.0, 8, Formats::Strict, None) {
            Artwork::Rejected(Some(stamp)) => stamp,
            _ => panic!("a file that is not an image is rejected with its stamp"),
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn texture() -> gdk::Texture {
    gdk::MemoryTexture::new(
        1,
        1,
        gdk::MemoryFormat::R8g8b8,
        &glib::Bytes::from_static(&[0, 0, 0]),
        3,
    )
    .upcast()
}

fn decoded(path: &str, stamp: Stamp) -> (String, Artwork) {
    (path.to_owned(), Artwork::Decoded(texture(), stamp))
}

#[test]
fn art_is_requested_once_at_a_time_and_only_while_locked() {
    let now = Instant::now();
    let mut art = ArtState::default();

    assert!(
        art.request(Some("/a"), false, now).is_none(),
        "nothing is read unlocked"
    );
    let first = art.request(Some("/a"), true, now).expect("a request");
    assert_eq!((first.generation, first.path.as_str()), (0, "/a"));
    assert!(first.known.is_none());
    assert!(
        art.request(Some("/b"), true, now).is_none(),
        "a second decode waits for the first"
    );
}

#[test]
fn a_result_from_before_a_release_is_ignored_and_the_flag_is_reset() {
    let now = Instant::now();
    let scratch = Scratch::new("stale", b"x");
    let mut art = ArtState::default();
    let request = art.request(Some("/a"), true, now).expect("a request");

    art.release();
    assert!(
        art.request(Some("/a"), true, now).is_some(),
        "a release frees the slot"
    );

    let (path, loaded) = decoded("/a", scratch.stamp());
    assert_eq!(
        art.take(request.generation, path, loaded, Some("/a"), true, now),
        ArtTaken::Stale
    );
    assert!(art.shown.is_none(), "the stale texture is not shown");
    assert!(
        art.job.loading,
        "and the new request is still the one in flight"
    );
}

#[test]
fn a_result_after_unlock_is_dropped_and_frees_the_slot() {
    let now = Instant::now();
    let scratch = Scratch::new("dropped", b"x");
    let mut art = ArtState::default();
    let request = art.request(Some("/a"), true, now).expect("a request");
    let (path, loaded) = decoded("/a", scratch.stamp());

    assert_eq!(
        art.take(request.generation, path, loaded, Some("/a"), false, now),
        ArtTaken::Dropped
    );
    assert!(!art.job.loading);
    assert!(art.shown.is_none());
}

#[test]
fn a_result_for_a_track_that_is_no_longer_current_asks_for_the_current_one() {
    let now = Instant::now();
    let scratch = Scratch::new("moved-on", b"x");
    let mut art = ArtState::default();
    let request = art.request(Some("/a"), true, now).expect("a request");
    let (path, loaded) = decoded("/a", scratch.stamp());

    assert_eq!(
        art.take(request.generation, path, loaded, Some("/b"), true, now),
        ArtTaken::Reload
    );
    assert!(art.shown.is_none());
    assert!(art.request(Some("/b"), true, now).is_some());
}

#[test]
fn an_unreadable_path_is_retried_only_after_a_pause_or_a_new_path() {
    let now = Instant::now();
    let mut art = ArtState::default();
    let request = art.request(Some("/missing"), true, now).expect("a request");
    assert_eq!(
        art.take(
            request.generation,
            "/missing".to_owned(),
            Artwork::Rejected(None),
            Some("/missing"),
            true,
            now
        ),
        ArtTaken::Same
    );

    assert!(
        art.request(Some("/missing"), true, now + Duration::from_secs(29))
            .is_none(),
        "a media update inside the pause opens nothing"
    );
    let other = art
        .request(Some("/other"), true, now + Duration::from_secs(1))
        .expect("a different path is tried at once");
    art.take(
        other.generation,
        "/other".to_owned(),
        Artwork::Rejected(None),
        Some("/other"),
        true,
        now + Duration::from_secs(1),
    );
    let again = art
        .request(Some("/missing"), true, now + Duration::from_secs(31))
        .expect("and the first is retried after thirty seconds");
    assert!(again.known.is_none());
}

#[test]
fn a_rejected_file_is_not_decoded_again_until_its_stamp_changes() {
    let now = Instant::now();
    let scratch = Scratch::new("rejected", b"not an image");
    let stamp = scratch.stamp();
    let mut art = ArtState::default();
    let request = art.request(Some("/bad"), true, now).expect("a request");
    art.take(
        request.generation,
        "/bad".to_owned(),
        Artwork::Rejected(Some(stamp.clone())),
        Some("/bad"),
        true,
        now,
    );

    let again = art.request(Some("/bad"), true, now).expect("asked again");
    assert_eq!(
        again.known,
        Some(stamp.clone()),
        "the worker is told what was seen"
    );
    assert!(matches!(
        cached_artwork(&scratch.0, 8, Formats::Strict, again.known.as_ref()),
        Artwork::Unchanged
    ));

    std::fs::write(&scratch.0, b"not an image, but longer").expect("rewritten");
    assert!(!matches!(
        cached_artwork(&scratch.0, 8, Formats::Strict, Some(&stamp)),
        Artwork::Unchanged
    ));
}

#[test]
fn art_that_vanishes_is_cleared_and_a_missing_track_clears_it_too() {
    let now = Instant::now();
    let scratch = Scratch::new("vanishes", b"x");
    let mut art = ArtState::default();
    let request = art.request(Some("/a"), true, now).expect("a request");
    let (path, loaded) = decoded("/a", scratch.stamp());
    assert_eq!(
        art.take(request.generation, path, loaded, Some("/a"), true, now),
        ArtTaken::Changed
    );
    assert!(art.shown.is_some());

    let request = art.request(Some("/a"), true, now).expect("a request");
    assert_eq!(request.known, Some(scratch.stamp()));
    assert_eq!(
        art.take(
            request.generation,
            "/a".to_owned(),
            Artwork::Rejected(None),
            Some("/a"),
            true,
            now
        ),
        ArtTaken::Changed
    );
    assert!(art.shown.is_none());

    assert!(art.request(None, true, now).is_none());
    assert!(
        art.seen.is_none() && art.failed.is_none(),
        "no wanted art forgets what was seen"
    );
}

#[test]
fn releasing_forgets_all_art_state() {
    let now = Instant::now();
    let scratch = Scratch::new("release", b"x");
    let mut art = ArtState::default();
    let request = art.request(Some("/a"), true, now).expect("a request");
    let (path, loaded) = decoded("/a", scratch.stamp());
    art.take(request.generation, path, loaded, Some("/a"), true, now);
    art.request(Some("/a"), true, now);

    art.release();

    assert!(art.shown.is_none() && art.seen.is_none() && art.failed.is_none());
    assert!(!art.job.loading);
    assert_eq!(art.job.generation, 1);
}

#[test]
fn the_avatar_loads_once_per_lock_and_only_while_locked() {
    let mut avatar = AvatarState {
        icon: Some("/icon".to_owned()),
        ..Default::default()
    };

    assert!(avatar.request(false).is_none());
    let request = avatar.request(true).expect("a request");
    assert_eq!(request.icon, "/icon");
    assert!(avatar.request(true).is_none(), "one decode in flight");
    assert_eq!(
        avatar.take(request.generation, "/icon", Some(texture()), true),
        AvatarTaken::Applied
    );
    assert!(avatar.shown.is_some());
    assert!(
        avatar.request(true).is_none(),
        "and not again for the same lock"
    );

    let mut empty = AvatarState {
        icon: Some(String::new()),
        ..Default::default()
    };
    assert!(
        empty.request(true).is_none(),
        "an empty IconFile names nothing"
    );
}

#[test]
fn an_avatar_for_an_icon_that_changed_is_reloaded() {
    let mut avatar = AvatarState {
        icon: Some("/old".to_owned()),
        ..Default::default()
    };
    let request = avatar.request(true).expect("a request");
    avatar.icon = Some("/new".to_owned());

    assert_eq!(
        avatar.take(request.generation, "/old", Some(texture()), true),
        AvatarTaken::Reload
    );
    assert!(avatar.shown.is_none());
    assert_eq!(
        avatar.request(true).map(|request| request.icon).as_deref(),
        Some("/new")
    );
}

#[test]
fn an_avatar_after_unlock_is_dropped() {
    let mut avatar = AvatarState {
        icon: Some("/icon".to_owned()),
        ..Default::default()
    };
    let request = avatar.request(true).expect("a request");

    assert_eq!(
        avatar.take(request.generation, "/icon", Some(texture()), false),
        AvatarTaken::Dropped
    );
    assert!(avatar.shown.is_none());
    assert!(!avatar.job.loading);
}

#[test]
fn an_avatar_from_before_a_release_is_ignored() {
    let mut avatar = AvatarState {
        icon: Some("/icon".to_owned()),
        ..Default::default()
    };
    let stale = avatar.request(true).expect("a request");

    avatar.release();

    assert!(avatar.icon.is_none() && avatar.shown.is_none() && !avatar.tried);
    avatar.icon = Some("/icon".to_owned());
    let fresh = avatar.request(true).expect("a release frees the slot");
    assert_eq!(
        avatar.take(stale.generation, "/icon", Some(texture()), true),
        AvatarTaken::Stale
    );
    assert!(avatar.shown.is_none(), "the stale texture is not shown");
    assert_eq!(
        avatar.take(fresh.generation, "/icon", Some(texture()), true),
        AvatarTaken::Applied
    );
}

#[test]
fn art_decodes_run_in_order_on_one_thread_that_a_release_ends() {
    let (done, results) = std::sync::mpsc::channel();
    let mut worker = ArtWorker::default();
    for number in 0..3 {
        let done = done.clone();
        worker.submit(move || {
            done.send((number, std::thread::current().id()))
                .expect("sent")
        });
    }
    let first: Vec<_> = (0..3)
        .map(|_| {
            results
                .recv_timeout(Duration::from_secs(5))
                .expect("a result")
        })
        .collect();
    assert_eq!(
        first.iter().map(|(number, _)| *number).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert!(
        first.iter().all(|(_, thread)| *thread == first[0].1),
        "one thread serves every decode, so one malloc arena"
    );
    assert_ne!(first[0].1, std::thread::current().id());

    worker.release();
    worker.submit(move || done.send((3, std::thread::current().id())).expect("sent"));
    let (number, _) = results
        .recv_timeout(Duration::from_secs(5))
        .expect("a result");
    assert_eq!(number, 3, "a released worker starts again on the next lock");
}
