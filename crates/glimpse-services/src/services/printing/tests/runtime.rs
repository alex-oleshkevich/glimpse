use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering},
};

use glimpse_dbus::testing::PrivateBus;
use ipp::attribute::IppAttribute;
use ipp::model::{DelimiterTag, IppVersion, StatusCode};
use ipp::request::IppRequestResponse;
use ipp::value::IppValue;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::*;

#[derive(Default)]
struct Counts {
    creates: AtomicUsize,
    renews: AtomicUsize,
    cancels: AtomicUsize,
    lists: AtomicUsize,
    status: AtomicU16,
    unavailable: AtomicBool,
    job: AtomicUsize,
}

async fn responder() -> (Config, Arc<Counts>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let config = config(Some(&listener.local_addr().expect("address").to_string()));
    let counts = Arc::new(Counts::default());
    let seen = counts.clone();
    let task = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.expect("connection");
            let counts = seen.clone();
            tokio::spawn(async move {
                let Some(body) = read_body(&mut stream).await else {
                    return;
                };
                assert!(body.len() >= 8, "short IPP body");
                let operation = u16::from_be_bytes([body[2], body[3]]);
                let request_id = i32::from_be_bytes([body[4], body[5], body[6], body[7]]);
                let reply = match operation {
                    0x0016 => {
                        let id = counts.creates.fetch_add(1, Ordering::SeqCst) as i32 + 105;
                        subscription_reply(request_id, 0, Some(id))
                    }
                    0x001a => {
                        counts.renews.fetch_add(1, Ordering::SeqCst);
                        subscription_reply(request_id, counts.status.load(Ordering::SeqCst), None)
                    }
                    0x001b => {
                        counts.cancels.fetch_add(1, Ordering::SeqCst);
                        subscription_reply(request_id, 0, None)
                    }
                    0x000a | 0x4002 => {
                        counts.lists.fetch_add(1, Ordering::SeqCst);
                        list_reply(operation, request_id, counts.job.load(Ordering::SeqCst))
                    }
                    _ => panic!("unexpected operation {operation:04x}"),
                };
                let status = if counts.unavailable.load(Ordering::SeqCst) {
                    "503 Unavailable"
                } else {
                    "200 OK"
                };
                let header = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    reply.len()
                );
                stream
                    .write_all(header.as_bytes())
                    .await
                    .expect("response header");
                stream.write_all(&reply).await.expect("response body");
            });
        }
    });
    (config, counts, task)
}

async fn read_body(stream: &mut TcpStream) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let header_end = loop {
        if let Some(split) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break split + 4;
        }
        if !read_some(stream, &mut bytes).await {
            return None;
        }
        assert!(bytes.len() < 65_536, "headers too large");
    };
    let header = std::str::from_utf8(&bytes[..header_end]).expect("headers");
    if let Some(length) = header_value(header, "content-length") {
        let length = length.parse::<usize>().expect("length");
        while bytes.len() < header_end + length {
            assert!(
                read_some(stream, &mut bytes).await,
                "request body ended early"
            );
        }
        return Some(bytes[header_end..header_end + length].to_vec());
    }
    if header_value(header, "transfer-encoding").is_some_and(|value| {
        value
            .split(',')
            .any(|part| part.trim().eq_ignore_ascii_case("chunked"))
    }) {
        return Some(decode_chunks(stream, &mut bytes, header_end).await);
    }
    panic!("neither content-length nor chunked:\n{header}");
}

async fn read_some(stream: &mut TcpStream, bytes: &mut Vec<u8>) -> bool {
    let mut block = [0; 4096];
    let read = stream.read(&mut block).await.expect("request");
    if read == 0 {
        return false;
    }
    bytes.extend_from_slice(&block[..read]);
    true
}

fn header_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    header.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

