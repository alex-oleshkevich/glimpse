use std::io::Cursor;

use tokio::sync::oneshot;

use super::*;

fn request(header: &HoldHeader, body: &[u8]) -> Cursor<Vec<u8>> {
    let mut bytes = serde_json::to_vec(header).expect("json");
    bytes.push(b'\n');
    bytes.extend_from_slice(body);
    Cursor::new(bytes)
}

fn png(len: u64) -> HoldHeader {
    HoldHeader {
        version: 1,
        duties: Duties {
            clipboard: true,
            notify: None,
        },
        entries: vec![EntryHeader {
            mime: "image/png".to_owned(),
            len,
        }],
    }
}

#[test]
fn a_request_is_a_header_line_then_each_entry_in_order() {
    let entries = vec![
        Entry {
            mime: "text/plain".to_owned(),
            data: Arc::from(&b"hello"[..]),
        },
        Entry {
            mime: "image/png".to_owned(),
            data: Arc::from(&b"\x89PNG"[..]),
        },
    ];
    let header = HoldHeader::clipboard(&entries);

    let (read, got) =
        read_request(&mut request(&header, b"hello\x89PNG")).expect("a valid request");

    assert_eq!(read, header);
    assert_eq!(got, entries);
}

#[test]
fn a_request_is_refused_when_it_lies_or_asks_too_much() {
    assert!(read_request(&mut request(&png(10), b"short")).is_err());
    let greedy = read_request(&mut request(&png(SIZE_CAP + 1), b"")).expect_err("over the cap");
    assert!(format!("{greedy:#}").contains("more than"), "{greedy:#}");
    let mut split = png(SIZE_CAP / 2 + 1);
    split.entries.push(split.entries[0].clone());
    let split = read_request(&mut request(&split, b"")).expect_err("over the total");
    assert!(format!("{split:#}").contains("more than"), "{split:#}");
    let mut many = png(0);
    many.entries = vec![many.entries[0].clone(); MAX_ENTRIES + 1];
    let many = read_request(&mut request(&many, b"")).expect_err("too many entries");
    assert!(format!("{many:#}").contains("entries"), "{many:#}");
    let mut endless = vec![b' '; HEADER_CAP as usize + 10];
    endless.push(b'\n');
    let endless = read_request(&mut Cursor::new(endless)).expect_err("no end of header");
    assert!(format!("{endless:#}").contains("within"), "{endless:#}");
    let mut newer = png(0);
    newer.version = 2;
    assert!(read_request(&mut request(&newer, b"")).is_err());
    assert!(read_request(&mut Cursor::new(b"not json\n".to_vec())).is_err());
}

#[test]
fn a_reply_descriptor_that_is_not_open_is_refused() {
    use std::os::fd::IntoRawFd as _;

    assert!(reply_pipe(4000).is_err());
    let (_reader, writer) = io::pipe().expect("a pipe");
    assert!(reply_pipe(OwnedFd::from(writer).into_raw_fd()).is_ok());
}

#[test]
fn a_reply_reads_back_as_itself() {
    for reply in [
        HoldReply::Ok,
        HoldReply::Error("no data-control".to_owned()),
    ] {
        assert_eq!(HoldReply::parse(&reply.line()), reply);
    }
    assert!(matches!(HoldReply::parse(""), HoldReply::Error(_)));
    assert!(matches!(HoldReply::parse("okay\n"), HoldReply::Error(_)));
}

#[tokio::test]
async fn hold_stays_until_the_clipboard_duty_ends_after_the_notification_closed() {
    let (clipboard_lost, clipboard) = oneshot::channel::<()>();
    let (notification_closed, notification) = oneshot::channel::<()>();
    let held = tokio::spawn(until_done(
        Some(async {
            let _ = clipboard.await;
        }),
        Some(async {
            let _ = notification.await;
        }),
    ));

    notification_closed.send(()).expect("sent");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!held.is_finished());

    clipboard_lost.send(()).expect("sent");
    tokio::time::timeout(Duration::from_secs(1), held)
        .await
        .expect("ends once every duty is done")
        .expect("joins");
}

#[tokio::test]
async fn a_hold_with_no_duty_left_ends_at_once() {
    tokio::time::timeout(
        Duration::from_secs(1),
        until_done(None::<std::future::Ready<()>>, Some(std::future::ready(()))),
    )
    .await
    .expect("ends");
}
