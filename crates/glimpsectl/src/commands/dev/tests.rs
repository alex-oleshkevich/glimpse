use super::*;
use glimpse_dbus::{own_name, testing::PrivateBus};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct Manager {
    names: Vec<String>,
    stopped: Arc<Mutex<Vec<String>>>,
    subscribed: AtomicBool,
    completion_burst: Option<usize>,
}

#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    fn subscribe(&self) {
        self.subscribed.store(true, Ordering::SeqCst);
    }

    fn list_units_by_patterns(
        &self,
        _states: Vec<String>,
        _patterns: Vec<String>,
    ) -> Vec<glimpse_dbus::systemd1::Systemd1UnitEntry> {
        self.names
            .iter()
            .map(|name| {
                (
                    name.clone(),
                    String::new(),
                    "loaded".to_owned(),
                    "active".to_owned(),
                    "running".to_owned(),
                    String::new(),
                    "/test".try_into().unwrap(),
                    0,
                    String::new(),
                    "/".try_into().unwrap(),
                )
            })
            .collect()
    }

    async fn stop_unit(
        &self,
        name: &str,
        mode: &str,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> zbus::zvariant::OwnedObjectPath {
        assert!(self.subscribed.load(Ordering::SeqCst));
        assert_eq!(mode, "replace");
        self.stopped.lock().unwrap().push(name.to_owned());
        if let Some(burst) = self.completion_burst {
            let first = self.stopped.lock().unwrap().len() == 1;
            for index in 0..if first { burst } else { 0 } {
                connection
                    .emit_signal(
                        None::<&str>,
                        "/org/freedesktop/systemd1",
                        "org.freedesktop.systemd1.Manager",
                        "JobRemoved",
                        &(
                            index as u32,
                            zbus::zvariant::OwnedObjectPath::try_from("/test/job").unwrap(),
                            "unrelated.service",
                            "failed",
                        ),
                    )
                    .await
                    .unwrap();
            }
            connection
                .emit_signal(
                    None::<&str>,
                    "/org/freedesktop/systemd1",
                    "org.freedesktop.systemd1.Manager",
                    "JobRemoved",
                    &(
                        0u32,
                        zbus::zvariant::OwnedObjectPath::try_from("/test/other_job").unwrap(),
                        name,
                        "failed",
                    ),
                )
                .await
                .unwrap();
            connection
                .emit_signal(
                    None::<&str>,
                    "/org/freedesktop/systemd1",
                    "org.freedesktop.systemd1.Manager",
                    "JobRemoved",
                    &(
                        1u32,
                        zbus::zvariant::OwnedObjectPath::try_from("/test/job").unwrap(),
                        name,
                        "done",
                    ),
                )
                .await
                .unwrap();
        }
        "/test/job".try_into().unwrap()
    }
}

