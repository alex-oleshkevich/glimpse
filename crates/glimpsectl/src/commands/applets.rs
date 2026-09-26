use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use glimpse_config::{AppletKind, Config, is_desktop_id, placed_applets};
use glimpse_dbus::systemd1::{Systemd1ManagerProxy, Systemd1ScopeProxy, Systemd1UnitEntry};
use glimpse_services::{DesktopCatalog, ExecCatalog, ExecEntry, exec_unit_pattern};
use serde::Serialize;
use serde_json::{Map, Value};
use zbus::Connection;
use zbus::proxy::CacheProperties;

use super::{emit, safe, within};
use crate::render::{self, Section, Table};

#[derive(Debug, Clone, Serialize)]
struct Placement {
    instance: String,
    zone: String,
    panel: usize,
    options: Map<String, Value>,
}

#[derive(Debug, Serialize)]
struct Row {
    id: String,
    name: String,
    placement: String,
    path: String,
    exec: String,
    state: String,
}

#[derive(Serialize)]
struct Desktop {
    path: Option<PathBuf>,
    name: Option<String>,
    icon: Option<String>,
    exec: Option<String>,
    argv: Option<Vec<String>>,
    refusal: Option<String>,
}

#[derive(Serialize)]
struct RuntimeUnit {
    unit: String,
    pids: Vec<u32>,
    memory_current: u64,
    active_enter_timestamp: u64,
}

#[derive(Serialize)]
struct Runtime {
    status: &'static str,
    units: Vec<RuntimeUnit>,
}

fn valid_id(id: &str) -> Result<()> {
    if !is_desktop_id(id) {
        bail!("{id:?} is not a desktop-file id");
    }
    Ok(())
}

fn placements(config: &Config) -> BTreeMap<String, Vec<Placement>> {
    let mut result = BTreeMap::new();
    for (instance, applet) in placed_applets(config) {
        let AppletKind::Exec(exec) = applet.kind else {
            continue;
        };
        for (panel, definition) in config.panels.iter().enumerate() {
            for (zone, names) in [
                ("left", &definition.left),
                ("center", &definition.center),
                ("right", &definition.right),
            ] {
                if names.iter().any(|name| name == instance) {
                    result
                        .entry(exec.applet.clone())
                        .or_insert_with(Vec::new)
                        .push(Placement {
                            instance: instance.to_owned(),
                            zone: zone.to_owned(),
                            panel,
                            options: exec.options.clone(),
                        });
                }
            }
        }
    }
    result
}

fn rows(
    installed: Vec<(String, Result<ExecEntry, String>)>,
    placed: &BTreeMap<String, Vec<Placement>>,
    running: &BTreeSet<String>,
) -> Vec<Row> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for (id, entry) in installed {
        seen.insert(id.clone());
        let (name, path, exec) = match entry {
            Ok(entry) => (entry.name, entry.path.display().to_string(), entry.exec),
            Err(reason) => (reason, "—".to_owned(), "—".to_owned()),
        };
        result.push(Row {
            placement: placed.get(&id).map_or_else(
                || "—".to_owned(),
                |placements| {
                    placements
                        .iter()
                        .map(|place| place.instance.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                },
            ),
            state: if running.contains(&id) {
                "running"
            } else {
                "stopped"
            }
            .to_owned(),
            id,
            name,
            path,
            exec,
        });
    }
    for (id, places) in placed {
        if !seen.contains(id) {
            result.push(Row {
                id: id.clone(),
                name: "—".to_owned(),
                placement: places
                    .iter()
                    .map(|place| place.instance.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                path: "—".to_owned(),
                exec: "—".to_owned(),
                state: "not installed".to_owned(),
            });
        }
    }
    result
}

fn list_value(rows: Vec<Row>, unavailable: bool) -> Value {
    serde_json::json!({"runtime": if unavailable { "unavailable" } else { "available" }, "applets": rows})
}

async fn manager(connection: &Connection) -> Result<Systemd1ManagerProxy<'_>> {
    let proxy = within(
        Systemd1ManagerProxy::builder(connection)
            .cache_properties(CacheProperties::No)
            .build(),
    )
    .await?;
    Ok(proxy)
}

async fn active_units(
    manager: &Systemd1ManagerProxy<'_>,
    id: &str,
) -> Result<Vec<Systemd1UnitEntry>> {
    valid_id(id)?;
    let pattern = exec_unit_pattern(id);
    within(manager.list_units_by_patterns(&["active", "activating"], &[&pattern])).await
}

async fn session() -> Result<Connection> {
    within(Connection::session()).await
}

