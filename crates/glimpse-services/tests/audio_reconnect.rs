use std::time::Duration;

use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_services::{Audio, Running, ServiceState};

fn cpu_seconds() -> f64 {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    let seconds = |time: libc::timeval| time.tv_sec as f64 + time.tv_usec as f64 / 1e6;
    seconds(usage.ru_utime) + seconds(usage.ru_stime)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_missing_pulse_server_does_not_spin_reconnecting() {
    let scratch = tempfile::tempdir().unwrap();
    unsafe {
        std::env::set_var(
            "PULSE_SERVER",
            scratch.path().join("native").to_str().unwrap(),
        );
    }
    let (_service, handle) = Running::<Audio>::spawn(
        &Config::default(),
        Buses::unavailable("no bus in tests"),
        (),
    );
    tokio::time::sleep(Duration::from_millis(300)).await;

    let before = cpu_seconds();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let spent = cpu_seconds() - before;

    assert!(
        spent < 0.2,
        "burned {spent:.3}s of CPU in 2s with no pulse server"
    );
    assert!(matches!(
        *handle.health().borrow(),
        ServiceState::Degraded { .. }
    ));
}
