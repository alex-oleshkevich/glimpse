use super::*;
use glimpse_services::{Op, WireNode};
use serde_json::json;

#[tokio::test]
async fn failed_slot_refreshes_to_one_error_indicator() {
    use crate::applet::Applet;
    use glimpse_dbus::{Buses, notifications::NotificationsProvider};
    use glimpse_services::{
        ExecConfig, ExecDependencies, ExecEntry, ServiceRuntime, SystemdLauncher,
    };
    use tokio_util::sync::CancellationToken;

    struct MissingCatalog;
    impl glimpse_services::ExecCatalog for MissingCatalog {
        fn resolve(&self, _id: &str) -> Result<ExecEntry, String> {
            Err("not installed".into())
        }
    }

    let dir = tempfile::tempdir().expect("test directories");
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<glimpse_services::Exec>::new(
        ExecConfig {
            applets: std::collections::BTreeMap::from([(
                "missing".into(),
                glimpse_config::ExecAppletConfig {
                    applet: "me.example.Missing".into(),
                    options: Default::default(),
                    env: Default::default(),
                },
            )]),
            state_base: Some(dir.path().join("state")),
            runtime_base: Some(dir.path().join("runtime")),
            panel: "me.aresa.TestPanel".into(),
            application_dirs: vec![dir.path().join("applications")],
        },
        Buses::unavailable("test"),
        cancel.clone(),
    );
    let task = tokio::spawn(async move {
        runtime
            .run(ExecDependencies {
                catalog: Arc::new(MissingCatalog),
                launcher: Arc::new(SystemdLauncher::new(None)),
            })
            .await;
    });
    let notifications = NotificationsProvider::unavailable("test");
    let mut applet = super::super::Exec::start(
        "missing",
        handle.clone(),
        notifications.handle(),
        super::super::placement(
            glimpse_config::Position::Top,
            32,
            None,
            crate::components::panel::Zone::Start,
        ),
    );
    let mut state = handle.subscribe();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !state
            .borrow()
            .slots
            .get(&applet.slot)
            .is_some_and(|slot| matches!(slot.status, ExecStatus::Failed(_)))
        {
            state.changed().await.expect("slot status");
        }
    })
    .await
    .expect("failed slot");
    let (sender, _receiver) = relm4::channel();
    let ctx = crate::applet::Ctx::new("test".into(), None, sender);
    applet.refresh(&ctx.opener());
    let chips = applet.indicators();
    assert_eq!(chips.len(), 1);
    assert_eq!(chips[0].severity, Some(Severity::Error));
    assert_eq!(chips[0].tooltip.as_deref(), Some("missing"));
    assert!(
        chips[0]
            .icon
            .as_ref()
            .expect("error icon")
            .equal(Some(&gio::ThemedIcon::new("dialog-error-symbolic")))
    );
    assert_eq!(applet.failure().as_deref(), Some("not installed"));
    applet.refresh(&ctx.opener());
    assert_eq!(applet.indicators().len(), 1);
    handle.detach(applet.slot).await.expect("detach");
    cancel.cancel();
    task.await.expect("service stopped");
}

#[test]
fn failed_status_always_shows_its_reason() {
    for has_indicators in [false, true] {
        assert_eq!(
            failure(&ExecStatus::Failed("not installed".into()), has_indicators),
            Some("not installed".into())
        );
    }
}

#[test]
fn ended_status_shows_a_notice_only_without_indicators() {
    assert_eq!(
        failure(&ExecStatus::Ended, false),
        Some(gettext("The applet stopped"))
    );
    assert_eq!(failure(&ExecStatus::Ended, true), None);
}

#[test]
fn repeated_restarts_show_a_notice_only_without_indicators() {
    for attempt in [0, 1, 2] {
        for has_indicators in [false, true] {
            assert_eq!(
                failure(&ExecStatus::Restarting { attempt }, has_indicators),
                None
            );
        }
    }
    for attempt in [3, 4, u32::MAX] {
        assert_eq!(
            failure(&ExecStatus::Restarting { attempt }, false),
            Some(gettext("The applet keeps exiting. It restarts on its own."))
        );
        assert_eq!(failure(&ExecStatus::Restarting { attempt }, true), None);
    }
}

#[test]
fn starting_and_running_do_not_show_a_failure() {
    for status in [ExecStatus::Starting, ExecStatus::Running] {
        for has_indicators in [false, true] {
            assert_eq!(failure(&status, has_indicators), None);
        }
    }
}

#[test]
fn starting_and_early_restarts_are_in_flight_only_without_indicators() {
    assert!(in_flight(&ExecStatus::Starting, false));
    assert!(!in_flight(&ExecStatus::Starting, true));
    for attempt in [0, 1, 2] {
        assert!(in_flight(&ExecStatus::Restarting { attempt }, false));
        assert!(!in_flight(&ExecStatus::Restarting { attempt }, true));
    }
}

#[test]
fn terminal_statuses_are_not_in_flight() {
    for status in [
        ExecStatus::Running,
        ExecStatus::Ended,
        ExecStatus::Failed("not installed".into()),
        ExecStatus::Restarting { attempt: 3 },
        ExecStatus::Restarting { attempt: u32::MAX },
    ] {
        for has_indicators in [false, true] {
            assert!(!in_flight(&status, has_indicators));
        }
    }
}