pub async fn applets_list(config_path: Option<PathBuf>, json: bool) -> Result<()> {
    let config = glimpse_config::load(config_path.as_deref())?;
    let placed = placements(&config);
    let installed = DesktopCatalog.installed();
    let ids: BTreeSet<_> = installed
        .iter()
        .map(|(id, _)| id.clone())
        .chain(placed.keys().cloned())
        .collect();
    let mut running = BTreeSet::new();
    let mut unavailable = false;
    if let Ok(connection) = session().await {
        match manager(&connection).await {
            Ok(manager) => {
                for id in &ids {
                    if !is_desktop_id(id) {
                        continue;
                    }
                    match active_units(&manager, id).await {
                        Ok(units) if !units.is_empty() => {
                            running.insert(id.clone());
                        }
                        Ok(_) => {}
                        Err(_) => {
                            unavailable = true;
                        }
                    }
                }
            }
            Err(_) => unavailable = true,
        }
    } else {
        unavailable = true;
    }
    let rows = rows(installed, &placed, &running);
    if json {
        return emit(&list_value(rows, unavailable));
    }
    render::print(
        &Table::new()
            .with_headers(["ID", "NAME", "PLACEMENT", "DESKTOP", "EXEC", "STATE"])
            .with_empty("no external applets installed or placed")
            .with_rows(rows.into_iter().map(|row| {
                [
                    row.id,
                    safe(&row.name),
                    row.placement,
                    row.path,
                    safe(&row.exec),
                    row.state,
                ]
            }))
            .render(),
    )?;
    if unavailable {
        render::print("runtime: unavailable")?;
    }
    Ok(())
}

fn desktop(result: Result<ExecEntry, String>) -> Desktop {
    match result {
        Ok(entry) => Desktop {
            path: Some(entry.path),
            name: Some(entry.name),
            icon: entry.icon,
            exec: Some(entry.exec),
            argv: Some(entry.argv),
            refusal: None,
        },
        Err(reason) => Desktop {
            path: None,
            name: None,
            icon: None,
            exec: None,
            argv: None,
            refusal: Some(reason),
        },
    }
}

fn desktop_detail(desktop: &Desktop) -> Result<String> {
    if let Some(reason) = &desktop.refusal {
        return Ok(safe(reason));
    }
    Ok(format!(
        "path: {}\nName: {}\nIcon: {}\nExec: {}\nargv: {}",
        desktop
            .path
            .as_ref()
            .map_or_else(|| "—".to_owned(), |path| path.display().to_string()),
        safe(desktop.name.as_deref().unwrap_or("—")),
        desktop.icon.as_deref().unwrap_or("—"),
        safe(desktop.exec.as_deref().unwrap_or("—")),
        safe(&serde_json::to_string(&desktop.argv)?)
    ))
}

async fn runtime(connection: &Connection, id: &str) -> Result<Runtime> {
    let manager = manager(connection).await?;
    let mut units = Vec::new();
    for entry in active_units(&manager, id).await? {
        let name = entry.0;
        let path = entry.6;
        let processes = within(manager.get_unit_processes(&name)).await?;
        let scope = within(
            Systemd1ScopeProxy::builder(connection)
                .path(path.clone())?
                .cache_properties(CacheProperties::No)
                .build(),
        )
        .await?;
        let memory_current = within(scope.memory_current()).await?;
        let unit = within(zbus::Proxy::new(
            connection,
            "org.freedesktop.systemd1",
            path,
            "org.freedesktop.systemd1.Unit",
        ))
        .await?;
        let active_enter_timestamp =
            within(unit.get_property::<u64>("ActiveEnterTimestamp")).await?;
        units.push(RuntimeUnit {
            unit: name,
            pids: processes.into_iter().map(|(_, pid, _)| pid).collect(),
            memory_current,
            active_enter_timestamp,
        });
    }
    Ok(Runtime {
        status: if units.is_empty() {
            "not running"
        } else {
            "running"
        },
        units,
    })
}

pub async fn applets_inspect(config_path: Option<PathBuf>, id: String, json: bool) -> Result<()> {
    valid_id(&id)?;
    let config = glimpse_config::load(config_path.as_deref())?;
    let desktop = desktop(DesktopCatalog.resolve(&id));
    let placed = placements(&config).remove(&id).unwrap_or_default();
    let runtime = match session().await {
        Ok(connection) => runtime(&connection, &id).await.unwrap_or(Runtime {
            status: "unavailable",
            units: Vec::new(),
        }),
        Err(_) => Runtime {
            status: "unavailable",
            units: Vec::new(),
        },
    };
    if json {
        return emit(
            &serde_json::json!({ "desktop": desktop, "config": placed, "runtime": runtime }),
        );
    }
    Section::new("Desktop")
        .with(desktop_detail(&desktop)?)
        .print()?;
    render::print("")?;
    Section::new("Config")
        .with(if placed.is_empty() {
            "not placed".to_owned()
        } else {
            serde_json::to_string_pretty(&placed)?
        })
        .print()?;
    render::print("")?;
    let detail = if runtime.status == "running" {
        serde_json::to_string_pretty(&runtime.units)?
    } else if runtime.status == "unavailable" {
        "runtime: unavailable".to_owned()
    } else {
        runtime.status.to_owned()
    };
    Section::new("Runtime").with(detail).print()?;
    Ok(())
}

pub async fn applets_restart(id: String) -> Result<()> {
    valid_id(&id)?;
    let connection = session().await.context("runtime: unavailable")?;
    let manager = manager(&connection).await.context("runtime: unavailable")?;
    for unit in active_units(&manager, &id).await? {
        within(manager.kill_unit(&unit.0, "all", 15)).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_json_reports_unavailable_runtime() {
        let value = list_value(Vec::new(), true);
        assert_eq!(value["runtime"], "unavailable");
        assert_eq!(value["applets"], serde_json::json!([]));
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
}
