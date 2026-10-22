use std::collections::{BTreeSet, HashMap};

use anyhow::Result;
use glimpse_dbus::gtk_actions::{GtkActionsProxy, object_path};
use serde::Serialize;
use zbus::Connection;
use zbus::export::futures_core::Stream;
use zbus::proxy::CacheProperties;
use zbus::zvariant::{OwnedValue, Value};

use super::{emit, safe, within, yes_no};
use crate::cli::{AppletCommand, AppletTarget, PanelCommand, PopoversCommand, Position};
use crate::errors::{Gone, Refused};
use crate::render::{self, Table};

type Descriptions = HashMap<String, (bool, zbus::zvariant::Signature, Vec<OwnedValue>)>;

#[derive(Clone, Debug, Serialize)]
struct Indicator {
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    overlay: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tooltip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    badge: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attention: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notice: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    class: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct AppletRow {
    #[serde(skip)]
    action: String,
    connector: String,
    position: Position,
    zone: String,
    index: u32,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    alive: bool,
    #[serde(rename = "own-view")]
    own_view: bool,
    popover: bool,
    indicators: Vec<Indicator>,
}

fn text(values: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    Some(safe(<&str>::try_from(values.get(key)?).ok()?))
}

fn flag(values: &HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
    bool::try_from(values.get(key)?).ok()
}

fn decode(action: &str, value: &OwnedValue) -> Option<AppletRow> {
    let values = HashMap::<String, OwnedValue>::try_from(value.clone()).ok()?;
    let indicators = values
        .get("indicators")
        .and_then(|value| Vec::<HashMap<String, OwnedValue>>::try_from(value.clone()).ok())
        .unwrap_or_default()
        .iter()
        .map(|values| Indicator {
            icon: text(values, "icon"),
            overlay: text(values, "overlay"),
            dot: text(values, "dot"),
            label: text(values, "label"),
            tooltip: text(values, "tooltip"),
            badge: text(values, "badge"),
            attention: flag(values, "attention"),
            notice: flag(values, "notice"),
            severity: text(values, "severity"),
            class: text(values, "class"),
        })
        .collect();
    Some(AppletRow {
        action: action.to_owned(),
        connector: text(&values, "connector")?,
        position: text(&values, "position")?.parse().ok()?,
        zone: text(&values, "zone")?,
        index: u32::try_from(values.get("index")?).ok()?,
        name: text(&values, "name")?,
        kind: text(&values, "kind"),
        alive: flag(&values, "alive")?,
        own_view: flag(&values, "own-view")?,
        popover: flag(&values, "popover")?,
        indicators,
    })
}

fn sort(rows: &mut [AppletRow]) {
    let zone = |zone: &str| match zone {
        "left" => 0,
        "center" => 1,
        "right" => 2,
        _ => 3,
    };
    rows.sort_by(|a, b| {
        (&a.connector, a.position, zone(&a.zone), a.index).cmp(&(
            &b.connector,
            b.position,
            zone(&b.zone),
            b.index,
        ))
    });
}

fn rows(descriptions: &Descriptions) -> Vec<AppletRow> {
    let mut rows: Vec<_> = descriptions
        .iter()
        .filter(|(action, _)| action.starts_with("applet."))
        .filter_map(|(action, (_, _, state))| decode(action, state.first()?))
        .collect();
    sort(&mut rows);
    rows
}

fn resolve<'a>(
    rows: &'a [AppletRow],
    connector: &str,
    applet: &str,
    position: Option<Position>,
) -> Result<&'a AppletRow, Refused> {
    let matching: Vec<_> = rows
        .iter()
        .filter(|row| {
            row.connector == connector
                && row.name == applet
                && position.is_none_or(|position| row.position == position)
        })
        .collect();
    if matching.is_empty() {
        return Err(Refused(match position {
            Some(position) => format!("{connector} has no applet `{applet}` at {position}"),
            None => format!("{connector} has no applet `{applet}`"),
        }));
    }
    let mut positions: Vec<_> = matching.iter().map(|row| row.position).collect();
    positions.sort();
    positions.dedup();
    if positions.len() > 1 {
        return Err(Refused(format!(
            "{applet} is on {connector} {}; pass --position",
            positions
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" and ")
        )));
    }
    if matching.len() > 1 {
        return Err(Refused(format!(
            "{applet} appears more than once on {connector} {}",
            matching[0].position
        )));
    }
    Ok(matching[0])
}

