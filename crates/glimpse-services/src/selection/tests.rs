use super::*;

#[tokio::test]
async fn every_subscription_gets_its_own_live_stream() {
    use futures_util::StreamExt as _;

    let selection = FakeSelection::default();
    let mut first = selection.events();
    selection.capture("text/plain", b"one");
    assert!(matches!(
        first.next().await,
        Some(SelectionEvent::Captured(_))
    ));

    drop(first);
    let mut second = selection.events();
    selection.capture("text/plain", b"two");

    assert!(
        matches!(second.next().await, Some(SelectionEvent::Captured(_))),
        "a rebuilt source must still receive captures"
    );
    assert_eq!(selection.subscriptions(), 2);
}

#[test]
fn neither_content_nor_preview_reaches_a_debug_line() {
    let capture = Capture {
        mime: "text/plain".to_owned(),
        data: Arc::from(&b"hunter2"[..]),
    };

    let rendered = format!("{capture:?}");

    assert!(!rendered.contains("hunter2"), "got {rendered}");
    assert!(rendered.contains("bytes: 7"), "got {rendered}");
}
