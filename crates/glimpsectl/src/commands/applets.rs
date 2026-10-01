use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Local, Utc};
use glimpse_config::{AppletKind, AppletSandbox, Config, is_desktop_id, resolve_applet};
use glimpse_dbus::systemd1::{Systemd1ManagerProxy, Systemd1ScopeProxy, Systemd1UnitEntry};
use glimpse_services::{DesktopCatalog, ExecCatalog, ExecEntry, exec_unit_pattern};
use serde::Serialize;
use serde_json::{Map, Value};
use zbus::Connection;
use zbus::proxy::CacheProperties;

use super::{emit, safe, within};
use crate::render::{self, Section, Table};

#[derive(Debug, Clone, Serialize)]
pub struct Placement {
    pub instance: String,
    pub zone: String,
    pub panel: usize,
    pub options: Map<String, Value>,
    pub sandbox: AppletSandbox,
}

#[derive(Debug, Serialize)]
pub struct Row {
    pub id: String,
    name: String,
    pub placement: String,
    path: String,
    exec: String,
    pub state: String,
    refused: Option<String>,
}

#[derive(Serialize)]
pub struct Desktop {
    path: Option<PathBuf>,
    name: Option<String>,
    icon: Option<String>,
    exec: Option<String>,
    argv: Option<Vec<String>>,
    pub refusal: Option<String>,
}

#[derive(Serialize)]
struct RuntimeUnit {
    unit: String,
    status: &'static str,
    pids: Option<Vec<u32>>,
    memory_current: Option<u64>,
    active_enter_timestamp: Option<u64>,
}

#[derive(Serialize)]
struct Runtime {
    status: &'static str,
    units: Vec<RuntimeUnit>,
}

pub fn valid_id(id: &str) -> Result<()> {
    if !is_desktop_id(id) {
        bail!("{id:?} is not a desktop-file id");
    }
    Ok(())
}

pub fn placements(config: &Config) -> BTreeMap<String, Vec<Placement>> {
    let mut result = BTreeMap::new();
    for (panel, definition) in config.panels.iter().enumerate() {
        for (zone, names) in [
            ("left", &definition.left),
            ("center", &definition.center),
            ("right", &definition.right),
        ] {
            for instance in names {
                let Some(applet) = resolve_applet(instance, &config.applets) else {
                    continue;
                };
                let AppletKind::Exec(exec) = applet.kind else {
                    continue;
                };
                result
                    .entry(exec.applet)
                    .or_insert_with(Vec::new)
                    .push(Placement {
                        instance: instance.clone(),
                        zone: zone.to_owned(),
                        panel,
                        options: exec.options,
                        sandbox: exec.sandbox,
                    });
            }
        }
    }
    result
}

pub fn rows(
    installed: Vec<(String, Result<ExecEntry, String>)>,
    placed: &BTreeMap<String, Vec<Placement>>,
    running: &BTreeSet<String>,
) -> Vec<Row> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for (id, entry) in installed {
        seen.insert(id.clone());
        let (name, path, exec, refused) = match entry {
            Ok(entry) => (
                entry.name,
                entry.path.display().to_string(),
                entry.exec,
                None,
            ),
            Err(reason) => ("—".to_owned(), "—".to_owned(), "—".to_owned(), Some(reason)),
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
            refused,
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
                refused: None,
            });
        }
    }
    result
}

