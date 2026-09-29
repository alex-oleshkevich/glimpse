use std::collections::HashMap;

use anyhow::Result;
use glimpse_dbus::gtk_actions::{GtkActionsProxy, object_path};
use serde::Serialize;
use zbus::Connection;
use zbus::export::futures_core::Stream;
use zbus::proxy::CacheProperties;
use zbus::zvariant::{OwnedValue, Value};

use super::{emit, safe, within, yes_no};
use crate::cli::{PanelCommand, PopoverCommand, PopoversCommand, Position};
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
        "start" => 0,
        "center" => 1,
        "end" => 2,
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
        &Table::new()
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
        render::print(&Table::new().with_row(cells(row)).render())
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

async fn applets(
    connection: &Connection,
    actions: &GtkActionsProxy<'_>,
    app_id: &str,
    watch: bool,
    json: bool,
) -> Result<()> {
    if !watch {
        return print_rows(&rows(&within(actions.describe_all()).await?), json);
    }
    let mut changes = Box::pin(within(actions.receive_changed()).await?);
    let dbus = within(zbus::fdo::DBusProxy::new(connection)).await?;
    let mut owners =
        Box::pin(within(dbus.receive_name_owner_changed_with_args(&[(0, app_id)])).await?);
    let mut visible = rows(&within(actions.describe_all()).await?);
    for row in &visible {
        if let render::Flow::Stop = print_watch_row(row, json)? {
            return Ok(());
        }
    }
    loop {
        tokio::select! {
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
            owner = std::future::poll_fn(|cx| owners.as_mut().poll_next(cx)) => {
                let Some(owner) = owner else { return Err(Gone.into()) };
                if owner.args()?.new_owner().is_none() { return Err(Gone.into()) }
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
    let actions = proxy(connection, app_id).await?;
    let (action, parameters) = match command {
        PanelCommand::Applets { watch } => {
            return applets(connection, &actions, app_id, watch, json).await;
        }
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
        PanelCommand::Popover(popover) => {
            let (verb, connector, applet, position) = match popover {
                PopoverCommand::Open(target) => {
                    ("open", target.connector, target.applet, target.position)
                }
                PopoverCommand::Close(target) => {
                    ("close", target.connector, target.applet, target.position)
                }
                PopoverCommand::Toggle(target) => {
                    ("toggle", target.connector, target.applet, target.position)
                }
            };
            let rows = rows(&within(actions.describe_all()).await?);
            let row = target(&rows, &connector, &applet, position, false)?;
            (
                row.action.clone(),
                vec![Value::from((verb, Value::from(0_u32)))],
            )
        }
    };
    within(actions.activate(&action, parameters, HashMap::new())).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_dbus::testing::PrivateBus;

    fn value(value: Value<'_>) -> OwnedValue {
        OwnedValue::try_from(value).unwrap()
    }

    fn row(connector: &str, position: Position, name: &str, own_view: bool) -> AppletRow {
        AppletRow {
            action: "applet.1".into(),
            connector: connector.into(),
            position,
            zone: "start".into(),
            index: 0,
            name: name.into(),
            kind: None,
            alive: true,
            own_view,
            popover: false,
            indicators: Vec::new(),
        }
    }

    #[test]
    fn resolves_placements_and_rejects_own_view_item_input() {
        let rows = [
            row("DP-1", Position::Top, "clock", true),
            row("DP-1", Position::Bottom, "clock", false),
        ];
        assert_eq!(
            resolve(&rows, "DP-2", "clock", None).unwrap_err().0,
            "DP-2 has no applet `clock`"
        );
        assert_eq!(
            resolve(&rows, "DP-1", "clock", None).unwrap_err().0,
            "clock is on DP-1 top and bottom; pass --position"
        );
        assert_eq!(
            resolve(&rows, "DP-1", "clock", Some(Position::Bottom))
                .unwrap()
                .position,
            Position::Bottom
        );
        assert_eq!(
            target(&rows, "DP-1", "clock", Some(Position::Top), true)
                .unwrap_err()
                .0,
            "clock takes clicks per item; use `panel popover` instead"
        );
        assert!(target(&rows, "DP-1", "clock", Some(Position::Top), false).is_ok());
    }

    #[test]
    fn rejects_duplicate_applet_in_one_position() {
        let mut second = row("DP-1", Position::Top, "clock", false);
        second.zone = "center".into();
        let rows = [row("DP-1", Position::Top, "clock", false), second];
        for position in [None, Some(Position::Top)] {
            assert_eq!(
                resolve(&rows, "DP-1", "clock", position).unwrap_err().0,
                "clock appears more than once on DP-1 top"
            );
        }
    }

    #[test]
    fn removal_drops_the_applet_and_reports_its_serial() {
        let mut visible = vec![row("DP-1", Position::Top, "clock", false)];
        assert_eq!(
            removal_line(&mut visible, "applet.1", true).as_deref(),
            Some(r#"{"removed":"applet.1"}"#)
        );
        assert!(visible.is_empty());
        assert_eq!(
            removal_line(&mut visible, "applet.1", false).as_deref(),
            Some("1 removed")
        );
    }

    #[test]
    fn decodes_state_and_sorts_by_placement() {
        let indicator = HashMap::from([
            ("icon".to_owned(), value(Value::from("weather-clear"))),
            ("label".to_owned(), value(Value::from("Sunny"))),
            ("attention".to_owned(), value(Value::from(true))),
        ]);
        let state = |zone: &str, index: u32| {
            value(Value::from(HashMap::from([
                ("connector".to_owned(), value(Value::from("DP-1"))),
                ("position".to_owned(), value(Value::from("top"))),
                ("zone".to_owned(), value(Value::from(zone))),
                ("index".to_owned(), value(Value::from(index))),
                ("name".to_owned(), value(Value::from("clock"))),
                ("kind".to_owned(), value(Value::from("builtin"))),
                ("alive".to_owned(), value(Value::from(true))),
                ("own-view".to_owned(), value(Value::from(false))),
                ("popover".to_owned(), value(Value::from(true))),
                (
                    "indicators".to_owned(),
                    value(Value::from(vec![indicator.clone()])),
                ),
            ])))
        };
        let mut rows = [
            decode("applet.2", &state("center", 0)).unwrap(),
            decode("applet.1", &state("start", 2)).unwrap(),
        ];
        sort(&mut rows);
        assert_eq!(rows[0].zone, "start");
        assert_eq!(rows[1].zone, "center");
        assert_eq!(rows[0].indicators[0].icon.as_deref(), Some("weather-clear"));
        assert_eq!(rows[0].indicators[0].label.as_deref(), Some("Sunny"));
        assert_eq!(rows[0].indicators[0].attention, Some(true));
        assert_eq!(rows[0].indicators[0].badge, None);
        let sparse = value(Value::from(HashMap::from([
            ("connector".to_owned(), value(Value::from("DP-1"))),
            ("position".to_owned(), value(Value::from("top"))),
            ("zone".to_owned(), value(Value::from("start"))),
            ("index".to_owned(), value(Value::from(0_u32))),
            ("name".to_owned(), value(Value::from("clock"))),
            ("kind".to_owned(), value(Value::from("builtin"))),
            ("alive".to_owned(), value(Value::from(true))),
            ("own-view".to_owned(), value(Value::from(false))),
            ("popover".to_owned(), value(Value::from(false))),
        ])));
        let sparse = decode("applet.3", &sparse).unwrap();
        assert_eq!(sparse.kind.as_deref(), Some("builtin"));
        assert!(sparse.indicators.is_empty());
    }

    #[tokio::test]
    async fn missing_panel_is_unreachable() {
        let bus = PrivateBus::start();
        let client = bus.connection().await;
        let error = panel(
            &client,
            "me.aresa.GlimpsePanel",
            PanelCommand::Applets { watch: false },
            false,
        )
        .await
        .unwrap_err();
        assert_eq!(
            crate::errors::exit(&error),
            crate::errors::Exit::Unreachable
        );
    }
}
