use super::*;

struct CompetingReader {
    reader: io::PipeReader,
    thief: std::fs::File,
}

impl AsFd for CompetingReader {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.reader.as_fd()
    }
}

impl Read for CompetingReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.thief.read_exact(&mut [0])?;
        self.reader.read(buffer)
    }
}

#[test]
fn a_competing_reader_cannot_block_the_snapshot_past_its_deadline() {
    use std::io::Write as _;
    use std::os::fd::AsRawFd as _;

    let (reader, mut writer) = io::pipe().unwrap();
    let thief = std::fs::File::open(format!("/proc/self/fd/{}", writer.as_raw_fd())).unwrap();
    writer.write_all(b"x").unwrap();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let rescue = std::thread::spawn(move || {
        let _ = released.recv_timeout(Duration::from_secs(1));
        drop(writer);
    });
    let started = Instant::now();
    let result = drain(
        CompetingReader { reader, thief },
        started + Duration::from_millis(50),
    );
    let elapsed = started.elapsed();
    let _ = release.send(());
    rescue.join().unwrap();
    assert_eq!(result.unwrap_err(), SkipReason::TimedOut);
    assert!(elapsed < Duration::from_millis(500));
}

fn mimes(list: &[&str]) -> Vec<String> {
    list.iter().map(|mime| (*mime).to_owned()).collect()
}

#[test]
fn a_sensitive_selection_reads_nothing_and_skips_every_mime() {
    let offered = mimes(&["text/plain", "x-kde-passwordManagerHint", "UTF8_STRING"]);

    let snapshot = read_mimes(&offered, |_| {
        panic!("a sensitive selection must not be read")
    });

    assert!(snapshot.entries.is_empty());
    assert_eq!(
        snapshot.skipped,
        offered
            .iter()
            .map(|mime| (mime.clone(), SkipReason::Sensitive))
            .collect::<Vec<_>>()
    );
}

#[test]
fn an_ordinary_selection_reads_every_offered_mime() {
    let offered = mimes(&["text/plain", "image/png"]);

    let snapshot = read_mimes(&offered, |_| Ok(Arc::from(&b"entry"[..])));

    assert_eq!(
        snapshot
            .entries
            .iter()
            .map(|entry| &entry.mime)
            .collect::<Vec<_>>(),
        offered.iter().collect::<Vec<_>>()
    );
    assert!(snapshot.skipped.is_empty());
}

#[test]
fn neither_content_reaches_a_debug_line() {
    let entry = Entry {
        mime: "text/plain".to_owned(),
        data: Arc::from(&b"hunter2"[..]),
    };

    let rendered = format!("{entry:?}");

    assert!(!rendered.contains("hunter2"), "got {rendered}");
    assert!(rendered.contains("bytes: 7"), "got {rendered}");
}

#[test]
fn snapshot_caps_the_total_and_stops_reading_after_an_overflow() {
    let offered = mimes(&["one", "two", "three"]);
    let mut reads = 0;
    let snapshot = read_mimes(&offered, |_| {
        reads += 1;
        Ok(Arc::from(vec![0; MAX_READ_BYTES * 3 / 4]))
    });
    assert_eq!(reads, 2);
    assert_eq!(snapshot.entries.len(), 1);
    assert_eq!(
        snapshot.skipped,
        vec![
            ("two".to_owned(), SkipReason::TooLarge),
            ("three".to_owned(), SkipReason::TooLarge)
        ]
    );
}

#[test]
fn failed_mimes_do_not_consume_the_snapshot_total_or_change_their_reason() {
    let offered = mimes(&["failed", "one", "two", "timed-out"]);
    let snapshot = read_mimes(&offered, |mime| match mime {
        "failed" => Err(SkipReason::Unreadable),
        "timed-out" => Err(SkipReason::TimedOut),
        _ => Ok(Arc::from(vec![0; MAX_READ_BYTES / 4])),
    });
    assert_eq!(snapshot.entries.len(), 2);
    assert_eq!(
        snapshot.skipped,
        vec![
            ("failed".to_owned(), SkipReason::Unreadable),
            ("timed-out".to_owned(), SkipReason::TimedOut)
        ]
    );
}
