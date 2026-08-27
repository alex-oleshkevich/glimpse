use std::path::PathBuf;

use anyhow::Result;
use glimpse_contracts::{ServiceState, SystemServices};
use glimpse_ipc::Client;

use super::services::{SERVICES, described};
use crate::render::{self, Section, Table, styled};

/// Diagnoses a missing daemon, so it connects for itself and reports the failure rather than
/// exiting on it. Findings are the output; the exit code stays 0 because the check succeeded.
pub async fn doctor(socket: glimpse_utils::SocketArg, config: Option<PathBuf>) -> Result<()> {
    let path = glimpse_ipc::socket_path(socket.as_deref())?;
    let client = Client::connect(&path).await;

    let mut sections = vec![socket_section(&path, &client), config_section(config)];

    if let Ok(client) = &client {
        sections.push(services_section(client).await);
    }

    for (index, section) in sections.into_iter().enumerate() {
        // A blank line between blocks, and none trailing the last.
        if index > 0 {
            render::print("")?;
        }
        section.print()?;
    }
    Ok(())
}

fn socket_section(
    path: &std::path::Path,
    client: &Result<Client, glimpse_ipc::ConnectError>,
) -> Section {
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
        Ok(_) => section,
        Err(_) => section.with_note("every command needing the daemon will fail"),
    }
}

fn config_section(config: Option<PathBuf>) -> Section {
    let files = match glimpse_config::resolved_files(config.as_deref()) {
        Ok(files) => files,
        Err(error) => {
            return Section::new("Configuration").with(styled::bad(&error.to_string()));
        }
    };

    let section = Section::new("Configuration").with(
        Table::new()
            .with_empty("no configuration file, so every default applies")
            .with_rows(files.iter().map(|file| [file.display().to_string()]))
            .render(),
    );

    match glimpse_config::load(config.as_deref()) {
        Ok(_) => section.with_note("the stack loads"),
        Err(error) => section
            .with(styled::bad(&error.to_string()))
            .with_note("the daemon falls back to defaults and logs this"),
    }
}

async fn services_section(client: &Client) -> Section {
    let Ok(Some(event)) = client.get(SERVICES).await else {
        return Section::new("Services").with(styled::warn("the daemon has not reported any yet"));
    };

    let Ok(report) = serde_json::from_value::<SystemServices>(event.data) else {
        return Section::new("Services").with(styled::bad("`system.services` did not decode"));
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
        Some(note) => section.with_note(&note),
        None => section,
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
