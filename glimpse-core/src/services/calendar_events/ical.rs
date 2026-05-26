use std::{fs, path::Path, time::Duration};

use anyhow::{Context, anyhow};
use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};

use crate::{CalendarSourceConfig, CalendarSourceType};

use super::{
    model::{CalendarEvent, CalendarSource},
    source::SourceSnapshot,
};

const HTTP_FETCH_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn load_ical_source(config: &CalendarSourceConfig) -> anyhow::Result<SourceSnapshot> {
    if config.source_type != CalendarSourceType::Ical {
        return Err(anyhow!(
            "calendar source {} is not an iCalendar source",
            config.id
        ));
    }
    let content = load_ical_content(&config.uri).await?;
    let trimmed = content.trim_start();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let content = fetch_calendar_url(trimmed.trim()).await?;
        return parse_ical_source(config, &content);
    }
    parse_ical_source(config, &content)
}

async fn load_ical_content(uri: &str) -> anyhow::Result<String> {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        return fetch_calendar_url(uri).await;
    }
    let path = file_uri_path(uri)?;
    fs::read_to_string(&path)
        .with_context(|| format!("failed to read iCalendar source {}", path.display()))
}

async fn fetch_calendar_url(uri: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(HTTP_FETCH_TIMEOUT)
        .build()
        .context("failed to create calendar HTTP client")?;
    let response = client
        .get(uri)
        .send()
        .await
        .with_context(|| format!("failed to fetch calendar URL {uri}"))?
        .error_for_status()
        .with_context(|| format!("calendar URL returned an error status {uri}"))?;
    response
        .text()
        .await
        .with_context(|| format!("failed to read calendar URL response {uri}"))
}

pub fn parse_ical_source(
    config: &CalendarSourceConfig,
    content: &str,
) -> anyhow::Result<SourceSnapshot> {
    let source = CalendarSource {
        source_id: config.id.clone(),
        display_name: config.name.clone().unwrap_or_else(|| config.id.clone()),
        color: config.color.clone(),
    };
    let mut events = parse_events(content, &source)?;
    events.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| a.end.cmp(&b.end))
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.event_id.cmp(&b.event_id))
    });
    Ok(SourceSnapshot { source, events })
}

fn parse_events(content: &str, source: &CalendarSource) -> anyhow::Result<Vec<CalendarEvent>> {
    let mut events = Vec::new();
    let mut current = ParsedEvent::default();
    let mut in_event = false;

    for line in unfold_lines(content) {
        match line.as_str() {
            "BEGIN:VEVENT" => {
                in_event = true;
                current = ParsedEvent::default();
            }
            "END:VEVENT" if in_event => {
                events.push(std::mem::take(&mut current).to_event(source)?);
                in_event = false;
            }
            _ if in_event => current.apply_line(&line),
            _ => {}
        }
    }

    Ok(events)
}

fn unfold_lines(content: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in content.lines() {
        let line = raw.trim_end_matches('\r');
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(previous) = lines.last_mut() {
                previous.push_str(line.trim_start());
            }
        } else {
            lines.push(line.to_string());
        }
    }
    lines
}

#[derive(Default)]
struct ParsedEvent {
    uid: Option<String>,
    summary: Option<String>,
    start: Option<IcalDateTime>,
    end: Option<IcalDateTime>,
    location: Option<String>,
}

impl ParsedEvent {
    fn apply_line(&mut self, line: &str) {
        let Some((name, value)) = line.split_once(':') else {
            return;
        };
        let property = name
            .split_once(';')
            .map(|(property, _)| property)
            .unwrap_or(name);
        match property {
            "UID" => self.uid = Some(unescape_text(value)),
            "SUMMARY" => self.summary = Some(unescape_text(value)),
            "DTSTART" => self.start = parse_ical_datetime(value),
            "DTEND" => self.end = parse_ical_datetime(value),
            "LOCATION" => self.location = Some(unescape_text(value)),
            _ => {}
        }
    }

    fn to_event(self, source: &CalendarSource) -> anyhow::Result<CalendarEvent> {
        let uid = self
            .uid
            .ok_or_else(|| anyhow!("calendar event is missing UID"))?;
        let start = self
            .start
            .ok_or_else(|| anyhow!("calendar event {uid} is missing DTSTART"))?;
        let end = self.end.clone().unwrap_or_else(|| start.default_end());
        Ok(CalendarEvent {
            event_id: uid,
            title: self.summary.unwrap_or_else(|| "Untitled event".into()),
            start: start.to_rfc3339(),
            end: end.to_rfc3339(),
            location: self.location.filter(|value| !value.is_empty()),
            all_day: start.all_day,
            source: source.clone(),
        })
    }
}

#[derive(Clone)]
struct IcalDateTime {
    value: chrono::DateTime<Utc>,
    all_day: bool,
}