pub fn list_value(rows: Vec<Row>, unavailable: bool) -> Value {
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
                    safe(&row.id),
                    safe(row.refused.as_deref().unwrap_or(&row.name)),
                    row.placement,
                    safe(&row.path),
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

pub fn desktop(result: Result<ExecEntry, String>) -> Desktop {
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

pub fn desktop_detail(desktop: &Desktop) -> Result<String> {
    if let Some(reason) = &desktop.refusal {
        return Ok(safe(reason));
    }
    Ok(format!(
        "path: {}\nName: {}\nIcon: {}\nExec: {}\nargv: {}",
        desktop
            .path
            .as_ref()
            .map_or_else(|| "—".to_owned(), |path| safe(&path.display().to_string())),
        safe(desktop.name.as_deref().unwrap_or("—")),
        safe(desktop.icon.as_deref().unwrap_or("—")),
        safe(desktop.exec.as_deref().unwrap_or("—")),
        safe(&serde_json::to_string(&desktop.argv)?)
    ))
}

fn config_text(placed: &[Placement]) -> String {
    if placed.is_empty() {
        return "not placed".to_owned();
    }
    placed
        .iter()
        .map(|place| {
            let mut lines = vec![format!(
                "panel {} · {} · {}",
                place.panel,
                safe(&place.zone),
                safe(&place.instance)
            )];
            if place.options.is_empty() {
                lines.push("  options: none".to_owned());
            } else {
                for (key, value) in &place.options {
                    lines.push(format!(
                        "  options: {} = {}",
                        safe(key),
                        safe(&value.to_string())
                    ));
                }
            }
            let grants = |paths: &[String]| {
                if paths.is_empty() {
                    "none".to_owned()
                } else {
                    paths
                        .iter()
                        .map(|path| safe(path))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            };
            lines.push(format!(
                "  sandbox: read {}; write {}; network {}{}",
                grants(&place.sandbox.read),
                grants(&place.sandbox.write),
                if place.sandbox.network { "yes" } else { "no" },
                if place.sandbox.unconfined {
                    "; unconfined"
                } else {
                    ""
                }
            ));
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn size_text(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut size = bytes as f64;
    let mut unit = "B";
    for next in ["KiB", "MiB", "GiB"] {
        size /= 1024.0;
        unit = next;
        if size < 1024.0 {
            break;
        }
    }
    format!("{size:.1} {unit}")
}

fn runtime_text(runtime: &Runtime) -> String {
    let mut text = if runtime.status == "not running" {
        "stopped".to_owned()
    } else {
        runtime.status.to_owned()
    };
    for unit in &runtime.units {
        let _ = write!(text, "\n{}  {}", safe(&unit.unit), unit.status);
        if let Some(pids) = &unit.pids {
            let pids = if pids.is_empty() {
                "none".to_owned()
            } else {
                pids.iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let _ = write!(text, "  pid {pids}");
        }
        if let Some(memory) = unit.memory_current {
            let _ = write!(text, "  memory {}", size_text(memory));
        }
        if let Some(since) = unit
            .active_enter_timestamp
            .and_then(|value| i64::try_from(value).ok())
            .and_then(DateTime::<Utc>::from_timestamp_micros)
        {
            let _ = write!(
                text,
                "  since {}",
                since.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S")
            );
        }
    }
    text
}

async fn runtime(connection: &Connection, id: &str) -> Result<Runtime> {
    let manager = manager(connection).await?;
    let mut units = Vec::new();
    for entry in active_units(&manager, id).await? {
        let name = entry.0;
        let path = entry.6;
        let info = async {
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
            Ok::<_, anyhow::Error>((
                processes.into_iter().map(|(_, pid, _)| pid).collect(),
                memory_current,
                active_enter_timestamp,
            ))
        }
        .await;
        units.push(match info {
            Ok((pids, memory_current, active_enter_timestamp)) => RuntimeUnit {
                unit: name,
                status: "running",
                pids: Some(pids),
                memory_current: Some(memory_current),
                active_enter_timestamp: Some(active_enter_timestamp),
            },
            Err(_) => RuntimeUnit {
                unit: name,
                status: "unavailable",
                pids: None,
                memory_current: None,
                active_enter_timestamp: None,
            },
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
    Section::new("Config").with(config_text(&placed)).print()?;
    render::print("")?;
    Section::new("Runtime")
        .with(runtime_text(&runtime))
        .print()?;
    Ok(())
}

pub async fn applets_restart(id: String) -> Result<()> {
    valid_id(&id)?;
    let connection = session().await.context("runtime: unavailable")?;
    let manager = manager(&connection).await.context("runtime: unavailable")?;
    let units = active_units(&manager, &id).await?;
    anyhow::ensure!(!units.is_empty(), "no running scopes for {id}");
    for unit in units {
        within(manager.kill_unit(&unit.0, "all", 15)).await?;
    }
    Ok(())
}

#[cfg(test)]
mod inspect_tests;
