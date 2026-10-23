use tokio_util::sync::CancellationToken;

use super::*;
use crate::service::ServiceRuntime;
use glimpse_dbus::Buses;

fn config(value: Option<&str>) -> Config {
    Config {
        server: normalize_server(value),
    }
}

async fn printing() -> (
    Printing,
    Ctx<Printing>,
    tokio::sync::mpsc::Receiver<Input<Printing>>,
) {
    let cancel = CancellationToken::new();
    let (events, inbox) = tokio::sync::mpsc::channel(256);
    let (state, _) = tokio::sync::watch::channel(PrintingState::default());
    let (health, _) = tokio::sync::watch::channel(crate::ServiceState::Starting);
    let ctx = Ctx::new(
        events,
        &cancel,
        state,
        health,
        Buses::unavailable("test bus unavailable"),
    );
    let service = Printing::start(&ctx, config(Some("127.0.0.1:1")), ()).await;
    (service, ctx, inbox)
}

#[test]
fn server_normalization_preserves_transport_path_and_port() {
    for (input, expected) in [
        (None, "http://localhost:631/"),
        (Some("printserver"), "http://printserver:631/"),
        (Some("printserver:8631"), "http://printserver:8631/"),
        (
            Some("ipp://printserver:8631/cups"),
            "http://printserver:8631/cups",
        ),
        (
            Some("ipps://printserver:8631/cups"),
            "https://printserver:8631/cups",
        ),
        (
            Some("https://printserver:8631/cups"),
            "https://printserver:8631/cups",
        ),
        (
            Some("http://printserver:80/cups"),
            "http://printserver:80/cups",
        ),
        (
            Some("https://printserver:443/cups"),
            "https://printserver:443/cups",
        ),
        (Some("https://printserver/cups"), "https://printserver/cups"),
        (Some("[::1]"), "http://[::1]:631/"),
        (Some("[::1]:8631"), "http://[::1]:8631/"),
        (Some("http://[::1]:631/"), "http://[::1]:631/"),
        (Some("http://[::1]:80/"), "http://[::1]:80/"),
        (Some("ipp://[::1]/cups"), "http://[::1]:631/cups"),
    ] {
        assert_eq!(
            normalize_server(input).expect("valid server").to_string(),
            expected
        );
    }
    for input in [
        "",
        "/run/cups/cups.sock",
        "ftp://host/",
        "host/path",
        "bad server",
        "http://user:password@host/",
    ] {
        assert!(normalize_server(Some(input)).is_err(), "{input}");
    }
}

#[tokio::test]
async fn initial_lease_waits_for_notifier_registration_to_settle() {
    let (mut service, ctx, mut inbox) = printing().await;
    service.handle(&ctx, Input::Event(Event::LeaseDue(0))).await;
    service.fetch(&ctx);
    assert!(!service.leasing && !service.fetching);
    assert!(inbox.try_recv().is_err());
    service
        .handle(
            &ctx,
            Input::Event(Event::NotifierReady(0, Err("AddMatch refused".to_owned()))),
        )
        .await;
    assert!(!service.leasing);
    service.handle(&ctx, Input::Event(Event::LeaseDue(0))).await;
    assert!(service.leasing);
    let event = time::timeout(Duration::from_secs(2), inbox.recv())
        .await
        .expect("bounded lease")
        .expect("event");
    service.handle(&ctx, event).await;
    assert_eq!(service.notifier_error.as_deref(), Some("AddMatch refused"));
    assert!(service.lease_error.is_some());
    ctx.shutdown().await;
}

#[tokio::test]
async fn successful_fetch_keeps_independent_subscription_and_notifier_errors() {
    let (mut service, ctx, _) = printing().await;
    service.notifier_error = Some("notifier failed".to_owned());
    service.lease_error = Some("renew failed".to_owned());
    service
        .handle(
            &ctx,
            Input::Event(Event::Fetched(0, Ok(PrintingState::default()))),
        )
        .await;
    assert!(ctx.is_degraded());
    assert_eq!(service.lease_error.as_deref(), Some("renew failed"));
    service.notifier_error = None;
    service.health(&ctx);
    assert!(ctx.is_degraded());
}

#[tokio::test]
async fn a_signal_inside_the_floor_keeps_one_trailing_fetch() {
    let (mut service, ctx, mut inbox) = printing().await;
    service.notifier_ready = true;
    service.next_fetch = Instant::now() + Duration::from_millis(100);
    for _ in 0..100 {
        service
            .handle(&ctx, Input::Event(Event::Signal(0, false)))
            .await;
    }
    assert!(service.fetching);
    assert!(inbox.try_recv().is_err());
    let started = time::timeout(Duration::from_secs(1), inbox.recv())
        .await
        .expect("trailing fetch starts")
        .expect("event");
    assert!(matches!(started, Input::Event(Event::FetchStarted(0))));
    service.handle(&ctx, started).await;
    assert!(!service.fetch_pending);
    let done = time::timeout(Duration::from_secs(1), inbox.recv())
        .await
        .expect("bounded fetch")
        .expect("event");
    service.handle(&ctx, done).await;
    assert!(!service.fetching);
    assert!(inbox.try_recv().is_err());
    ctx.shutdown().await;
}