impl IcalDateTime {
    fn to_rfc3339(&self) -> String {
        self.value.to_rfc3339()
    }

    fn default_end(&self) -> Self {
        let duration = if self.all_day {
            chrono::Duration::days(1)
        } else {
            chrono::Duration::hours(1)
        };
        Self {
            value: self.value + duration,
            all_day: self.all_day,
        }
    }
}

fn parse_ical_datetime(value: &str) -> Option<IcalDateTime> {
    if value.len() == 8 {
        let date = NaiveDate::parse_from_str(value, "%Y%m%d").ok()?;
        return Some(IcalDateTime {
            value: Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0)?),
            all_day: true,
        });
    }

    let value = value.strip_suffix('Z').unwrap_or(value);
    let datetime = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?;
    Some(IcalDateTime {
        value: Utc.from_utc_datetime(&datetime),
        all_day: false,
    })
}

fn unescape_text(value: &str) -> String {
    value
        .replace("\\n", "\n")
        .replace("\\N", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

fn file_uri_path(uri: &str) -> anyhow::Result<&Path> {
    let path = uri
        .strip_prefix("file://")
        .ok_or_else(|| anyhow!("only file:// iCalendar sources are supported by this loader"))?;
    Ok(Path::new(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CalendarSourceConfig, CalendarSourceType};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("glimpse-calendar-{stamp}-{name}"))
    }

    fn ical_config(uri: String) -> CalendarSourceConfig {
        CalendarSourceConfig {
            id: "personal".into(),
            source_type: CalendarSourceType::Ical,
            name: Some("Personal".into()),
            uri,
            poll_interval: None,
            color: Some("#4285f4".into()),
        }
    }

    #[tokio::test]
    async fn load_ical_source_reads_raw_ics_from_file_uri() {
        let path = temp_path("personal.ics");
        fs::write(
            &path,
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:event-1\nSUMMARY:Team Standup\nDTSTART:20260526T070000Z\nDTEND:20260526T073000Z\nEND:VEVENT\nEND:VCALENDAR\n",
        )
        .expect("test ics should be written");
        let config = ical_config(format!("file://{}", path.display()));

        let snapshot = load_ical_source(&config)
            .await
            .expect("ics file should load");

        assert_eq!(snapshot.source.source_id, "personal");
        assert_eq!(snapshot.source.display_name, "Personal");
        assert_eq!(snapshot.source.color.as_deref(), Some("#4285f4"));
        assert_eq!(snapshot.events.len(), 1);
        assert_eq!(snapshot.events[0].event_id, "event-1");
        assert_eq!(snapshot.events[0].title, "Team Standup");
        assert_eq!(snapshot.events[0].start, "2026-05-26T07:00:00+00:00");
        assert_eq!(snapshot.events[0].end, "2026-05-26T07:30:00+00:00");
    }

    #[tokio::test]
    async fn load_ical_source_defaults_all_day_end_to_next_day() {
        let path = temp_path("all-day.ics");
        fs::write(
            &path,
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:all-day-1\nSUMMARY:Holiday\nDTSTART:20260526\nEND:VEVENT\nEND:VCALENDAR\n",
        )
        .expect("test ics should be written");
        let config = ical_config(format!("file://{}", path.display()));

        let snapshot = load_ical_source(&config)
            .await
            .expect("ics file should load");

        assert_eq!(snapshot.events.len(), 1);
        assert!(snapshot.events[0].all_day);
        assert_eq!(snapshot.events[0].start, "2026-05-26T00:00:00+00:00");
        assert_eq!(snapshot.events[0].end, "2026-05-27T00:00:00+00:00");
    }

    #[tokio::test]
    async fn load_ical_source_treats_file_containing_calendar_url_as_target_url() {
        let body = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:event-url\nSUMMARY:From URL\nDTSTART:20260526T070000Z\nDTEND:20260526T073000Z\nEND:VEVENT\nEND:VCALENDAR\n";
        let server = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test server should bind");
        let address = server
            .local_addr()
            .expect("test server should have address");
        let task = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut stream, _) = server.accept().await.expect("request should connect");
            let mut buffer = [0; 1024];
            let _ = stream.read(&mut buffer).await.expect("request should read");
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/calendar\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("response should write");
        });
        let path = temp_path("personal.url");
        fs::write(&path, format!("http://{address}/private.ics\n"))
            .expect("test url should be written");
        let config = ical_config(format!("file://{}", path.display()));

        let snapshot = load_ical_source(&config)
            .await
            .expect("secret URL file should load target calendar");

        assert_eq!(snapshot.events.len(), 1);
        assert_eq!(snapshot.events[0].event_id, "event-url");
        task.await.expect("test server should finish");
    }
}
