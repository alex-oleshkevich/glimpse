use std::{path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use glimpse_core::ipc::{protocol::unescape, shell_socket_path};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};
use tokio_util::sync::CancellationToken;

use crate::app::{AppCommand, WeatherDisplay};

pub(crate) const RETRY_DELAY: Duration = Duration::from_secs(5);

pub(crate) fn spawn(sender: relm4::Sender<AppCommand>, cancel: CancellationToken) {
    let task_cancel = cancel.child_token();
    tokio::spawn(async move {
        run(sender, task_cancel).await;
    });
}

async fn run(sender: relm4::Sender<AppCommand>, cancel: CancellationToken) {
    loop {
        if cancel.is_cancelled() {
            break;
        }

        match connect_once(shell_socket_path(), sender.clone(), cancel.clone()).await {
            Ok(()) => break,
            Err(error) => {
                tracing::warn!(
                    %error,
                    retry_seconds = RETRY_DELAY.as_secs(),
                    "lock: shell status stream disconnected; retrying"
                );
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(RETRY_DELAY) => {}
                }
            }
        }
    }
}

async fn connect_once(
    path: impl AsRef<Path>,
    sender: relm4::Sender<AppCommand>,
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
    apply_shell_status_line(&status, &sender)?;

    writer.write_all(b"subscribe weather.updated\n").await?;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            line = lines.next_line() => {
                let Some(line) = line? else {
                    bail!("shell IPC closed");
                };
                apply_shell_event_line(&line, &sender)?;
            }
        }
    }
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

fn apply_shell_status_line(line: &str, sender: &relm4::Sender<AppCommand>) -> Result<()> {
    let (name, fields) = parse_ipc_line(line);
    if name == "ack" && field(&fields, "ok") == Some("true") {
        let display = weather_display_from_fields(&fields, "weather_icon", "weather_temperature")
            .map_err(anyhow::Error::msg)?;
        let _ = sender.send(AppCommand::WeatherState(display));
    }
    Ok(())
}

fn apply_shell_event_line(line: &str, sender: &relm4::Sender<AppCommand>) -> Result<()> {
    let (name, fields) = parse_ipc_line(line);
    if name == "weather.updated" {
        let display =
            weather_display_from_fields(&fields, "icon", "temperature").map_err(anyhow::Error::msg)?;
        let _ = sender.send(AppCommand::WeatherState(display));
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

pub(crate) fn weather_display_from_fields(
    fields: &[(String, String)],
    icon_key: &str,
    temperature_key: &str,
) -> Result<Option<WeatherDisplay>, String> {
    if matches!(
        field(fields, "state").or_else(|| field(fields, "weather_state")),
        Some("loading" | "unavailable" | "unknown")
    ) {
        return Ok(None);
    }

    let Some(icon) = field(fields, icon_key) else {
        return Ok(None);
    };
    let Some(temperature) = field(fields, temperature_key) else {
        return Ok(None);
    };
    if icon.is_empty() || temperature.is_empty() {
        return Ok(None);
    }

    Ok(Some(WeatherDisplay {
        icon: icon.to_owned(),
        temperature: temperature.to_owned(),
    }))
}

fn field<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(field, _)| field == key)
        .map(|(_, value)| value.as_str())
}

#[cfg(test)]
mod tests {
    use super::{RETRY_DELAY, parse_ipc_line, weather_display_from_fields};
    use std::time::Duration;

    #[test]
    fn weather_status_fields_parse_to_display() {
        let display = weather_display_from_fields(
            &[
                ("weather_state".into(), "ready".into()),
                ("weather_icon".into(), "weather-clear-symbolic".into()),
                ("weather_temperature".into(), "21°C".into()),
            ],
            "weather_icon",
            "weather_temperature",
        )
        .expect("fields should parse")
        .expect("ready weather should render");

        assert_eq!(display.icon, "weather-clear-symbolic");
        assert_eq!(display.temperature, "21°C");
    }

    #[test]
    fn weather_event_fields_parse_to_display() {
        let display = weather_display_from_fields(
            &[
                ("state".into(), "ready".into()),
                ("icon".into(), "weather-showers-symbolic".into()),
                ("temperature".into(), "8°C".into()),
            ],
            "icon",
            "temperature",
        )
        .expect("fields should parse")
        .expect("ready weather should render");

        assert_eq!(display.icon, "weather-showers-symbolic");
        assert_eq!(display.temperature, "8°C");
    }

    #[test]
    fn unavailable_weather_clears_display() {
        let display = weather_display_from_fields(
            &[("state".into(), "unavailable".into())],
            "icon",
            "temperature",
        )
        .expect("fields should parse");

        assert!(display.is_none());
    }

    #[test]
    fn shell_status_retry_delay_is_fixed_at_five_seconds() {
        assert_eq!(RETRY_DELAY, Duration::from_secs(5));
    }

    #[test]
    fn shell_status_line_parser_unescapes_fields() {
        let (name, fields) =
            parse_ipc_line("weather.updated state=ready icon=weather-clear-symbolic city=New\\sYork");

        assert_eq!(name, "weather.updated");
        assert_eq!(
            fields,
            vec![
                ("state".into(), "ready".into()),
                ("icon".into(), "weather-clear-symbolic".into()),
                ("city".into(), "New York".into()),
            ]
        );
    }
}
