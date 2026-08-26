use std::{path::PathBuf, time::Duration};

use anyhow::Result;
use glimpse_contracts::{ServiceState, SystemServices};
use glimpse_ipc::Client;

use super::services::{SERVICES, described};
use super::write_line;
use crate::render::{Section, Table, stacked, styled};

/// Diagnoses a missing daemon, so it connects for itself and reports the failure rather than
/// exiting on it. Findings are the output; the exit code stays 0 because the check succeeded.
/// Short and fixed, not `--timeout`: the answer here is whether anything is listening, and a
/// diagnostic that hangs for the request timeout is the thing it was meant to diagnose.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

pub async fn doctor(socket: glimpse_utils::SocketArg, config: Option<PathBuf>) -> Result<()> {
    let path = glimpse_ipc::socket_path(socket.as_deref())?;
    let client = Client::connect(&path, CONNECT_TIMEOUT).await;

    let mut blocks = vec![socket_section(&path, &client), config_section(config)];

    if let Ok(client) = &client {
        blocks.push(services_section(client).await);
    }

    write_line(&stacked(blocks))?;
    Ok(())
}

fn socket_section(
    path: &std::path::Path,
    client: &Result<Client, glimpse_ipc::ConnectError>,
) -> String {
    let section = Section::new("Socket").with(
        Table::new()
            .with_row(["path".to_owned(), path.display().to_string()])
            .with_row([
                "daemon".to_owned(),
                match client {
                    Ok(_) => styled::good("answering"),
                    Err(error) => styled::bad(&error.to_string()),
                },
            ])
            .render(),
    );

    match client {
        Ok(_) => section.render(),
        Err(_) => section
            .with_note("every command needing the daemon will fail")
            .render(),
    }
}

fn config_section(config: Option<PathBuf>) -> String {
    let files = match glimpse_config::resolved_files(config.as_deref()) {
        Ok(files) => files,
        Err(error) => {
            return Section::new("Configuration")
                .with(styled::bad(&error.to_string()))
                .render();
        }
    };

    let section = Section::new("Configuration").with(
        Table::new()
            .with_empty("no configuration file, so every default applies")
            .with_rows(files.iter().map(|file| [file.display().to_string()]))
            .render(),
    );

    match glimpse_config::load(config.as_deref()) {
        Ok(_) => section.with_note("the stack loads").render(),
        Err(error) => section
            .with(styled::bad(&error.to_string()))
            .with_note("the daemon falls back to defaults and logs this")
            .render(),
    }
}

async fn services_section(client: &Client) -> String {
    let Ok(Some(event)) = client.get(SERVICES).await else {
        return Section::new("Services")
            .with(styled::warn("the daemon has not reported any yet"))
            .render();
    };

    let Ok(report) = serde_json::from_value::<SystemServices>(event.data) else {
        return Section::new("Services")
            .with(styled::bad("`system.services` did not decode"))
            .render();
    };

    let section = Section::new("Services").with(
        Table::new()
            .with_headers(["SERVICE", "STATE", "DETAIL"])
            .with_empty("the daemon has no registered services")
            .with_rows(report.services.iter().map(|(name, state)| {
                let (state, detail) = described(state);
                [name.clone(), state, styled::key(&detail)]
            }))
            .render(),
    );

    match unhealthy(&report) {
        Some(note) => section.with_note(&note).render(),
        None => section.render(),
    }
}

/// The count worth reading twice. A healthy daemon gets no note rather than a line saying so — the
/// table above already said it.
fn unhealthy(report: &SystemServices) -> Option<String> {
    let mut degraded = 0;
    let mut stopped = 0;
    for state in report.services.values() {
        match state {
            ServiceState::Degraded { .. } => degraded += 1,
            ServiceState::Stopped { .. } => stopped += 1,
            _ => {}
        }
    }

    match (degraded, stopped) {
        (0, 0) => None,
        (0, stopped) => Some(format!("{stopped} stopped")),
        (degraded, 0) => Some(format!("{degraded} degraded")),
        (degraded, stopped) => Some(format!("{degraded} degraded, {stopped} stopped")),
    }
}