async fn decode_chunks(stream: &mut TcpStream, bytes: &mut Vec<u8>, mut cursor: usize) -> Vec<u8> {
    let mut body = Vec::new();
    loop {
        let line_end = loop {
            if let Some(offset) = bytes[cursor..].windows(2).position(|part| part == b"\r\n") {
                break cursor + offset;
            }
            assert!(read_some(stream, bytes).await, "chunk line ended");
            assert!(bytes.len() < 1 << 20, "request too large");
        };
        let line = std::str::from_utf8(&bytes[cursor..line_end]).expect("chunk size");
        let size_text = line.split(';').next().expect("chunk size").trim();
        let size = usize::from_str_radix(size_text, 16).expect("chunk size");
        cursor = line_end + 2;
        if size == 0 {
            break;
        }
        while bytes.len() < cursor + size + 2 {
            assert!(read_some(stream, bytes).await, "chunk data ended");
        }
        body.extend_from_slice(&bytes[cursor..cursor + size]);
        cursor += size + 2;
    }
    body
}

fn subscription_reply(request_id: i32, status: u16, id: Option<i32>) -> Vec<u8> {
    let mut reply = vec![2, 0];
    reply.extend_from_slice(&status.to_be_bytes());
    reply.extend_from_slice(&request_id.to_be_bytes());
    if let Some(id) = id {
        reply.push(6);
        reply.extend_from_slice(&[0x21, 0, 22]);
        reply.extend_from_slice(b"notify-subscription-id");
        reply.extend_from_slice(&[0, 4]);
        reply.extend_from_slice(&id.to_be_bytes());
    }
    reply.push(3);
    reply
}

fn list_reply(operation: u16, request_id: i32, job: usize) -> Vec<u8> {
    let mut response =
        IppRequestResponse::new_response(IppVersion::v2_0(), StatusCode::SuccessfulOk, request_id)
            .expect("ipp response");
    if operation == 0x4002 {
        response.attributes_mut().add(
            DelimiterTag::PrinterAttributes,
            IppAttribute::with_name(
                "printer-name",
                IppValue::NameWithoutLanguage("test-1".try_into().expect("printer name")),
            )
            .expect("printer attribute"),
        );
    } else if job > 0 {
        response.attributes_mut().add(
            DelimiterTag::JobAttributes,
            IppAttribute::with_name(
                "job-id",
                IppValue::Integer(i32::try_from(job).expect("job id")),
            )
            .expect("job attribute"),
        );
    }
    response.to_bytes().to_vec()
}

