use std::sync::Arc;
use std::time::Duration;

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::broadcast,
};

use glimpse_core::ipc::client::{IpcClientHandler, NoopCommandHandler};
use glimpse_core::ipc::protocol::IpcEvent;

// ── helpers ───────────────────────────────────────────────────────────────────

struct TestConn {
    event_tx: broadcast::Sender<Arc<IpcEvent>>,
    lines: tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    writer: tokio::net::unix::OwnedWriteHalf,
    _task: tokio::task::JoinHandle<()>,
}

impl TestConn {
    async fn new() -> Self {
        let (event_tx, _) = broadcast::channel::<Arc<IpcEvent>>(64);
        let (client, server) = UnixStream::pair().unwrap();
        let event_rx = event_tx.subscribe();

        let task = tokio::spawn(async move {
            IpcClientHandler::new(server, event_rx, NoopCommandHandler).run().await;
        });

        let (reader, writer) = client.into_split();
        let mut lines = BufReader::new(reader).lines();

        // consume hello
        let hello = tokio::time::timeout(Duration::from_secs(1), lines.next_line())
            .await
            .expect("hello timed out")
            .expect("io error on hello")
            .expect("EOF before hello");
        assert!(hello.starts_with("hello version="), "unexpected greeting: {hello}");

        TestConn { event_tx, lines, writer, _task: task }
    }

    async fn send(&mut self, line: &str) {
        self.writer.write_all(format!("{line}\n").as_bytes()).await.unwrap();
    }

    async fn subscribe(&mut self, patterns: &str) {
        self.send(&format!("subscribe {patterns}")).await;
        self.sync().await;
    }

    async fn unsubscribe(&mut self, patterns: &str) {
        self.send(&format!("unsubscribe {patterns}")).await;
        self.sync().await;
    }

    async fn sync(&mut self) {
        self.send("__sync__").await;
        let line = tokio::time::timeout(Duration::from_secs(1), self.lines.next_line())
            .await
            .expect("sync ack timed out")
            .expect("sync io error")
            .expect("EOF waiting for sync ack");
        assert!(line.contains("ack ok=false"), "expected sync ack, got: {line}");
    }

    async fn recv(&mut self) -> Option<String> {
        tokio::time::timeout(Duration::from_millis(400), self.lines.next_line())
            .await
            .ok()
            .and_then(|r| r.ok())
            .flatten()
    }

    async fn expect(&mut self, contains: &str) -> String {
        let line = self.recv().await.unwrap_or_else(|| {
            panic!("expected line containing '{contains}', got nothing")
        });
        assert!(line.contains(contains), "expected '{contains}' in '{line}'");
        line
    }

    async fn expect_none(&mut self) {
        let got = self.recv().await;
        assert!(got.is_none(), "expected no more lines, got: {got:?}");
    }

    fn emit(&self, name: &str, fields: Vec<(&str, &str)>) {
        let owned: Vec<(String, String)> =
            fields.into_iter().map(|(k, v)| (k.to_owned(), v.to_owned())).collect();
        let _ = self.event_tx.send(Arc::new(IpcEvent::new(name, owned)));
    }
}

// ── tests ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn hello_on_connect() {
    let _c = TestConn::new().await;
}

#[tokio::test]
async fn subscribe_wildcard_receives_event() {
    let mut c = TestConn::new().await;
    c.subscribe("*").await;

    c.emit("audio.volume_changed", vec![("volume", "75")]);

    let line = c.expect("audio.volume_changed").await;
    assert!(line.contains("volume=75"), "{line}");
    assert!(line.contains("ts="), "{line}");
}

#[tokio::test]
async fn subscribe_prefix_filters_correctly() {
    let mut c = TestConn::new().await;
    c.subscribe("bluetooth.*").await;

    c.emit("audio.volume_changed", vec![("volume", "50")]); // filtered
    c.emit("bluetooth.device_connected", vec![("address", "AA:BB:CC:DD:EE:FF")]);

    let line = c.expect("bluetooth.device_connected").await;
    assert!(line.contains("address=AA:BB:CC:DD:EE:FF"), "{line}");
    c.expect_none().await;
}

#[tokio::test]
async fn subscribe_exact_pattern() {
    let mut c = TestConn::new().await;
    c.subscribe("battery.critical").await;

    c.emit("battery.level_changed", vec![("percentage", "15")]); // filtered
    c.emit("battery.critical", vec![("percentage", "9")]);

    let line = c.expect("battery.critical").await;
    assert!(line.contains("percentage=9"), "{line}");
    c.expect_none().await;
}

#[tokio::test]
async fn multiple_subscriptions() {
    let mut c = TestConn::new().await;
    c.subscribe("audio.* battery.critical").await;

    c.emit("audio.muted", vec![]);
    c.emit("network.connected", vec![]); // filtered
    c.emit("battery.critical", vec![("percentage", "5")]);

    c.expect("audio.muted").await;
    c.expect("battery.critical").await;
    c.expect_none().await;
}

#[tokio::test]
async fn unsubscribe_stops_delivery() {
    let mut c = TestConn::new().await;
    c.subscribe("*").await;

    c.emit("audio.muted", vec![]);
    c.expect("audio.muted").await;

    c.unsubscribe("*").await;

    c.emit("audio.unmuted", vec![]);
    c.expect_none().await;
}

#[tokio::test]
async fn event_values_with_spaces_are_escaped() {
    let mut c = TestConn::new().await;
    c.subscribe("*").await;

    c.emit("compositor.window_focused", vec![("title", "Hello World")]);

    let line = c.expect("compositor.window_focused").await;
    assert!(line.contains("title=Hello\\sWorld"), "expected escaped value in: {line}");
}

#[tokio::test]
async fn unknown_command_ack_false() {
    let mut c = TestConn::new().await;
    c.send("frobnicate").await;
    c.expect("ack ok=false").await;
}

#[tokio::test]
async fn oversize_line_disconnects_client() {
    let mut c = TestConn::new().await;
    let big = format!("{}\n", "x".repeat(65 * 1024));
    let _ = c.writer.write_all(big.as_bytes()).await;

    let got = tokio::time::timeout(Duration::from_secs(1), c.lines.next_line())
        .await
        .expect("timed out waiting for disconnect")
        .expect("io error");
    assert!(got.is_none(), "expected EOF after oversize line, got: {got:?}");
}
