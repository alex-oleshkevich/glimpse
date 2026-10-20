use super::*;
use rustix::fs::{fcntl_getfl, fcntl_setfl};

fn mimes(list: &[&str]) -> Vec<String> {
    list.iter().map(|mime| (*mime).to_owned()).collect()
}

#[test]
fn the_verdict_on_a_selection_nobody_wants_is_unwanted() {
    assert_eq!(verdict(&mimes(&["text/plain"]), false), None);
    assert_eq!(
        verdict(&mimes(&["text/plain"]), true),
        Some("text/plain".to_owned())
    );
}

#[test]
fn a_sensitive_selection_someone_wants_is_still_not_read() {
    assert_eq!(
        verdict(&mimes(&["text/plain", "x-kde-passwordManagerHint"]), true),
        None
    );
}

#[test]
fn the_ladder_prefers_utf8_text_over_every_other_spelling() {
    let chosen = choose(&mimes(&[
        "STRING",
        "text/plain",
        "text/plain;charset=utf-8",
    ]));

    assert_eq!(chosen.as_deref(), Some("text/plain;charset=utf-8"));
}

#[test]
fn an_offer_of_only_an_image_is_read_as_an_image() {
    assert_eq!(choose(&mimes(&["image/png"])).as_deref(), Some("image/png"));
}

#[test]
fn text_is_preferred_to_an_image_offered_beside_it() {
    assert_eq!(
        choose(&mimes(&["image/png", "text/plain"])).as_deref(),
        Some("text/plain")
    );
}

#[test]
fn an_offer_of_nothing_we_understand_is_not_read() {
    assert_eq!(choose(&mimes(&["application/x-private"])), None);
    assert_eq!(verdict(&mimes(&["application/x-private"]), true), None);
    assert_eq!(choose(&[]), None);
}

#[test]
fn a_silent_read_releases_the_only_blocking_thread_and_watch_can_continue() {
    use std::io::Write as _;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    let (reader, writer) = std::io::pipe().unwrap();
    let result = runtime.block_on(async {
        let (events, mut received) = mpsc::unbounded_channel();
        let mut reading = read_entry(reader, "text/plain".to_owned(), events.clone());
        assert!(futures_util::poll!(reading.as_mut()).is_pending());
        let sentinel = tokio::task::spawn_blocking(|| ());
        let freed = tokio::time::timeout(READ_TIMEOUT + Duration::from_secs(1), sentinel).await;
        if freed.is_err() {
            return false;
        }
        reading.await;
        assert!(received.try_recv().is_err());
        let (reader, mut writer) = std::io::pipe().unwrap();
        let reading = read_entry(reader, "text/plain".to_owned(), events);
        writer.write_all(b"next").unwrap();
        drop(writer);
        reading.await;
        matches!(received.recv().await, Some(Watched::Incoming(entry)) if &*entry.data == b"next")
    });
    drop(writer);
    runtime.shutdown_timeout(Duration::from_secs(1));
    assert!(
        result,
        "the silent read parked the blocking pool past its deadline"
    );
}

#[test]
fn selection_reads_reject_oversize_data_and_accept_empty_eof() {
    use std::io::Write as _;
    let (reader, mut writer) = std::io::pipe().unwrap();
    let writing = std::thread::spawn(move || {
        let _ = writer.write_all(&vec![0; super::super::MAX_READ_BYTES + 1]);
    });
    assert!(matches!(
        drain(reader, Instant::now() + READ_TIMEOUT),
        Err(super::super::SkipReason::TooLarge)
    ));
    writing.join().unwrap();
    let (reader, writer) = std::io::pipe().unwrap();
    drop(writer);
    assert!(
        drain(reader, Instant::now() + READ_TIMEOUT)
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_superseded_read_cannot_publish_after_the_new_selection() {
    use std::io::Write as _;
    let (events, mut received) = mpsc::unbounded_channel();
    let (reader, mut first_writer) = std::io::pipe().unwrap();
    let mut first = read_entry(reader, "text/plain".to_owned(), events.clone());
    assert!(futures_util::poll!(first.as_mut()).is_pending());
    first_writer.write_all(b"A").unwrap();
    drop(first_writer);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), received.recv())
            .await
            .is_err()
    );
    drop(first);
    let (reader, mut second_writer) = std::io::pipe().unwrap();
    let second = read_entry(reader, "text/plain".to_owned(), events);
    second_writer.write_all(b"B").unwrap();
    drop(second_writer);
    second.await;
    assert!(
        matches!(received.recv().await, Some(Watched::Incoming(entry)) if &*entry.data == b"B")
    );
    assert!(received.recv().await.is_none());
}