async fn until(mut condition: impl FnMut() -> bool) {
    time::timeout(Duration::from_secs(3), async {
        while !condition() {
            time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("condition converges");
}

#[tokio::test]
async fn runtime_renews_without_polling_recreates_gone_and_cancels_on_stop() {
    let bus = PrivateBus::start();
    let connection = bus.connection().await;
    let sender = bus.connection().await;
    let (config, counts, server) = responder().await;
    let cancel = CancellationToken::new();
    let (mut runtime, handle) =
        ServiceRuntime::<Printing>::new(config, Buses::for_system(connection), cancel.clone());
    let running = tokio::spawn(async move { runtime.run(()).await });
    until(|| counts.lists.load(Ordering::SeqCst) == 2).await;
    until(|| !handle.snapshot().printers.is_empty()).await;
    let mut health = handle.health();
    until(|| matches!(*health.borrow_and_update(), crate::ServiceState::Running)).await;
    time::pause();
    time::advance(Duration::from_secs(3600)).await;
    time::resume();
    until(|| counts.renews.load(Ordering::SeqCst) == 1).await;
    assert_eq!(counts.lists.load(Ordering::SeqCst), 2);
    counts.status.store(0x0500, Ordering::SeqCst);
    sender
        .emit_signal(
            None::<&str>,
            "/org/cups/cupsd/Notifier",
            "org.cups.cupsd.Notifier",
            "ServerRestarted",
            &(vec![255_u8; 64],),
        )
        .await
        .expect("malformed body is ignored");
    until(|| counts.renews.load(Ordering::SeqCst) == 2 && counts.lists.load(Ordering::SeqCst) == 4)
        .await;
    until(|| {
        matches!(
            *health.borrow_and_update(),
            crate::ServiceState::Degraded { .. }
        )
    })
    .await;
    assert_eq!(counts.creates.load(Ordering::SeqCst), 1);
    counts.status.store(0x0406, Ordering::SeqCst);
    sender
        .emit_signal(
            None::<&str>,
            "/org/cups/cupsd/Notifier",
            "org.cups.cupsd.Notifier",
            "ServerRestarted",
            &(),
        )
        .await
        .expect("restart");
    sender
        .emit_signal(
            None::<&str>,
            "/org/cups/cupsd/Notifier",
            "org.cups.cupsd.Notifier",
            "JobState",
            &(),
        )
        .await
        .expect("normal signal");
    until(|| {
        counts.creates.load(Ordering::SeqCst) == 2 && counts.lists.load(Ordering::SeqCst) == 6
    })
    .await;
    counts.status.store(0, Ordering::SeqCst);
    cancel.cancel();
    time::timeout(Duration::from_secs(2), running)
        .await
        .expect("bounded stop")
        .expect("runtime");
    assert_eq!(counts.cancels.load(Ordering::SeqCst), 1);
    server.abort();
}

#[tokio::test]
async fn successful_ipp_cannot_hide_an_unavailable_notifier() {
    let (config, counts, server) = responder().await;
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Printing>::new(
        config,
        Buses::unavailable("AddMatch unavailable"),
        cancel.clone(),
    );
    let running = tokio::spawn(async move { runtime.run(()).await });
    until(|| counts.lists.load(Ordering::SeqCst) == 2).await;
    assert!(matches!(
        *handle.health().borrow(),
        crate::ServiceState::Degraded { .. }
    ));
    cancel.cancel();
    running.await.expect("runtime");
    server.abort();
}

#[tokio::test]
#[ignore = "requires scripts/printing-test-cupsd.sh isolated CUPS and private system bus"]
async fn real_cups_subscription_tracks_held_released_and_cancelled_jobs() {
    let address = std::env::var("GLIMPSE_TEST_CUPS_BUS").expect("isolated bus address");
    let container = std::env::var("GLIMPSE_TEST_CUPS_CONTAINER").expect("isolated container");
    let connection = zbus::connection::Builder::address(address.as_str())
        .expect("address")
        .build()
        .await
        .expect("bus");
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Printing>::new(
        config(Some("127.0.0.1:8631")),
        Buses::for_system(connection),
        cancel.clone(),
    );
    let running = tokio::spawn(async move { runtime.run(()).await });
    until(|| {
        matches!(*handle.health().borrow(), crate::ServiceState::Running)
            && !handle.snapshot().printers.is_empty()
    })
    .await;
    for action in ["release", "cancel"] {
        let output = tokio::process::Command::new("docker")
            .args([
                "exec",
                &container,
                "lp",
                "-h",
                "127.0.0.1:8631",
                "-d",
                "test-1",
                "-H",
                "indefinite",
                "/etc/hostname",
            ])
            .output()
            .await
            .expect("held job");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).expect("lp output");
        let id: u32 = text
            .split_whitespace()
            .nth(3)
            .expect("job")
            .rsplit('-')
            .next()
            .expect("id")
            .parse()
            .expect("number");
        let start = Instant::now();
        time::timeout(
            Duration::from_secs(2),
            until(|| {
                handle
                    .snapshot()
                    .jobs
                    .iter()
                    .any(|job| job.id == id && job.state == JobState::Held)
            }),
        )
        .await
        .expect("held transition within two seconds");
        eprintln!("held {id} visible after {:?}", start.elapsed());
        let start = Instant::now();
        if action == "release" {
            handle.resume_job(id).await.expect("release");
        } else {
            handle.cancel_job(id).await.expect("cancel");
        }
        time::timeout(
            Duration::from_secs(2),
            until(|| {
                !handle
                    .snapshot()
                    .jobs
                    .iter()
                    .any(|job| job.id == id && job.state.is_active())
            }),
        )
        .await
        .expect("job leaves active list within two seconds");
        eprintln!("{action} {id} visible after {:?}", start.elapsed());
    }
    time::sleep(Duration::from_secs(2)).await;
    let log = std::env::var("GLIMPSE_TEST_CUPS_ACCESS_LOG").expect("isolated access log");
    let count = || {
        std::fs::read_to_string(&log)
            .expect("access log")
            .lines()
            .filter(|line| line.contains("Get-Jobs") || line.contains("CUPS-Get-Printers"))
            .count()
    };
    let before = count();
    assert!(before >= 2);
    time::sleep(Duration::from_secs(2)).await;
    assert_eq!(count(), before, "idle service does not fetch");
    cancel.cancel();
    time::timeout(Duration::from_secs(2), running)
        .await
        .expect("bounded stop")
        .expect("runtime");
}

#[tokio::test]
async fn failed_initial_cups_recovers_on_the_next_lease_tick() {
    let bus = PrivateBus::start();
    let (config, counts, server) = responder().await;
    counts.unavailable.store(true, Ordering::SeqCst);
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Printing>::new(
        config,
        Buses::for_system(bus.connection().await),
        cancel.clone(),
    );
    let running = tokio::spawn(async move { runtime.run(()).await });
    until(|| {
        matches!(
            *handle.health().borrow(),
            crate::ServiceState::Degraded { .. }
        )
    })
    .await;
    assert_eq!(counts.creates.load(Ordering::SeqCst), 1);
    assert_eq!(counts.lists.load(Ordering::SeqCst), 0);
    counts.unavailable.store(false, Ordering::SeqCst);
    time::pause();
    time::advance(RENEW_EVERY).await;
    time::resume();
    until(|| !handle.snapshot().printers.is_empty()).await;
    assert_eq!(counts.creates.load(Ordering::SeqCst), 2);
    assert_eq!(counts.lists.load(Ordering::SeqCst), 2);
    assert!(matches!(
        *handle.health().borrow(),
        crate::ServiceState::Running
    ));
    cancel.cancel();
    running.await.expect("runtime");
    server.abort();
}

#[tokio::test]
async fn private_bus_burst_has_one_trailing_fetch_and_converges_to_authoritative_state() {
    let bus = PrivateBus::start();
    let sender = bus.connection().await;
    let (config, counts, server) = responder().await;
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Printing>::new(
        config,
        Buses::for_system(bus.connection().await),
        cancel.clone(),
    );
    let running = tokio::spawn(async move { runtime.run(()).await });
    until(|| !handle.snapshot().printers.is_empty()).await;
    assert_eq!(counts.lists.load(Ordering::SeqCst), 2);
    let spinning = Arc::new(AtomicBool::new(true));
    let spin = spinning.clone();
    let clock_guard = tokio::spawn(async move {
        while spin.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    });
    time::pause();
    for id in 1..=100 {
        counts.job.store(id, Ordering::SeqCst);
        sender
            .emit_signal(
                None::<&str>,
                "/org/cups/cupsd/Notifier",
                "org.cups.cupsd.Notifier",
                "JobState",
                &(vec![255_u8],),
            )
            .await
            .expect("signal");
    }
    let proxy = zbus::fdo::DBusProxy::new(&sender).await.expect("bus proxy");
    proxy.get_id().await.expect("signal delivery barrier");
    for _ in 0..1000 {
        tokio::task::yield_now().await;
    }
    assert_eq!(counts.lists.load(Ordering::SeqCst), 2);
    time::advance(FETCH_FLOOR).await;
    time::resume();
    spinning.store(false, Ordering::SeqCst);
    clock_guard.await.expect("clock guard");
    until(|| handle.snapshot().jobs.iter().any(|job| job.id == 100)).await;
    time::sleep(FETCH_FLOOR + Duration::from_millis(100)).await;
    assert_eq!(counts.lists.load(Ordering::SeqCst), 4);
    cancel.cancel();
    running.await.expect("runtime");
    server.abort();
}

#[tokio::test]
async fn losing_the_notifier_bus_stays_degraded_after_a_successful_refresh() {
    let mut bus = PrivateBus::start();
    let (config, counts, server) = responder().await;
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Printing>::new(
        config,
        Buses::for_system(bus.connection().await),
        cancel.clone(),
    );
    let running = tokio::spawn(async move { runtime.run(()).await });
    until(|| !handle.snapshot().printers.is_empty()).await;
    bus.kill();
    until(|| {
        matches!(
            *handle.health().borrow(),
            crate::ServiceState::Degraded { .. }
        )
    })
    .await;
    handle
        .refresh()
        .await
        .expect("authoritative HTTP still works");
    assert!(counts.lists.load(Ordering::SeqCst) >= 4);
    assert!(matches!(
        *handle.health().borrow(),
        crate::ServiceState::Degraded { .. }
    ));
    cancel.cancel();
    running.await.expect("runtime");
    server.abort();
}