#[tokio::test]
async fn a_lease_tick_during_an_in_flight_lease_is_retried_when_that_lease_ends() {
    let (mut service, ctx, mut inbox) = printing().await;
    service.notifier_ready = true;
    service.leasing = true;
    service.subscription = Some(105);
    service.handle(&ctx, Input::Event(Event::LeaseDue(0))).await;
    assert!(service.lease_pending);
    assert!(service.leasing);
    assert_eq!(service.subscription, Some(105));
    service
        .handle(
            &ctx,
            Input::Event(Event::Leased(
                0,
                LeaseResult {
                    id: Some(105),
                    created: false,
                    error: Some("renew failed".to_owned()),
                },
            )),
        )
        .await;
    assert!(service.leasing);
    assert!(!service.lease_pending);
    let event = time::timeout(Duration::from_secs(2), inbox.recv())
        .await
        .expect("retry reaches the server")
        .expect("event");
    assert!(matches!(event, Input::Event(Event::Leased(0, _))));
    ctx.shutdown().await;
}

#[tokio::test]
async fn restart_requests_are_coalesced_while_the_current_lease_is_running() {
    let (mut service, ctx, _) = printing().await;
    service.notifier_ready = true;
    service.leasing = true;
    service.subscription = Some(105);
    service
        .handle(&ctx, Input::Event(Event::Signal(0, true)))
        .await;
    service
        .handle(&ctx, Input::Event(Event::Signal(0, false)))
        .await;
    assert!(service.lease_pending && service.fetch_pending);
    assert_eq!(service.subscription, Some(105));
}

#[tokio::test]
async fn late_completions_cannot_overwrite_the_new_generation() {
    let (mut service, ctx, _) = printing().await;
    service.generation = 1;
    service
        .handle(
            &ctx,
            Input::Event(Event::Leased(
                0,
                LeaseResult {
                    id: Some(105),
                    created: true,
                    error: None,
                },
            )),
        )
        .await;
    assert_eq!(service.subscription, None);
    assert!(!service.fetching);
}

#[tokio::test]
async fn a_job_action_against_an_unreachable_cups_resolves_a_typed_failure() {
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Printing>::new(
        config(Some("127.0.0.1:1")),
        Buses::unavailable("test"),
        cancel.clone(),
    );
    let running = tokio::spawn(async move { runtime.run(()).await });
    let outcome = handle.cancel_job(1).await;
    cancel.cancel();
    running.await.expect("runtime");
    assert!(matches!(
        outcome,
        Err(PrintingError::Failed(Failure::Unreachable))
    ));
}

#[tokio::test]
async fn refresh_resolves_even_when_the_authoritative_fetch_fails() {
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Printing>::new(
        config(Some("127.0.0.1:1")),
        Buses::unavailable("test"),
        cancel.clone(),
    );
    assert_eq!(handle.snapshot(), PrintingState::default());
    let running = tokio::spawn(async move { runtime.run(()).await });
    assert!(
        time::timeout(Duration::from_secs(2), handle.refresh())
            .await
            .expect("refresh resolves")
            .is_ok()
    );
    cancel.cancel();
    running.await.expect("runtime");
    assert!(matches!(
        handle.cancel_job(1).await,
        Err(PrintingError::Service(CommandError::Unavailable(_)))
    ));
}

mod runtime;

#[tokio::test]
async fn refresh_during_a_fetch_waits_for_its_own_trailing_snapshot() {
    let (mut service, ctx, _) = printing().await;
    service.notifier_ready = true;
    service.fetching = true;
    let (first, mut first_result) = oneshot::channel();
    service.fetch_replies.push(first);
    let (second, mut second_result) = oneshot::channel();
    service.refresh(&ctx, second);
    service
        .handle(
            &ctx,
            Input::Event(Event::Fetched(0, Ok(PrintingState::default()))),
        )
        .await;
    assert!(first_result.try_recv().expect("first reply").is_ok());
    assert!(matches!(
        second_result.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    service
        .handle(
            &ctx,
            Input::Event(Event::Fetched(0, Ok(PrintingState::default()))),
        )
        .await;
    assert!(second_result.try_recv().expect("second reply").is_ok());
    ctx.shutdown().await;
}