#[test]
fn failure_reason_is_capped_without_interpreting_markup() {
    let reason = format!("<b>markup</b>{}", "é".repeat(987));
    assert_eq!(reason.chars().count(), 1000);
    let reason = failure(&ExecStatus::Failed(reason), false).expect("failure reason");
    let shown = glimpse_utils::clean(&reason, 256);
    assert_eq!(shown, format!("<b>markup</b>{}…", "é".repeat(243)));
    assert_eq!(shown.chars().count(), 257);
}

#[test]
fn keeps_newer_local_value_until_acknowledged() {
    assert!(keeps_newer_local_value(Some(5), Some(4)));
    assert!(!keeps_newer_local_value(Some(5), Some(5)));
    assert!(!keeps_newer_local_value(Some(5), None));
}

#[test]
fn icon_cache_clears_after_64_entries() {
    let mut icons = HashMap::new();
    for index in 0..65 {
        cached_icon(&mut icons, &format!("icon-{index}"));
    }
    assert_eq!(icons.len(), 65);
    cached_icon(&mut icons, "next");
    assert_eq!(icons.len(), 1);
}

#[test]
fn a_continuous_scale_gets_a_usable_gtk_step() {
    assert_eq!(scale_step(0.0, 100.0, 0.0), 1.0);
    assert_eq!(scale_step(0.0, 100.0, 0.5), 0.5);
}

#[test]
fn requests_survive_an_unchanged_tree() {
    let tree = Arc::new(Tree::default());
    let mut serial = 0;
    let (requests, unchanged) =
        requests_before_tree_check(&mut serial, &[(1, BarRequest::ClosePopover)], &tree, &tree);
    assert!(unchanged);
    assert_eq!(requests, vec![BarRequest::ClosePopover]);
    assert_eq!(serial, 1);
}

#[test]
fn pointer_events_keep_the_wire_names_and_arguments() {
    assert_eq!(pointer(Pointer::Press(Button::Left)), None);
    assert_eq!(
        pointer(Pointer::Press(Button::Right)),
        Some(("onPress", vec![json!(3)], true))
    );
    assert_eq!(
        pointer(Pointer::Scroll(Direction::Up)),
        Some(("onScroll", vec![json!(0), json!(-1)], false))
    );
}

#[test]
fn tree_maps_every_chip_field_and_drops_bad_dots() {
    let tree =
        Tree::default()
            .apply(vec![Op::Insert {
        parent: 0,
        node: WireNode { id: 1, kind: "indicator".into(), props: serde_json::from_value(json!({
            "icon": "view-list-symbolic", "overlay": "emblem-symbolic", "dot": "#e01b24",
            "text": "Todo", "tooltip": "Tasks", "badge": "3", "severity": "warning",
            "attention": true, "notice": true, "className": "pill"
        })).expect("props"), children: vec![] },
        before: None,
    }])
            .expect("tree");
    let chips = indicators(&tree, &mut |name| gio::ThemedIcon::new(name).upcast());
    let chip = &chips[0].1;
    assert!(chip.icon.is_some());
    assert!(chip.overlay.is_some());
    assert!(chip.dot.is_some());
    assert_eq!(chip.label.as_deref(), Some("Todo"));
    assert_eq!(chip.tooltip.as_deref(), Some("Tasks"));
    assert_eq!(chip.badge.as_deref(), Some("3"));
    assert_eq!(chip.severity, Some(Severity::Warning));
    assert!(chip.attention && chip.notice);
    assert_eq!(chip.class.as_deref(), Some("pill"));
    let bad = Tree::default()
        .apply(vec![Op::Insert {
            parent: 0,
            node: WireNode {
                id: 2,
                kind: "indicator".into(),
                props: serde_json::from_value(json!({"dot":"bad"})).expect("props"),
                children: vec![],
            },
            before: None,
        }])
        .expect("tree");
    let bad = &indicators(&bad, &mut |name| gio::ThemedIcon::new(name).upcast())[0].1;
    assert!(bad.dot.is_none());
}

#[test]
fn files_that_run_programs_stay_closed() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().to_path_buf();
    let state = root.join("state");
    std::fs::create_dir(&state).expect("state");
    let outside = root.as_path().join("notes.txt");
    std::fs::write(&outside, "hi").expect("outside");
    let script = root.as_path().join("run.sh");
    std::fs::write(&script, "#!/bin/sh\n").expect("script");
    let desktop = root.as_path().join("evil.desktop");
    std::fs::write(&desktop, "[Desktop Entry]\nExec=sh\n").expect("desktop");
    let tool = root.as_path().join("tool");
    std::fs::write(&tool, "x").expect("tool");
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).expect("mode");
    let link = state.join("escape");
    std::os::unix::fs::symlink(&outside, &link).expect("link");

    let canonical = outside.canonicalize().expect("outside");
    assert_eq!(
        openable(&outside),
        Ok((canonical.clone(), "text/plain".to_owned()))
    );
    assert!(openable(root.as_path()).is_ok());
    assert_eq!(openable(&link).map(|(path, _)| path), Ok(canonical));
    let disguised = root.as_path().join("notes");
    let mut elf = b"\x7f\x45LF\x02\x01\x01".to_vec();
    elf.resize(64, 0);
    std::fs::write(&disguised, elf).expect("disguised");
    assert_eq!(openable(&disguised), Err("launches programs"));
    assert_eq!(openable(&tool), Err("executable"));
    assert_eq!(openable(&script), Err("launches programs"));
    assert_eq!(openable(&desktop), Err("launches programs"));
    assert_eq!(openable(&root.as_path().join("missing")), Err("missing"));
}
