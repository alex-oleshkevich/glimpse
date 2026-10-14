use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use glimpse_config::Config;
use glimpse_services::ExecEntry;
use glimpsectl::commands::applets::{
    Placement, desktop, desktop_detail, list_value, placements, rows, valid_id,
};
use glimpsectl::commands::safe;
use serde_json::Map;

#[test]
fn list_json_reports_unavailable_runtime() {
    let value = list_value(Vec::new(), true);
    assert_eq!(value["runtime"], "unavailable");
    assert_eq!(value["applets"], serde_json::json!([]));
}

#[test]
fn list_json_keeps_refusal_out_of_name() {
    let rows = rows(
        vec![("me.example.Bad".to_owned(), Err("hidden".to_owned()))],
        &BTreeMap::new(),
        &BTreeSet::new(),
    );
    let value = list_value(rows, false);
    assert_eq!(value["applets"][0]["name"], "—");
    assert_eq!(value["applets"][0]["refused"], "hidden");
}

#[test]
fn placement_is_recorded_once_per_zone_occurrence() {
    let mut config = Config::default();
    config.panels[0].left = vec!["me.example.Clock".to_owned()];
    config.panels.push(config.panels[0].clone());
    assert_eq!(placements(&config)["me.example.Clock"].len(), 2);
}

fn entry(name: &str) -> ExecEntry {
    ExecEntry {
        argv: vec!["applet".to_owned()],
        cwd: None,
        name: name.to_owned(),
        icon: None,
        path: PathBuf::from(format!("/apps/{name}.desktop")),
        exec: "applet".to_owned(),
    }
}

#[test]
fn installed_rows_precede_missing_placements() {
    let mut placed = BTreeMap::new();
    placed.insert(
        "me.example.A".to_owned(),
        vec![Placement {
            instance: "a1".to_owned(),
            zone: "left".to_owned(),
            panel: 0,
            options: Map::new(),
        }],
    );
    placed.insert(
        "me.example.Missing".to_owned(),
        vec![Placement {
            instance: "lost".to_owned(),
            zone: "right".to_owned(),
            panel: 0,
            options: Map::new(),
        }],
    );
    let running = BTreeSet::from(["me.example.A".to_owned()]);
    let rows = rows(
        vec![
            ("me.example.A".to_owned(), Ok(entry("A"))),
            ("me.example.B".to_owned(), Ok(entry("B"))),
        ],
        &placed,
        &running,
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.state.as_str())
            .collect::<Vec<_>>(),
        ["running", "stopped", "not installed"]
    );
    assert_eq!(rows[0].placement, "a1");
    assert_eq!(rows[1].placement, "—");
    assert_eq!(rows[2].id, "me.example.Missing");
}

#[test]
fn refused_desktop_keeps_the_reason_and_valid_entry_keeps_argv() {
    let refused = desktop(Err("Hidden".to_owned()));
    assert_eq!(refused.refusal.as_deref(), Some("Hidden"));
    assert_eq!(desktop_detail(&refused).expect("render refusal"), "Hidden");
    let mut entry = entry("A");
    entry.exec = "applet %c".to_owned();
    entry.argv = glimpse_services::exec_expand(&entry.exec, &entry.name, None, &entry.path)
        .expect("expanded argv");
    let expected = serde_json::to_string(&entry.argv).expect("argv JSON");
    let detail = desktop_detail(&desktop(Ok(entry))).expect("render desktop");
    assert!(detail.contains(&format!("argv: {expected}")));
}

#[test]
fn text_detail_cleans_hostile_desktop_fields() {
    assert_eq!(
        desktop_detail(&desktop(Err("bad\nreason".to_owned()))).expect("refusal"),
        safe("bad\nreason")
    );
    let mut entry = entry("bad\nname");
    entry.exec = "bad\nexec".to_owned();
    let detail = desktop_detail(&desktop(Ok(entry))).expect("detail");
    assert!(detail.contains(&format!("Name: {}", safe("bad\nname"))));
    assert!(detail.contains(&format!("Exec: {}", safe("bad\nexec"))));
}

#[test]
fn invalid_id_is_named() {
    let error = valid_id("bad*id").expect_err("invalid id");
    assert!(error.to_string().contains("bad*id"));
}