fn target<'a>(
    rows: &'a [AppletRow],
    connector: &str,
    applet: &str,
    position: Option<Position>,
    item_input: bool,
) -> Result<&'a AppletRow, Refused> {
    let row = resolve(rows, connector, applet, position)?;
    if item_input && row.own_view {
        return Err(Refused(format!(
            "{applet} takes clicks per item; use `panel popover` instead"
        )));
    }
    Ok(row)
}

fn indicator_summary(row: &AppletRow) -> String {
    let value = if row.own_view {
        "(own view)".to_owned()
    } else if row.indicators.is_empty() {
        "—".to_owned()
    } else {
        row.indicators
            .iter()
            .map(|indicator| match (&indicator.icon, &indicator.label) {
                (Some(icon), Some(label)) => format!("{icon} \"{label}\""),
                (Some(icon), None) => icon.clone(),
                (None, Some(label)) => format!("\"{label}\""),
                (None, None) => "—".to_owned(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    if row.alive {
        value
    } else {
        format!("{value} (panicked)")
    }
}

fn cells(row: &AppletRow) -> [String; 9] {
    [
        row.connector.clone(),
        row.position.to_string(),
        row.zone.clone(),
        row.index.to_string(),
        row.name.clone(),
        row.kind.clone().unwrap_or_else(|| "—".into()),
        yes_no(row.alive).to_owned(),
        yes_no(row.popover).to_owned(),
        indicator_summary(row),
    ]
}

fn print_rows(rows: &[AppletRow], json: bool) -> Result<()> {
    if json {
        return emit(&rows);
    }
    render::print(
        &Table::default()
            .with_headers([
                "OUTPUT",
                "BAR",
                "ZONE",
                "#",
                "APPLET",
                "KIND",
                "ALIVE",
                "POPOVER",
                "INDICATORS",
            ])
            .with_empty("no applets")
            .with_rows(rows.iter().map(cells))
            .render(),
    )?;
    Ok(())
}

async fn proxy<'a>(connection: &'a Connection, app_id: &'a str) -> Result<GtkActionsProxy<'a>> {
    let path = object_path(app_id);
    within(
        GtkActionsProxy::builder(connection)
            .destination(app_id)?
            .path(path)?
            .cache_properties(CacheProperties::No)
            .build(),
    )
    .await
}

fn print_watch_row(row: &AppletRow, json: bool) -> Result<render::Flow> {
    if json {
        render::print(&serde_json::to_string(row)?)
    } else {
        render::print(&Table::default().with_row(cells(row)).render())
    }
}

fn removal_line(visible: &mut Vec<AppletRow>, action: &str, json: bool) -> Option<String> {
    let serial = action.strip_prefix("applet.")?;
    visible.retain(|row| row.action != action);
    Some(if json {
        serde_json::json!({"removed": action}).to_string()
    } else {
        format!("{serial} removed")
    })
}

async fn applets(connection: &Connection, app_id: &str, watch: bool, json: bool) -> Result<()> {
    if !watch {
        let actions = proxy(connection, app_id).await?;
        return print_rows(&rows(&within(actions.describe_all()).await?), json);
    }
    let dbus = within(zbus::fdo::DBusProxy::new(connection)).await?;
    let mut owners =
        Box::pin(within(dbus.receive_name_owner_changed_with_args(&[(0, app_id)])).await?);
    let owner = within(dbus.get_name_owner(app_id.try_into()?)).await?;
    let followed = owner.clone();
    let mut lost = Box::pin(async move {
        loop {
            let Some(change) = std::future::poll_fn(|cx| owners.as_mut().poll_next(cx)).await
            else {
                return Err(Gone.into());
            };
            if change
                .args()?
                .old_owner()
                .as_ref()
                .is_some_and(|old| old.as_str() == followed.as_str())
            {
                return Err(Gone.into());
            }
        }
    });
    let actions = tokio::select! {
        biased;
        result = &mut lost => return result,
        actions = within(GtkActionsProxy::builder(connection)
            .destination(owner)?
            .path(object_path(app_id))?
            .cache_properties(CacheProperties::No)
            .build()) => actions?,
    };
    let mut changes = tokio::select! {
        biased;
        result = &mut lost => return result,
        changes = within(actions.receive_changed()) => Box::pin(changes?),
    };
    let descriptions = tokio::select! {
        biased;
        result = &mut lost => return result,
        descriptions = within(actions.describe_all()) => descriptions?,
    };
    let mut visible = rows(&descriptions);
    for row in &visible {
        if let render::Flow::Stop = print_watch_row(row, json)? {
            return Ok(());
        }
    }
    loop {
        tokio::select! {
            biased;
            result = &mut lost => return result,
            change = std::future::poll_fn(|cx| changes.as_mut().poll_next(cx)) => {
                let Some(change) = change else { return Err(Gone.into()) };
                let args = change.args()?;
                for action in args.removals() {
                    if let Some(line) = removal_line(&mut visible, action, json)
                        && let render::Flow::Stop = render::print(&line)? { return Ok(()) }
                }
                let mut updated = Vec::new();
                for (action, value) in args.state_changes().iter() {
                    if visible.iter().any(|row| row.action == *action)
                        && let Some(row) = decode(action, value) {
                        updated.push(row);
                    }
                }
                for (action, (_, _, state)) in args.additions().iter() {
                    if action.starts_with("applet.") && let Some(row) = state.first().and_then(|value| decode(action, value)) {
                        updated.push(row);
                    }
                }
                sort(&mut updated);
                for row in updated {
                    visible.retain(|existing| existing.action != row.action);
                    if let render::Flow::Stop = print_watch_row(&row, json)? { return Ok(()) }
                    visible.push(row);
                }
                sort(&mut visible);
            }
        }
    }
}

pub async fn panel(
    connection: &Connection,
    app_id: &str,
    command: PanelCommand,
    json: bool,
) -> Result<()> {
    match command {
        PanelCommand::List { watch } => return panel_list(connection, watch, json).await,
        PanelCommand::Reload => return panel_reload(connection, app_id).await,
        PanelCommand::Applets { watch } => return applets(connection, app_id, watch, json).await,
        _ => {}
    }
    let actions = proxy(connection, app_id).await?;
    let (action, parameters) = match command {
        PanelCommand::Popovers(PopoversCommand::Close) => ("close-popovers".to_owned(), Vec::new()),
        PanelCommand::Click {
            connector,
            applet,
            button,
            position,
        } => {
            let rows = rows(&within(actions.describe_all()).await?);
            let row = target(&rows, &connector, &applet, position, true)?;
            (
                row.action.clone(),
                vec![Value::from(("click", Value::from(button)))],
            )
        }
        PanelCommand::Scroll {
            connector,
            applet,
            direction,
            steps,
            position,
        } => {
            let rows = rows(&within(actions.describe_all()).await?);
            let row = target(&rows, &connector, &applet, position, true)?;
            let (dx, dy) = direction.delta();
            for _ in 0..steps {
                within(actions.activate(
                    &row.action,
                    vec![Value::from(("scroll", Value::from((dx, dy))))],
                    HashMap::new(),
                ))
                .await?;
            }
            return Ok(());
        }
        PanelCommand::List { .. } | PanelCommand::Reload | PanelCommand::Applets { .. } => {
            unreachable!()
        }
    };
    within(actions.activate(&action, parameters, HashMap::new())).await
}

fn resolve_live<'a>(
    rows: &'a [AppletRow],
    target: &AppletTarget,
) -> Result<&'a AppletRow, Refused> {
    let mut matching = rows.iter().filter(|row| {
        row.name == target.name
            && target
                .output
                .as_ref()
                .is_none_or(|output| row.connector == *output)
            && target.edge.is_none_or(|edge| row.position == edge)
    });
    let row = matching.next().ok_or_else(|| {
        Refused(format!(
            "no matching applet `{}`; use applet list",
            target.name
        ))
    })?;
    if matching.next().is_some() {
        return Err(Refused(format!(
            "applet `{}` is ambiguous; pass --output and --edge",
            target.name
        )));
    }
    Ok(row)
}

pub async fn applet_live(
    connection: &Connection,
    app_id: &str,
    command: AppletCommand,
    json: bool,
) -> Result<()> {
    if let AppletCommand::List { watch, .. } = command {
        return applets(connection, app_id, watch, json).await;
    }
    let actions = proxy(connection, app_id).await?;
    let (target, verb, argument, steps) = match command {
        AppletCommand::Inspect { target, .. } => {
            let rows = rows(&within(actions.describe_all()).await?);
            let row = resolve_live(&rows, &target)?;
            return if json {
                emit(row)
            } else {
                print_rows(std::slice::from_ref(row), false)
            };
        }
        AppletCommand::Click { target, button } => (target, "click", Value::from(button), 1),
        AppletCommand::Scroll {
            target,
            direction,
            steps,
        } => (target, "scroll", Value::from(direction.delta()), steps),
        AppletCommand::Open(target) => (target, "open", Value::from(0_u32), 1),
        AppletCommand::Close(target) => (target, "close", Value::from(0_u32), 1),
        AppletCommand::Toggle(target) => (target, "toggle", Value::from(0_u32), 1),
        _ => return Err(Refused("expected a live applet command".into()).into()),
    };
    let rows = rows(&within(actions.describe_all()).await?);
    let row = resolve_live(&rows, &target)?;
    if row.own_view && matches!(verb, "click" | "scroll") {
        return Err(Refused(format!(
            "{} takes input per item; use applet open",
            target.name
        ))
        .into());
    }
    for _ in 0..steps {
        within(actions.activate(
            &row.action,
            vec![Value::from((verb, argument.clone()))],
            HashMap::new(),
        ))
        .await?;
    }
    Ok(())
}

pub(super) async fn panel_names(connection: &Connection) -> Result<Vec<String>> {
    let dbus = within(zbus::fdo::DBusProxy::new(connection)).await?;
    let owned = within(dbus.list_names()).await?;
    let mut panels = BTreeSet::new();
    for name in &owned {
        if let Some(name) = name.as_str().strip_suffix(".AppletHost")
            && owned.iter().any(|owned| owned.as_str() == name)
        {
            panels.insert(name.to_owned());
        }
        if name.as_str() == "me.aresa.GlimpsePanel" {
            panels.insert(name.to_string());
        }
    }
    Ok(panels.into_iter().collect())
}

#[derive(Serialize)]
struct PanelRow {
    app_id: String,
    #[serde(skip)]
    owner: String,
    pid: Option<u32>,
    unit: Option<String>,
    version: Option<String>,
    config: Option<(bool, String)>,
    bars: Option<Vec<BarRow>>,
    applet_count: Option<usize>,
    applet_host: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct BarRow {
    index: u32,
    connector: String,
    position: Position,
}

fn decode_bars(descriptions: &Descriptions) -> Option<Vec<BarRow>> {
    let state = descriptions.get("bars")?.2.first()?.clone();
    Vec::<(u32, String, String)>::try_from(state)
        .ok()?
        .into_iter()
        .map(|(index, connector, position)| {
            Some(BarRow {
                index,
                connector: safe(&connector),
                position: position.parse().ok()?,
            })
        })
        .collect()
}

async fn panel_row(
    connection: &Connection,
    dbus: &zbus::fdo::DBusProxy<'_>,
    app_id: String,
    owner: zbus::names::OwnedUniqueName,
    manager: Option<&glimpse_dbus::systemd1::Systemd1ManagerProxy<'_>>,
) -> Result<PanelRow> {
    use glimpse_dbus::systemd1::Systemd1UnitProxy;
    let pid = within(dbus.get_connection_unix_process_id(owner.as_str().try_into()?))
        .await
        .ok();
    let unit = async {
        let manager = manager?;
        let path = within(manager.get_unit_by_pid(pid?)).await.ok()?;
        let unit = within(
            Systemd1UnitProxy::builder(connection)
                .destination(manager.inner().destination().as_str().to_owned())
                .ok()?
                .path(path)
                .ok()?
                .cache_properties(CacheProperties::No)
                .build(),
        )
        .await
        .ok()?;
        within(unit.id()).await.ok()
    }
    .await;
    let descriptions = async {
        let actions = within(
            GtkActionsProxy::builder(connection)
                .destination(owner.clone())
                .ok()?
                .path(object_path(&app_id))
                .ok()?
                .cache_properties(CacheProperties::No)
                .build(),
        )
        .await
        .ok()?;
        within(actions.describe_all()).await.ok()
    }
    .await;
    let version = descriptions
        .as_ref()
        .and_then(|descriptions| descriptions.get("version"))
        .and_then(|(_, _, state)| state.first())
        .and_then(|value| String::try_from(value.clone()).ok())
        .map(|value| safe(&value));
    let config = descriptions
        .as_ref()
        .and_then(|descriptions| descriptions.get("config-status"))
        .and_then(|(_, _, state)| state.first())
        .and_then(|value| zbus::zvariant::Structure::try_from(value.clone()).ok())
        .and_then(|value| <(bool, String)>::try_from(value).ok())
        .map(|(ok, message)| (ok, safe(&message)));
    let bars = descriptions.as_ref().and_then(decode_bars);
    let applet_count = descriptions
        .as_ref()
        .map(|descriptions| rows(descriptions).len());
    let host = glimpse_dbus::applet_host::applet_host_name(&app_id);
    let host = within(dbus.get_name_owner(host.as_str().try_into()?))
        .await
        .ok();
    let applet_host = match (pid, host) {
        (Some(pid), Some(host)) => {
            within(dbus.get_connection_unix_process_id(host.as_str().try_into()?))
                .await
                .is_ok_and(|host_pid| host_pid == pid)
        }
        _ => false,
    };
    Ok(PanelRow {
        app_id,
        owner: owner.to_string(),
        pid,
        unit,
        version,
        config,
        bars,
        applet_count,
        applet_host,
    })
}

async fn panel_rows(connection: &Connection) -> Result<Vec<PanelRow>> {
    use glimpse_dbus::systemd1::Systemd1ManagerProxy;
    let dbus = within(zbus::fdo::DBusProxy::new(connection)).await?;
    let manager = match within(dbus.get_name_owner("org.freedesktop.systemd1".try_into()?)).await {
        Ok(owner) => within(
            Systemd1ManagerProxy::builder(connection)
                .destination(owner)?
                .cache_properties(CacheProperties::No)
                .build(),
        )
        .await
        .ok(),
        Err(_) => None,
    };
    let mut result = Vec::new();
    for app_id in panel_names(connection).await? {
        let Ok(owner) = within(dbus.get_name_owner(app_id.as_str().try_into()?)).await else {
            continue;
        };
        result.push(panel_row(connection, &dbus, app_id, owner, manager.as_ref()).await?);
    }
    Ok(result)
}

fn print_panels(rows: &[PanelRow], json: bool, watch: bool) -> Result<render::Flow> {
    if json {
        return render::print(&if watch {
            serde_json::to_string(rows)?
        } else {
            serde_json::to_string_pretty(rows)?
        });
    }
    render::print(
        &Table::default()
            .with_headers([
                "APP ID", "PID", "UNIT", "VERSION", "CONFIG", "BARS", "APPLETS", "HOST",
            ])
            .with_empty("no panels")
            .with_rows(rows.iter().map(|row| {
                [
                    safe(&row.app_id),
                    row.pid
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "-".into()),
                    row.unit.as_deref().map(safe).unwrap_or_else(|| "-".into()),
                    row.version.clone().unwrap_or_else(|| "unknown".into()),
                    row.config
                        .as_ref()
                        .map(|(ok, message)| {
                            if *ok {
                                "ok".into()
                            } else {
                                format!("rejected: {message}")
                            }
                        })
                        .unwrap_or_else(|| "unknown".into()),
                    row.bars
                        .as_ref()
                        .map(|bars| {
                            bars.iter()
                                .map(|bar| {
                                    format!("{}:{}:{}", bar.index, bar.connector, bar.position)
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_else(|| "unknown".into()),
                    row.applet_count
                        .map(|count| count.to_string())
                        .unwrap_or_else(|| "unknown".into()),
                    yes_no(row.applet_host).into(),
                ]
            }))
            .render(),
    )
}

async fn panel_list(connection: &Connection, watch: bool, json: bool) -> Result<()> {
    let dbus = within(zbus::fdo::DBusProxy::new(connection)).await?;
    if !watch {
        print_panels(&panel_rows(connection).await?, json, false)?;
        return Ok(());
    }
    let mut owners = Box::pin(within(dbus.receive_name_owner_changed()).await?);
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.gtk.Actions")?
        .member("Changed")?
        .build();
    let mut changes = Box::pin(
        within(zbus::MessageStream::for_match_rule(
            rule,
            connection,
            Some(64),
        ))
        .await?,
    );
    let mut visible = panel_rows(connection).await?;
    if let render::Flow::Stop = print_panels(&visible, json, true)? {
        return Ok(());
    }
    loop {
        let refresh = tokio::select! {
            change = std::future::poll_fn(|cx| changes.as_mut().poll_next(cx)) => {
                let Some(change) = change else { return Err(Gone.into()); };
                let change = change?;
                let header = change.header();
                visible.iter().any(|row| header.sender().is_some_and(|sender| sender.as_str() == row.owner) && header.path().is_some_and(|path| path.as_str() == object_path(&row.app_id)))
            }
            owner = std::future::poll_fn(|cx| owners.as_mut().poll_next(cx)) => {
                let Some(owner) = owner else { return Err(Gone.into()); };
                let args = owner.args()?;
                let name = args.name().as_str();
                name.ends_with(".AppletHost")
                    || name == "me.aresa.GlimpsePanel"
                    || visible.iter().any(|row| row.app_id == name)
                    || (!name.starts_with(':')
                        && within(dbus.list_names()).await?.iter().any(|host| {
                            host.as_str().strip_suffix(".AppletHost") == Some(name)
                        }))
            }
        };
        if refresh {
            for _ in 0..64 {
                match std::future::poll_fn(|cx| {
                    std::task::Poll::Ready(changes.as_mut().poll_next(cx))
                })
                .await
                {
                    std::task::Poll::Ready(Some(change)) => {
                        change?;
                    }
                    std::task::Poll::Ready(None) => return Err(Gone.into()),
                    std::task::Poll::Pending => break,
                }
            }
            visible = panel_rows(connection).await?;
            if let render::Flow::Stop = print_panels(&visible, json, true)? {
                return Ok(());
            }
        }
    }
}

async fn panel_reload(connection: &Connection, app_id: &str) -> Result<()> {
    let dbus = within(zbus::fdo::DBusProxy::new(connection)).await?;
    let owner = within(dbus.get_name_owner(app_id.try_into()?)).await?;
    let pid = within(dbus.get_connection_unix_process_id(owner.as_str().try_into()?)).await?;
    let pid = i32::try_from(pid).map_err(|_| Refused("panel PID is out of range".into()))?;
    if pid <= 0 {
        return Err(Refused("invalid panel PID".into()).into());
    }
    if unsafe { libc::kill(pid, libc::SIGHUP) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
