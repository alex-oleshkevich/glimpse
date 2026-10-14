use super::*;
use crate::services::PanelServices;
use glimpse_dbus::Buses;
use glimpse_dbus::applet_host::AppletHostProxy;
use glimpse_dbus::testing::PrivateBus;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

fn desktop() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "glimpse-panel-attached-{}-{}",
        std::process::id(),
        NEXT_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("me.x.A.desktop");
    fs::write(
        &path,
        "[Desktop Entry]\nType=Application\nName=Test\nExec=/bin/true\nImplements=me.aresa.Glimpse.Applet1;\n",
    )
    .unwrap();
    path
}

#[test]
fn validation_and_caps_reject_bad_attachments() {
    let path = desktop();
    let path = path.to_str().unwrap();
    let layout = vec![vec!["DP-1".to_owned()]];
    let valid = || validate(path, 0, "", "", "{}", &layout).unwrap();
    let numbered = |index: usize| {
        let mut valid = valid();
        valid.desktop_id = format!("me.x.A{index}");
        valid
    };
    let insert = |registry: &mut AttachedRegistry, sender: &str, index: usize| {
        let valid = numbered(index);
        registry.insert(sender.to_owned(), valid.desktop_id.clone(), valid)
    };
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
    for index in 0..8 {
        insert(&mut registry, "one", index).unwrap();
    }
    assert_eq!(registry.applets[0].zone, Zone::Right);
    assert!(insert(&mut registry, "one", 8).is_err());
    for index in 0..8 {
        insert(&mut registry, &format!("other-{index}"), 10 + index).unwrap();
    }
    assert!(insert(&mut registry, "last", 20).is_err());
    assert!(registry.remove("me.x.A0").is_some());
    assert!(insert(&mut registry, "one", 8).is_ok());
    assert!(insert(&mut registry, "one", 8).is_err());
    let mut configured = AttachedRegistry::default();
    configured.set_layout(
        layout.clone(),
        vec![vec![("me.x.A".to_owned(), Zone::Center)]],
    );
    let valid = valid();
    configured
        .insert("one".to_owned(), valid.desktop_id.clone(), valid)
        .unwrap();
    assert_eq!(configured.applets[0].zone, Zone::Center);
    fs::remove_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
}

#[test]
fn reconcile_keeps_unmapped_named_output_and_moves_default_output() {
    let path = desktop();
    let layout = vec![vec!["DP-1".to_owned(), "HDMI-1".to_owned()]];
    let mut registry = AttachedRegistry::default();
    registry.set_layout(layout.clone(), vec![vec![]]);
    let attach = |registry: &mut AttachedRegistry, sender: &str, id: &str, output: &str| {
        let mut valid = validate(path.to_str().unwrap(), 0, output, "", "{}", &layout).unwrap();
        valid.desktop_id = id.to_owned();
        registry.insert(sender.to_owned(), id.to_owned(), valid)
    };
    attach(&mut registry, "named", "me.x.A", "DP-1").unwrap();
    attach(&mut registry, "default", "me.x.B", "").unwrap();
    assert_eq!(registry.applets[1].output, "");
    assert_eq!(visible_on(&registry.applets, &layout, 0, "DP-1").len(), 2);

    let unmapped = vec![vec!["HDMI-1".to_owned()]];
    registry.set_layout(unmapped.clone(), vec![vec![]]);
    assert_eq!(registry.applets.len(), 2);
    assert_eq!(visible_on(&registry.applets, &unmapped, 0, "DP-1").len(), 0);
    assert_eq!(
        visible_on(&registry.applets, &unmapped, 0, "HDMI-1")[0].1,
        "me.x.B"
    );

    registry.set_layout(layout.clone(), vec![vec![]]);
    let remapped = visible_on(&registry.applets, &layout, 0, "DP-1");
    assert_eq!(remapped.len(), 2);
    assert_eq!(remapped[0].1, "me.x.A");
    assert_eq!(remapped[1].1, "me.x.B");
    assert!(visible_on(&registry.applets, &layout, 0, "HDMI-1").is_empty());
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn private_bus_owns_handles_and_detaches_a_vanished_sender() {
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
    let exec = services.exec();
    let (input, receiver) = relm4::channel();
    export(&server, registry.clone(), exec.clone(), input).await;
    let name = glimpse_dbus::applet_host_name(&glimpse_dbus::panel_app_id());
    let proxy = |connection: zbus::Connection| {
        let name = name.clone();
        async move {
            AppletHostProxy::builder(&connection)
                .destination(name)
                .unwrap()
                .build()
                .await
                .unwrap()
        }
    };
    let owner = proxy(first).await;
    let stranger = proxy(second).await;
    let path = desktop();
    let path = path.to_str().unwrap();
    let (stderr, _reader) = std::os::unix::net::UnixStream::pair().unwrap();
    let env = std::collections::HashMap::from([("K", "v")]);
    let handle = owner
        .attach(path, 0, "", "", "{}", env.clone(), Fd::from(&stderr))
        .await
        .unwrap();
    assert_eq!(handle, "me.x.A");
    assert!(matches!(receiver.recv().await, Some(AppInput::Attached)));
    assert!(exec.snapshot().dev.contains("me.x.A"));
    let error = stranger
        .attach(path, 0, "", "", "{}", env.clone(), Fd::from(&stderr))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("me.x.A is already attached"));
    assert_eq!(registry.lock().unwrap().applets.len(), 1);
    for error in [
        stranger.set_options(&handle, "{}").await.unwrap_err(),
        stranger.restart(&handle).await.unwrap_err(),
        stranger.detach(&handle).await.unwrap_err(),
        owner.set_options("unknown", "{}").await.unwrap_err(),
        owner.restart("unknown").await.unwrap_err(),
        owner.detach("unknown").await.unwrap_err(),
    ] {
        assert!(error.to_string().contains("AccessDenied"));
    }
    assert!(exec.snapshot().dev.contains("me.x.A"));
    owner.set_options(&handle, "{\"a\":1}").await.unwrap();
    assert!(owner.set_options(&handle, "[1]").await.is_err());
    owner.restart(&handle).await.unwrap();
    owner.detach(&handle).await.unwrap();
    assert!(exec.snapshot().dev.is_empty());
    assert!(matches!(
        receiver.recv().await,
        Some(AppInput::Detached(detached)) if detached == "me.x.A"
    ));
    assert!(registry.lock().unwrap().applets.is_empty());

    let doomed = bus.connection().await;
    let transient = proxy(doomed.clone()).await;
    transient
        .attach(path, 0, "", "", "{}", env, Fd::from(&stderr))
        .await
        .unwrap();
    assert!(matches!(receiver.recv().await, Some(AppInput::Attached)));
    assert!(exec.snapshot().dev.contains("me.x.A"));
    drop(transient);
    doomed.close().await.unwrap();
    let detached = tokio::time::timeout(std::time::Duration::from_secs(10), receiver.recv())
        .await
        .unwrap();
    assert!(matches!(detached, Some(AppInput::Detached(detached)) if detached == "me.x.A"));
    assert!(exec.snapshot().dev.is_empty());
    services.shutdown().await;
    fs::remove_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
}