#[tokio::test]
async fn dropping_the_receiver_during_a_read_does_not_panic() {
    use std::io::Write as _;
    let (events, received) = mpsc::unbounded_channel();
    let (reader, mut writer) = std::io::pipe().unwrap();
    let reading = read_entry(reader, "text/plain".to_owned(), events);
    writer.write_all(b"entry").unwrap();
    drop(writer);
    drop(received);
    reading.await;

    let (events, mut received) = mpsc::unbounded_channel();
    let (reader, writer) = std::io::pipe().unwrap();
    drop(writer);
    read_entry(reader, "text/plain".to_owned(), events).await;
    assert!(received.recv().await.is_none());
}

#[test]
fn paste_writes_obey_the_deadline_and_release_process_wide_slots() {
    use std::io::Read as _;
    let slots: Vec<_> = (0..MAX_CONCURRENT_SENDS)
        .map(|_| Sending::start().unwrap())
        .collect();
    assert!(Sending::start().is_none());
    let (reader, writer) = std::io::pipe().unwrap();
    let started = Instant::now();
    let result = write_before(
        writer.into(),
        &vec![0; 1024 * 1024],
        started + Duration::from_millis(50),
    );
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(1));
    drop(reader);
    drop(slots);
    let slot = Sending::start().unwrap();
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    assert!(write_before(writer.into(), b"closed", Instant::now() + READ_TIMEOUT).is_err());
    drop(slot);
    assert_eq!(SENDING.load(Ordering::Relaxed), 0);

    let (mut reader, writer) = std::io::pipe().unwrap();
    let retained = writer.try_clone().unwrap();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let target = std::thread::spawn(move || {
        wait(
            &reader,
            PollFlags::IN,
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap();
        fcntl_setfl(
            &retained,
            fcntl_getfl(&retained).unwrap() & !OFlags::NONBLOCK,
        )
        .unwrap();
        reader.read_exact(&mut [0; 4096]).unwrap();
        let _ = released.recv_timeout(Duration::from_secs(2));
    });
    let slot = Sending::start().unwrap();
    let started = Instant::now();
    let result = write_before(
        writer.into(),
        &vec![0; 1024 * 1024],
        started + Duration::from_millis(250),
    );
    drop(slot);
    let elapsed = started.elapsed();
    let _ = release.send(());
    target.join().unwrap();
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
    assert!(elapsed < Duration::from_secs(1));
    assert_eq!(SENDING.load(Ordering::Relaxed), 0);
    assert_eq!(
        write_before(
            std::fs::File::open("/dev/null").unwrap().into(),
            b"bad",
            Instant::now() + READ_TIMEOUT
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn a_draining_paste_target_receives_the_full_entry() {
    use std::io::Read as _;
    let (mut reader, writer) = std::io::pipe().unwrap();
    let reading = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let bytes = vec![42; 1024 * 1024];
    write_before(writer.into(), &bytes, Instant::now() + READ_TIMEOUT).unwrap();
    assert_eq!(reading.join().unwrap(), bytes);
}

#[test]
fn mime_lists_cap_count_and_length_without_hiding_a_sensitive_hint() {
    let mut offered = Vec::new();
    keep_mime(&mut offered, "x".repeat(4096));
    assert!(offered.is_empty());
    for i in 0..10_000 {
        keep_mime(&mut offered, format!("application/x-{i}"));
    }
    assert_eq!(offered.len(), MAX_MIMES);
    keep_mime(&mut offered, "text/plain;charset=utf-8".to_owned());
    assert_eq!(verdict(&offered, true), None);
    offered[0] = "text/plain".to_owned();
    keep_mime(&mut offered, "x-kde-passwordManagerHint".to_owned());
    assert_eq!(offered.len(), MAX_MIMES);
    assert_eq!(verdict(&offered, true), None);
}

#[test]
fn a_mime_is_matched_without_regard_to_casing() {
    assert_eq!(
        choose(&mimes(&["TEXT/PLAIN"])).as_deref(),
        Some("text/plain")
    );
}