#[tokio::test]
async fn cleanup_stops_only_exact_panel_and_child_pid_units() {
    let bus = PrivateBus::start();
    let connection = bus.connection().await;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("scratch");
    fs::create_dir(&path).unwrap();
    let child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
    let pid = child.id();
    let matching =
        format!("app-glimpse-org.example.Test\\x2dPanel-org.example.App-2-{pid}-3.service");
    let names = vec![
        matching.clone(),
        format!(
            "app-glimpse-org.example.Test\\x2dPanel-org.example.App-2-{}-3.service",
            pid + 1
        ),
        format!("app-glimpse-org.example.Test\\x2dPanelExtra-org.example.App-2-{pid}-3.service"),
        format!("app-glimpse-org.example.Other-org.example.App-2-{pid}-3.service"),
        format!("app-glimpse-org.example.Test\\x2dPanel-org.example.App-2-{pid}-bad.service"),
    ];
    let stopped = Arc::new(Mutex::new(Vec::new()));
    connection
        .object_server()
        .at(
            "/org/freedesktop/systemd1",
            Manager {
                names,
                stopped: stopped.clone(),
                subscribed: AtomicBool::new(false),
                completion_burst: None,
            },
        )
        .await
        .unwrap();
    own_name(&connection, "org.freedesktop.systemd1")
        .await
        .unwrap();
    let mut panel = OwnedPanel {
        child,
        bus: connection.clone(),
        app_id: "org.example.Test-Panel".to_owned(),
        scratch: Scratch {
            path: path.clone(),
            remove_on_drop: false,
        },
    };
    let shutdown = tokio::spawn(async move {
        let result = panel.shutdown().await;
        assert!(panel.child.try_wait().unwrap().is_some());
        result
    });
    tokio::time::timeout(Duration::from_secs(3), async {
        while stopped.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(!shutdown.is_finished());
    assert!(path.exists());
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!shutdown.is_finished());
    connection
        .emit_signal(
            None::<&str>,
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
            "JobRemoved",
            &(
                1u32,
                zbus::zvariant::OwnedObjectPath::try_from("/test/job").unwrap(),
                matching.as_str(),
                "done",
            ),
        )
        .await
        .unwrap();
    shutdown.await.unwrap().unwrap();
    assert_eq!(*stopped.lock().unwrap(), vec![matching]);
    assert!(!path.exists());
}

#[tokio::test]
async fn cleanup_reports_unfinished_stop_jobs_as_timeout() {
    let bus = PrivateBus::start();
    let connection = bus.connection().await;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("scratch");
    fs::create_dir(&path).unwrap();
    let child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
    let matching = format!(
        "app-glimpse-org.example.Test-org.example.App-2-{}-3.service",
        child.id()
    );
    let stopped = Arc::new(Mutex::new(Vec::new()));
    connection
        .object_server()
        .at(
            "/org/freedesktop/systemd1",
            Manager {
                names: vec![matching],
                stopped: stopped.clone(),
                subscribed: AtomicBool::new(false),
                completion_burst: None,
            },
        )
        .await
        .unwrap();
    own_name(&connection, "org.freedesktop.systemd1")
        .await
        .unwrap();
    let mut panel = OwnedPanel {
        child,
        bus: connection,
        app_id: "org.example.Test".to_owned(),
        scratch: Scratch {
            path: path.clone(),
            remove_on_drop: false,
        },
    };
    let shutdown = tokio::spawn(async move { panel.shutdown().await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while stopped.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(!shutdown.is_finished());
    assert!(path.exists());
    tokio::time::pause();
    tokio::time::advance(crate::errors::DEADLINE + Duration::from_secs(1)).await;
    let error = shutdown.await.unwrap().unwrap_err();
    assert!(error.to_string().contains("cleanup timed out"));
    assert_eq!(crate::errors::exit(&error), crate::errors::Exit::Timeout);
    assert!(
        path.exists(),
        "unfinished stop jobs retain the panel config"
    );
}

#[tokio::test]
async fn cleanup_drains_immediate_completions_and_unrelated_bursts_before_stop_replies() {
    for burst in [0, 80] {
        let bus = PrivateBus::start();
        let connection = bus.connection().await;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("scratch");
        fs::create_dir(&path).unwrap();
        let child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        let names: Vec<_> = (0..80)
            .map(|index| {
                format!(
                    "app-glimpse-org.example.Test-org.example.App-{index}-{}-3.service",
                    child.id()
                )
            })
            .collect();
        let stopped = Arc::new(Mutex::new(Vec::new()));
        connection
            .object_server()
            .at(
                "/org/freedesktop/systemd1",
                Manager {
                    names: names.clone(),
                    stopped: stopped.clone(),
                    subscribed: AtomicBool::new(false),
                    completion_burst: Some(burst),
                },
            )
            .await
            .unwrap();
        own_name(&connection, "org.freedesktop.systemd1")
            .await
            .unwrap();
        let mut panel = OwnedPanel {
            child,
            bus: connection,
            app_id: "org.example.Test".to_owned(),
            scratch: Scratch {
                path,
                remove_on_drop: false,
            },
        };
        tokio::time::timeout(Duration::from_secs(3), panel.shutdown())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(*stopped.lock().unwrap(), names);
    }
}
