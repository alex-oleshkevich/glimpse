use super::*;
use crate::services::PanelServices;
use glimpse_dbus::Buses;
use glimpse_dbus::applet_host::AppletHostProxy;
use glimpse_dbus::testing::PrivateBus;
use std::fs;

fn desktop() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("me.x.A.desktop");
    fs::write(
        &path,
        "[Desktop Entry]\nType=Application\nName=Test\nExec=/bin/true\nImplements=me.aresa.Glimpse.Applet1;\n",
    )
    .unwrap();
    (dir, path)
}

#[test]
fn validation_and_caps_reject_bad_attachments() {
    let (_dir, path) = desktop();
    let path = path.to_str().unwrap();
    let layout = vec![vec!["DP-1".to_owned()]];
    let valid = || validate(path, 0, "", "", "{}", &layout).unwrap();
    assert_eq!(valid().output, "");
    assert!(validate(path, 99, "", "", "{}", &layout).is_err());
    assert!(validate(path, 0, "missing", "", "{}", &layout).is_err());
    assert!(validate(path, 0, "", "middle", "{}", &layout).is_err());
    assert!(validate("relative.desktop", 0, "", "", "{}", &layout).is_err());
    assert!(validate(&path.replace(".desktop", ".txt"), 0, "", "", "{}", &layout).is_err());
    let bad_stem = std::path::Path::new(path).with_file_name("bad id.desktop");
    fs::copy(path, &bad_stem).unwrap();
    assert!(validate(bad_stem.to_str().unwrap(), 0, "", "", "{}", &layout).is_err());
    let link = std::path::Path::new(path).with_file_name("me.x.Link.desktop");
    std::os::unix::fs::symlink(path, &link).unwrap();
    assert_eq!(
        validate(link.to_str().unwrap(), 0, "", "", "{}", &layout)
            .unwrap()
            .desktop_id,
        "me.x.Link"
    );
    assert!(validate(path, 0, "", "", "[1]", &layout).is_err());
    assert!(validate(path, 0, "", "", "{", &layout).is_err());
    assert!(matches!(
        check_options_size(&"x".repeat(64 * 1024 + 1)),
        Err(zbus::fdo::Error::InvalidArgs(message)) if message == "options are too large"
    ));

    let mut registry = AttachedRegistry::default();
    registry.set_layout(
        layout.clone(),
        vec![vec![("me.x.A".to_owned(), Zone::Center)]],
    );
    for _ in 0..8 {
        registry.insert("one".to_owned(), valid()).unwrap();
    }
    assert_eq!(registry.applets[0].zone, Zone::Center);
    assert!(registry.insert("one".to_owned(), valid()).is_err());
    for index in 0..8 {
        registry.insert(format!("other-{index}"), valid()).unwrap();
    }
    assert!(registry.insert("last".to_owned(), valid()).is_err());
    assert!(registry.remove("dev-1").is_some());
    assert!(registry.insert("one".to_owned(), valid()).is_ok());
}

