use super::*;

#[test]
fn an_unavailable_bus_reports_why() {
    let buses = Buses::unavailable("no bus in tests");
    assert_eq!(buses.session_bus().unwrap_err(), "no bus in tests");
    assert_eq!(buses.system_bus().unwrap_err(), "no bus in tests");
}

struct Sentinel(tokio::sync::mpsc::UnboundedSender<()>);

impl Drop for Sentinel {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

fn pending_with_sentinel() -> (
    impl futures_util::Stream<Item = u32> + Send,
    tokio::sync::mpsc::UnboundedReceiver<()>,
) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let sentinel = Sentinel(tx);
    let source = futures_util::stream::poll_fn(move |_| {
        let _ = &sentinel;
        std::task::Poll::Pending
    });
    (source, rx)
}

#[tokio::test]
async fn events_keep_the_newest_and_report_what_was_missed() {
    let polled = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let counter = polled.clone();
    let source = futures_util::stream::iter(0..1000u32).inspect(move |_| {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    });
    let mut out = events(source);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(polled.load(std::sync::atomic::Ordering::SeqCst), 1000);
    assert_eq!(out.next().await, Some(Delivery::Missed));
    let mut rest = Vec::new();
    while let Some(Delivery::Item(n)) = out.next().await {
        rest.push(n);
    }
    assert_eq!(rest.len(), 64);
    assert_eq!(rest.last(), Some(&999));
}

#[tokio::test]
async fn a_wake_flood_collapses_to_at_most_two() {
    let mut out = wake(futures_util::stream::iter(0..1000u32));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let mut wakes = 0;
    while out.next().await.is_some() {
        wakes += 1;
    }
    assert!((1..=2).contains(&wakes), "{wakes}");
}

#[tokio::test]
async fn dropping_the_events_consumer_ends_the_forwarder() {
    let (source, mut gone) = pending_with_sentinel();
    let out = events(source);
    tokio::task::yield_now().await;
    drop(out);
    tokio::time::timeout(std::time::Duration::from_secs(2), gone.recv())
        .await
        .unwrap();
}

#[tokio::test]
async fn dropping_the_wake_consumer_ends_the_forwarder() {
    let (source, mut gone) = pending_with_sentinel();
    let out = wake(source);
    tokio::task::yield_now().await;
    drop(out);
    tokio::time::timeout(std::time::Duration::from_secs(2), gone.recv())
        .await
        .unwrap();
}
