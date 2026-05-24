use std::{path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use glimpse_core::{
    ipc::{protocol::unescape, shell_socket_path},
    services::{
        framework::ServiceCommand,
        location::{self, LocationHandle},
    },
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};
use tokio_util::sync::CancellationToken;

pub async fn run(location: LocationHandle, cancel: CancellationToken) {
    let mut retry_delay = Duration::from_secs(1);
    loop {
        if cancel.is_cancelled() {
            break;
        }

        match connect_once(shell_socket_path(), location.clone(), cancel.clone()).await {
            Ok(()) => retry_delay = Duration::from_secs(1),
            Err(error) => {
                tracing::warn!(
                    %error,
                    retry_delay_ms = retry_delay.as_millis(),
                    "sunset: shell location bridge disconnected; retrying"
                );
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(retry_delay) => {}
                }
                retry_delay = (retry_delay * 2).min(Duration::from_secs(30));
            }
        }
    }
}

pub fn refresh_shell_location() {
    tokio::spawn(async {
        if let Err(error) =
            dispatch_shell_command(shell_socket_path(), "refresh service=location").await
        {
            tracing::debug!(%error, "sunset: failed to request shell location refresh");
        }
    });
}

pub async fn set_shell_location(latitude: f64, longitude: f64) -> Result<()> {
    dispatch_shell_command(
        shell_socket_path(),
        &format!("set_location lat={latitude} lon={longitude}"),
    )
    .await
}

async fn connect_once(
    path: impl AsRef<Path>,
    location: LocationHandle,
    cancel: CancellationToken,
) -> Result<()> {
    let stream = UnixStream::connect(path.as_ref())
        .await
        .with_context(|| format!("connect to shell IPC at {}", path.as_ref().display()))?;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    read_hello(&mut lines).await?;
    writer.write_all(b"status\n").await?;
    let status = read_required_line(&mut lines, "shell status ack").await?;
    apply_shell_status_line(&status, &location).await?;

    writer.write_all(b"subscribe location.updated\n").await?;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            line = lines.next_line() => {
                let Some(line) = line? else {
                    bail!("shell IPC closed");
                };
                apply_shell_event_line(&line, &location).await?;
            }
        }
    }
}

async fn dispatch_shell_command(path: impl AsRef<Path>, command: &str) -> Result<()> {
    let stream = UnixStream::connect(path.as_ref())
        .await
        .with_context(|| format!("connect to shell IPC at {}", path.as_ref().display()))?;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    read_hello(&mut lines).await?;
    writer.write_all(format!("{command}\n").as_bytes()).await?;
    let ack = read_required_line(&mut lines, "shell command ack").await?;
    if !ack.starts_with("ack ok=true") {
        bail!("shell rejected command '{command}': {ack}");
    }
    Ok(())
}

async fn read_hello(
    lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
) -> Result<()> {
    let hello = read_required_line(lines, "shell hello").await?;
    if !hello.starts_with("hello ") {
        bail!("unexpected shell greeting: {hello}");
    }
    Ok(())
}

async fn read_required_line(
    lines: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    context: &str,
) -> Result<String> {
    lines
        .next_line()
        .await?
        .ok_or_else(|| anyhow::anyhow!("shell IPC closed before {context}"))
}

async fn apply_shell_status_line(line: &str, location: &LocationHandle) -> Result<()> {
    let (name, fields) = parse_ipc_line(line);
    if name == "ack" && field(&fields, "ok") == Some("true") {
        if let Some(command) =
            location_command_from_fields(&fields, "location_latitude", "location_longitude")
                .map_err(|error| anyhow::anyhow!(error))?
        {
            location.send(ServiceCommand::Command(command)).await?;
        }
    }
    Ok(())
}

async fn apply_shell_event_line(line: &str, location: &LocationHandle) -> Result<()> {
    let (name, fields) = parse_ipc_line(line);
    if name == "location.updated"
        && let Some(command) = location_command_from_fields(&fields, "latitude", "longitude")
            .map_err(|error| anyhow::anyhow!(error))?
    {
        location.send(ServiceCommand::Command(command)).await?;
    }
    Ok(())
}

fn parse_ipc_line(line: &str) -> (String, Vec<(String, String)>) {
    let mut tokens = line.split_ascii_whitespace();
    let name = tokens.next().unwrap_or_default().to_owned();
    let fields = tokens
        .filter_map(|token| {
            let (key, value) = token.split_once('=')?;
            Some((key.to_owned(), unescape(value)))
        })
        .collect();
    (name, fields)
}

pub(crate) fn location_command_from_fields(
    fields: &[(String, String)],
    latitude_key: &str,
    longitude_key: &str,
) -> Result<Option<location::Command>, String> {
    let Some(latitude) = field(fields, latitude_key) else {
        return Ok(None);
    };
    let Some(longitude) = field(fields, longitude_key) else {
        return Ok(None);
    };

    let latitude = latitude
        .parse::<f64>()
        .map_err(|_| format!("{latitude_key} must be a number"))?;
    let longitude = longitude
        .parse::<f64>()
        .map_err(|_| format!("{longitude_key} must be a number"))?;
    if !(-90.0..=90.0).contains(&latitude) {
        return Err(format!("{latitude_key} must be in [-90, 90]"));
    }
    if !(-180.0..=180.0).contains(&longitude) {
        return Err(format!("{longitude_key} must be in [-180, 180]"));
    }

    Ok(Some(location::Command::SetManual(latitude, longitude)))
}

fn field<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(field, _)| field == key)
        .map(|(_, value)| value.as_str())
}

#[cfg(test)]
mod tests {
    use super::{location_command_from_fields, parse_ipc_line};
    use glimpse_core::services::location;

    #[test]
    fn shell_status_fields_parse_to_location_command() {
        let command = location_command_from_fields(
            &[
                ("ok".into(), "true".into()),
                ("location_latitude".into(), "52.2297".into()),
                ("location_longitude".into(), "21.0122".into()),
            ],
            "location_latitude",
            "location_longitude",
        )
        .expect("fields should parse");

        assert!(matches!(
            command,
            Some(location::Command::SetManual(52.2297, 21.0122))
        ));
    }

    #[test]
    fn shell_location_event_fields_parse_to_location_command() {
        let command = location_command_from_fields(
            &[
                ("latitude".into(), "52.2297".into()),
                ("longitude".into(), "21.0122".into()),
            ],
            "latitude",
            "longitude",
        )
        .expect("fields should parse");

        assert!(matches!(
            command,
            Some(location::Command::SetManual(52.2297, 21.0122))
        ));
    }

    #[test]
    fn shell_location_parser_rejects_invalid_coordinates() {
        let error = location_command_from_fields(
            &[
                ("latitude".into(), "91".into()),
                ("longitude".into(), "21.0122".into()),
            ],
            "latitude",
            "longitude",
        )
        .expect_err("latitude should be rejected");

        assert_eq!(error, "latitude must be in [-90, 90]");
    }

    #[test]
    fn ipc_line_parser_unescapes_fields() {
        let (name, fields) = parse_ipc_line("location.updated latitude=52.2 longitude=21\\s01");

        assert_eq!(name, "location.updated");
        assert_eq!(
            fields,
            vec![
                ("latitude".into(), "52.2".into()),
                ("longitude".into(), "21 01".into()),
            ]
        );
    }
}
