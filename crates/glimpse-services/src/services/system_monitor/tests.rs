use glimpse_dbus::Buses;
use tokio_util::sync::CancellationToken;

use super::*;

fn enabled_config() -> Config {
    Config {
        enabled: true,
        poll_interval: Duration::from_secs(2),
        disk_paths: vec![PathBuf::from("/")],
        gpu: true,
    }
}

async fn harness(
    config: Config,
) -> (
    SystemMonitor,
    Ctx<SystemMonitor>,
    tokio::sync::watch::Receiver<SystemMonitorState>,
) {
    let cancel = CancellationToken::new();
    let (events, _inbox) = tokio::sync::mpsc::channel(8);
    let (state, state_rx) = tokio::sync::watch::channel(SystemMonitorState::default());
    let (health, _health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
    let ctx = Ctx::<SystemMonitor>::new(
        events,
        &cancel,
        state,
        health,
        Buses::unavailable("no bus in tests"),
    );
    let service = SystemMonitor::start(&ctx, config, ()).await;
    (service, ctx, state_rx)
}

fn sampled(cpu_total: u64, cpu_idle: u64) -> Sampled {
    Sampled {
        cpu_total: Some(cpu_total),
        cpu_idle: Some(cpu_idle),
        ..Sampled::default()
    }
}

#[tokio::test]
async fn a_changed_poll_interval_produces_a_different_subscription_key() {
    let (mut service, ctx, _state) = harness(enabled_config()).await;
    let before: Vec<Watch> = service
        .subscriptions()
        .iter()
        .map(|sub| sub.key().clone())
        .collect();

    service
        .handle(
            &ctx,
            Input::Config(Config {
                poll_interval: Duration::from_secs(5),
                ..enabled_config()
            }),
        )
        .await;
    let after: Vec<Watch> = service
        .subscriptions()
        .iter()
        .map(|sub| sub.key().clone())
        .collect();

    assert_ne!(before, after);
}

#[tokio::test]
async fn the_first_sample_since_enabling_has_no_cpu_or_network_reading() {
    let (mut service, ctx, state) = harness(enabled_config()).await;

    service
        .handle(&ctx, Input::Event(Event::Sampled(sampled(1000, 500))))
        .await;

    assert_eq!(state.borrow().cpu, None);
    assert_eq!(state.borrow().network, None);
}

#[tokio::test]
async fn a_second_sample_produces_a_cpu_percentage_from_the_delta() {
    let (mut service, ctx, state) = harness(enabled_config()).await;

    service
        .handle(&ctx, Input::Event(Event::Sampled(sampled(1000, 800))))
        .await;
    service
        .handle(&ctx, Input::Event(Event::Sampled(sampled(1100, 850))))
        .await;

    let cpu = state.borrow().cpu.expect("a second sample has a delta");
    assert!(
        (cpu.percent - 50.0).abs() < 0.01,
        "50 of 100 new ticks were busy"
    );
}

#[tokio::test]
async fn no_swap_partition_publishes_no_swap_reading() {
    let (mut service, ctx, state) = harness(enabled_config()).await;

    service
        .handle(
            &ctx,
            Input::Event(Event::Sampled(Sampled {
                swap: None,
                ..sampled(1000, 500)
            })),
        )
        .await;

    assert_eq!(state.borrow().swap, None);
}

#[tokio::test]
async fn a_network_interface_that_disappears_does_not_panic_and_is_dropped_from_the_rate() {
    let (mut service, ctx, state) = harness(enabled_config()).await;

    service
        .handle(
            &ctx,
            Input::Event(Event::Sampled(Sampled {
                network: vec![
                    ("wlp99s0".to_owned(), 1000, 2000),
                    ("lo".to_owned(), 10, 10),
                ],
                ..sampled(1000, 500)
            })),
        )
        .await;
    service
        .handle(
            &ctx,
            Input::Event(Event::Sampled(Sampled {
                network: vec![("lo".to_owned(), 30, 30)],
                ..sampled(1100, 550)
            })),
        )
        .await;

    let network = state.borrow().network.expect("a second sample has a delta");
    assert!(
        network.rx_bytes_per_sec > 0.0,
        "lo's own delta must still be counted"
    );
}

#[tokio::test]
async fn disabling_publishes_the_default_state() {
    let (mut service, ctx, state) = harness(enabled_config()).await;

    service
        .handle(&ctx, Input::Event(Event::Sampled(sampled(1000, 500))))
        .await;
    service
        .handle(
            &ctx,
            Input::Config(Config {
                enabled: false,
                ..enabled_config()
            }),
        )
        .await;

    assert_eq!(*state.borrow(), SystemMonitorState::default());
}

#[tokio::test]
async fn a_sample_queued_before_a_disable_is_ignored_rather_than_reviving_stale_state() {
    let (mut service, ctx, state) = harness(enabled_config()).await;

    service
        .handle(
            &ctx,
            Input::Config(Config {
                enabled: false,
                ..enabled_config()
            }),
        )
        .await;
    service
        .handle(&ctx, Input::Event(Event::Sampled(sampled(1000, 500))))
        .await;

    assert_eq!(*state.borrow(), SystemMonitorState::default());
}

#[test]
fn a_document_placing_system_monitor_in_a_zone_computes_an_enabled_config() {
    let mut document = glimpse_config::Config {
        system_monitor: glimpse_config::SystemMonitor {
            poll_interval: 5,
            ..Default::default()
        },
        ..Default::default()
    };
    document.panels[0].right.push("system-monitor".to_owned());

    let config = Config::from(&document);

    assert!(config.enabled);
    assert_eq!(config.poll_interval, Duration::from_secs(5));
}

#[test]
fn a_document_with_no_placement_computes_a_disabled_config() {
    let config = Config::from(&glimpse_config::Config::default());

    assert!(!config.enabled);
}