#[test]
fn reconcile_keeps_unmapped_named_output_and_moves_default_output() {
    let (_dir, path) = desktop();
    let layout = vec![vec!["DP-1".to_owned(), "HDMI-1".to_owned()]];
    let mut registry = AttachedRegistry::default();
    registry.set_layout(layout.clone(), vec![vec![]]);
    registry
        .insert(
            "named".to_owned(),
            validate(path.to_str().unwrap(), 0, "DP-1", "", "{}", &layout).unwrap(),
        )
        .unwrap();
    registry
        .insert(
            "default".to_owned(),
            validate(path.to_str().unwrap(), 0, "", "", "{}", &layout).unwrap(),
        )
        .unwrap();
    let stale = registry
        .insert(
            "stale".to_owned(),
            validate(path.to_str().unwrap(), 0, "", "", "{}", &layout).unwrap(),
        )
        .unwrap();
    registry.offered(&stale);
    assert_eq!(registry.applets.len(), 1);
    registry.remove(&stale);
    registry
        .insert(
            "named".to_owned(),
            validate(path.to_str().unwrap(), 0, "DP-1", "", "{}", &layout).unwrap(),
        )
        .unwrap();
    registry
        .insert(
            "default".to_owned(),
            validate(path.to_str().unwrap(), 0, "", "", "{}", &layout).unwrap(),
        )
        .unwrap();
    for applet in &mut registry.applets {
        applet.offered = true;
    }
    assert_eq!(registry.applets[1].output, "");
    assert_eq!(visible_on(&registry.applets, &layout, 0, "DP-1").len(), 2);

    let unmapped = vec![vec!["HDMI-1".to_owned()]];
    registry.set_layout(unmapped.clone(), vec![vec![]]);
    assert_eq!(registry.applets.len(), 2);
    assert_eq!(visible_on(&registry.applets, &unmapped, 0, "DP-1").len(), 0);
    assert_eq!(
        visible_on(&registry.applets, &unmapped, 0, "HDMI-1")[0].1,
        "me.x.A"
    );

    registry.set_layout(layout.clone(), vec![vec![]]);
    let remapped = visible_on(&registry.applets, &layout, 0, "DP-1");
    assert_eq!(remapped.len(), 2);
    assert_eq!(remapped[0].1, "me.x.A");
    assert_eq!(remapped[1].1, "me.x.A");
    assert!(visible_on(&registry.applets, &layout, 0, "HDMI-1").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn private_bus_checks_sender_and_delivers_fds() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    let first = bus.connection().await;
    let second = bus.connection().await;
    let registry = SharedRegistry::default();
    registry
        .lock()
        .unwrap()
        .set_layout(vec![vec!["DP-1".to_owned()]], vec![vec![]]);
    let services = PanelServices::start_with_buses(
        &glimpse_config::Config::default(),
        Buses::unavailable("no bus in tests"),
    );
    let exec = services.handles.exec.clone();
    let (input, receiver) = relm4::channel();
    export(&server, registry.clone(), exec.clone(), input).await;
    let name = glimpse_dbus::applet_host_name(&glimpse_dbus::panel_app_id());
    let first = AppletHostProxy::builder(&first)
        .destination(name.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    let second = AppletHostProxy::builder(&second)
        .destination(name)
        .unwrap()
        .build()
        .await
        .unwrap();
    let (_dir, path) = desktop();
    let (to_panel, _writer) = std::os::unix::net::UnixStream::pair().unwrap();
    let (_reader, from_panel) = std::os::unix::net::UnixStream::pair().unwrap();
    let handle = first
        .attach(
            path.to_str().unwrap(),
            0,
            "",
            "",
            "{}",
            "{}",
            Fd::from(&to_panel),
            Fd::from(&from_panel),
        )
        .await
        .unwrap();
    assert_eq!(handle, "dev-1");
    assert!(matches!(receiver.recv().await, Some(AppInput::Attached)));
    assert!(exec.snapshot().offers.contains("me.x.A"));
    let (duplicate_to_panel, _duplicate_writer) = std::os::unix::net::UnixStream::pair().unwrap();
    let (_duplicate_reader, duplicate_from_panel) = std::os::unix::net::UnixStream::pair().unwrap();
    let error = second
        .attach(
            path.to_str().unwrap(),
            0,
            "",
            "",
            "{}",
            "{}",
            Fd::from(&duplicate_to_panel),
            Fd::from(&duplicate_from_panel),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("me.x.A is already attached"));
    assert_eq!(registry.lock().unwrap().applets.len(), 1);
    let error = second.set_options(&handle, "{}").await.unwrap_err();
    assert!(error.to_string().contains("AccessDenied"));
    let error = first.set_options("unknown", "{}").await.unwrap_err();
    assert!(error.to_string().contains("AccessDenied"));
    services.shutdown().await;
}
