use super::*;
use glimpse_services::{Op, WireNode};
use serde_json::json;

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
fn tree_maps_every_chip_field_and_drops_bad_icons_and_dots() {
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
                props: serde_json::from_value(json!({"icon":"bad/icon","dot":"bad"}))
                    .expect("props"),
                children: vec![],
            },
            before: None,
        }])
        .expect("tree");
    let bad = &indicators(&bad, &mut |name| gio::ThemedIcon::new(name).upcast())[0].1;
    assert!(bad.icon.is_none());
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
