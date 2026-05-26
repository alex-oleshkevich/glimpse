use std::{fs, path::Path, time::Duration};

use anyhow::{Context, anyhow};
use chrono::{LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use rrule::RRuleSet;

use crate::{CalendarSourceConfig, CalendarSourceType};

use super::{
    model::{CalendarAttendee, CalendarEvent, CalendarPerson, CalendarSource},
    source::SourceSnapshot,
};

const HTTP_FETCH_TIMEOUT: Duration = Duration::from_secs(15);
const RECURRENCE_LIMIT: u16 = 2048;

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
                events.extend(std::mem::take(&mut current).to_events(source)?);
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
    description: Option<String>,
    url: Option<String>,
    status: Option<String>,
    organizer: Option<CalendarPerson>,
    attendees: Vec<CalendarAttendee>,
    transparency: Option<String>,
    last_modified: Option<IcalDateTime>,
    sequence: Option<u32>,
    dtstart_line: Option<String>,
    recurrence_lines: Vec<String>,
}

impl ParsedEvent {
    fn apply_line(&mut self, line: &str) {
        let Some((name, value)) = line.split_once(':') else {
            return;
        };
        let property = IcalProperty::parse(name);
        match property.name.as_str() {
            "UID" => self.uid = Some(unescape_text(value)),
            "SUMMARY" => self.summary = Some(unescape_text(value)),
            "DTSTART" => {
                self.dtstart_line = Some(line.to_string());
                self.start = parse_ical_datetime(value, property.tzid.as_deref());
            }
            "DTEND" => self.end = parse_ical_datetime(value, property.tzid.as_deref()),
            "LOCATION" => self.location = Some(unescape_text(value)),
            "DESCRIPTION" => self.description = Some(unescape_text(value)),
            "URL" => self.url = Some(unescape_text(value)),
            "STATUS" => self.status = normalized_property_value(value),
            "ORGANIZER" => self.organizer = Some(parse_person(value, &property)),
            "ATTENDEE" => self.attendees.push(parse_attendee(value, &property)),
            "TRANSP" => self.transparency = normalized_property_value(value),
            "LAST-MODIFIED" => self.last_modified = parse_ical_datetime(value, None),
            "SEQUENCE" => self.sequence = value.trim().parse::<u32>().ok(),
            "RRULE" | "RDATE" | "EXDATE" => self.recurrence_lines.push(line.to_string()),
            _ => {}
        }
    }

    fn to_events(self, source: &CalendarSource) -> anyhow::Result<Vec<CalendarEvent>> {
        let uid = self
            .uid
            .ok_or_else(|| anyhow!("calendar event is missing UID"))?;
        let start = self
            .start
            .ok_or_else(|| anyhow!("calendar event {uid} is missing DTSTART"))?;
        let end = self.end.clone().unwrap_or_else(|| start.default_end());
        let base = CalendarEvent {
            event_id: uid,
            title: self.summary.unwrap_or_else(|| "Untitled event".into()),
            start: start.to_rfc3339(),
            end: end.to_rfc3339(),
            location: self.location.filter(|value| !value.is_empty()),
            meeting_url: detect_meeting_url(self.url.as_deref(), self.description.as_deref()),
            description: self.description.filter(|value| !value.is_empty()),
            url: self.url.filter(|value| !value.is_empty()),
            status: self.status,
            organizer: self.organizer,
            attendees: self.attendees,
            transparency: self.transparency,
            last_modified: self.last_modified.map(|value| value.to_rfc3339()),
            sequence: self.sequence,
            all_day: start.all_day,
            source: source.clone(),
        };
        if self.recurrence_lines.is_empty() {
            return Ok(vec![base]);
        }

        match expand_recurring_event(
            &base,
            &start,
            &end,
            self.dtstart_line,
            &self.recurrence_lines,
        ) {
            Ok(events) if !events.is_empty() => Ok(events),
            Ok(_) => Ok(vec![base]),
            Err(error) => {
                tracing::warn!(
                    event_id = %base.event_id,
                    %error,
                    "failed to expand recurring calendar event"
                );
                Ok(vec![base])
            }
        }
    }
}

fn expand_recurring_event(
    base: &CalendarEvent,
    start: &IcalDateTime,
    end: &IcalDateTime,
    dtstart_line: Option<String>,
    recurrence_lines: &[String],
) -> anyhow::Result<Vec<CalendarEvent>> {
    let dtstart_line = dtstart_line.ok_or_else(|| anyhow!("recurring event is missing DTSTART"))?;
    let mut rule_text = normalize_recurrence_timezone_aliases(&dtstart_line);
    for line in recurrence_lines {
        rule_text.push('\n');
        rule_text.push_str(&normalize_recurrence_timezone_aliases(line));
    }

    let rrule_set = rule_text.parse::<RRuleSet>()?;
    let duration = end.value - start.value;
    let result = rrule_set.all(RECURRENCE_LIMIT);
    if result.limited {
        tracing::debug!(
            event_id = %base.event_id,
            limit = RECURRENCE_LIMIT,
            "calendar recurrence expansion reached limit"
        );
    }
    let events = result
        .dates
        .into_iter()
        .map(|date| {
            let occurrence_start = date.with_timezone(&Utc);
            let occurrence_end = occurrence_start + duration;
            let mut event = base.clone();
            event.event_id = format!("{}#{}", base.event_id, occurrence_start.to_rfc3339());
            event.start = occurrence_start.to_rfc3339();
            event.end = occurrence_end.to_rfc3339();
            event
        })
        .collect();
    Ok(events)
}

fn normalize_recurrence_timezone_aliases(line: &str) -> String {
    let Some((name, value)) = line.split_once(':') else {
        return line.to_string();
    };
    let property = IcalProperty::parse(name);
    let Some(tzid) = property.tzid.as_deref() else {
        return line.to_string();
    };
    let Some(alias) = timezone_alias(tzid) else {
        return line.to_string();
    };

    format!("{}:{value}", replace_tzid_param(name, alias))
}

fn replace_tzid_param(name: &str, alias: &str) -> String {
    name.split(';')
        .map(|part| {
            let Some((key, _)) = part.split_once('=') else {
                return part.to_string();
            };
            if key.eq_ignore_ascii_case("TZID") {
                format!("{key}={alias}")
            } else {
                part.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

struct IcalProperty {
    name: String,
    tzid: Option<String>,
    params: Vec<(String, String)>,
}

impl IcalProperty {
    fn parse(raw: &str) -> Self {
        let mut parts = raw.split(';');
        let name = parts.next().unwrap_or(raw).to_ascii_uppercase();
        let params: Vec<(String, String)> = parts.filter_map(parse_param).collect();
        let tzid = params
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("TZID"))
            .map(|(_, value)| value.clone());

        Self { name, tzid, params }
    }

    fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

fn parse_param(part: &str) -> Option<(String, String)> {
    let (key, value) = part.split_once('=')?;
    Some((
        key.trim().to_ascii_uppercase(),
        unescape_text(value.trim().trim_matches('"')),
    ))
}

fn parse_person(value: &str, property: &IcalProperty) -> CalendarPerson {
    CalendarPerson {
        name: property
            .param("CN")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        email: mailto_email(value),
    }
}

fn parse_attendee(value: &str, property: &IcalProperty) -> CalendarAttendee {
    CalendarAttendee {
        person: parse_person(value, property),
        participation_status: property
            .param("PARTSTAT")
            .and_then(normalized_property_value),
        role: property.param("ROLE").and_then(normalized_property_value),
        rsvp: property.param("RSVP").and_then(parse_bool_param),
    }
}

fn mailto_email(value: &str) -> Option<String> {
    let value = value.trim();
    value
        .strip_prefix("mailto:")
        .or_else(|| value.strip_prefix("MAILTO:"))
        .unwrap_or(value)
        .trim()
        .split('?')
        .next()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalized_property_value(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_ascii_uppercase())
}

fn parse_bool_param(value: &str) -> Option<bool> {
    match value.trim().to_ascii_uppercase().as_str() {
        "TRUE" => Some(true),
        "FALSE" => Some(false),
        _ => None,
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

fn parse_ical_datetime(value: &str, tzid: Option<&str>) -> Option<IcalDateTime> {
    if value.len() == 8 {
        let date = NaiveDate::parse_from_str(value, "%Y%m%d").ok()?;
        return Some(IcalDateTime {
            value: Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0)?),
            all_day: true,
        });
    }

    if value.ends_with('Z') {
        let value = value.strip_suffix('Z')?;
        let datetime = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?;
        return Some(IcalDateTime {
            value: Utc.from_utc_datetime(&datetime),
            all_day: false,
        });
    }

    let datetime = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?;
    let value = match tzid.and_then(resolve_timezone) {
        Some(tz) => match tz.from_local_datetime(&datetime) {
            LocalResult::Single(value) => value.with_timezone(&Utc),
            LocalResult::Ambiguous(early, _) => early.with_timezone(&Utc),
            LocalResult::None => return None,
        },
        None => Utc.from_utc_datetime(&datetime),
    };

    Some(IcalDateTime {
        value,
        all_day: false,
    })
}

fn resolve_timezone(tzid: &str) -> Option<Tz> {
    timezone_alias(tzid)
        .unwrap_or_else(|| tzid.trim_matches('"'))
        .parse::<Tz>()
        .ok()
}

fn detect_meeting_url(url: Option<&str>, description: Option<&str>) -> Option<String> {
    url.into_iter()
        .chain(description.into_iter().flat_map(extract_urls))
        .find(|candidate| is_meeting_url(candidate))
        .map(ToOwned::to_owned)
}

fn extract_urls(value: &str) -> impl Iterator<Item = &str> {
    value.split_whitespace().filter_map(|word| {
        let trimmed = word.trim_matches(|ch: char| {
            matches!(
                ch,
                '<' | '>' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';'
            )
        });
        let trimmed = trimmed.trim_end_matches(|ch: char| matches!(ch, '.' | ':' | '!' | '?'));
        (trimmed.starts_with("http://") || trimmed.starts_with("https://")).then_some(trimmed)
    })
}

fn is_meeting_url(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("zoom.us/")
        || value.contains("meet.google.com/")
        || value.contains("teams.microsoft.com/")
        || value.contains("teams.live.com/")
}

fn timezone_alias(tzid: &str) -> Option<&'static str> {
    match tzid.trim_matches('"') {
        "W. Europe Standard Time" => Some("Europe/Zurich"),
        "Central European Standard Time" => Some("Europe/Warsaw"),
        _ => None,
    }
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
    async fn load_ical_source_parses_iana_tzid_datetime() {
        let path = temp_path("tzid.ics");
        fs::write(
            &path,
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:tzid-1\nSUMMARY:Zurich Meeting\nDTSTART;TZID=Europe/Zurich:20260526T090000\nDTEND;TZID=Europe/Zurich:20260526T100000\nEND:VEVENT\nEND:VCALENDAR\n",
        )
        .expect("test ics should be written");
        let config = ical_config(format!("file://{}", path.display()));

        let snapshot = load_ical_source(&config)
            .await
            .expect("ics file should load");

        assert_eq!(snapshot.events.len(), 1);
        assert_eq!(snapshot.events[0].start, "2026-05-26T07:00:00+00:00");
        assert_eq!(snapshot.events[0].end, "2026-05-26T08:00:00+00:00");
    }

    #[tokio::test]
    async fn load_ical_source_parses_outlook_windows_timezone_alias() {
        let path = temp_path("outlook-tzid.ics");
        fs::write(
            &path,
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:outlook-tzid-1\nSUMMARY:Outlook Meeting\nDTSTART;TZID=W. Europe Standard Time:20260526T090000\nDTEND;TZID=W. Europe Standard Time:20260526T100000\nEND:VEVENT\nEND:VCALENDAR\n",
        )
        .expect("test ics should be written");
        let config = ical_config(format!("file://{}", path.display()));

        let snapshot = load_ical_source(&config)
            .await
            .expect("ics file should load");

        assert_eq!(snapshot.events.len(), 1);
        assert_eq!(snapshot.events[0].start, "2026-05-26T07:00:00+00:00");
        assert_eq!(snapshot.events[0].end, "2026-05-26T08:00:00+00:00");
    }

    #[tokio::test]
    async fn load_ical_source_expands_weekly_rrule_occurrences() {
        let path = temp_path("weekly-rrule.ics");
        fs::write(
            &path,
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:family-weekly-1\nSUMMARY:Family Dinner\nDTSTART;TZID=Europe/Warsaw:20260505T180000\nDTEND;TZID=Europe/Warsaw:20260505T190000\nRRULE:FREQ=WEEKLY;COUNT=4\nEND:VEVENT\nEND:VCALENDAR\n",
        )
        .expect("test ics should be written");
        let config = ical_config(format!("file://{}", path.display()));

        let snapshot = load_ical_source(&config)
            .await
            .expect("ics file should load");

        assert_eq!(snapshot.events.len(), 4);
        assert_eq!(
            snapshot.events[0].event_id,
            "family-weekly-1#2026-05-05T16:00:00+00:00"
        );
        assert_eq!(snapshot.events[0].start, "2026-05-05T16:00:00+00:00");
        assert_eq!(snapshot.events[1].start, "2026-05-12T16:00:00+00:00");
        assert_eq!(snapshot.events[2].start, "2026-05-19T16:00:00+00:00");
        assert_eq!(snapshot.events[3].start, "2026-05-26T16:00:00+00:00");
        assert!(
            snapshot
                .events
                .iter()
                .all(|event| event.title == "Family Dinner")
        );
    }

    #[tokio::test]
    async fn load_ical_source_expands_outlook_windows_timezone_rrule_occurrences() {
        let path = temp_path("outlook-weekly-rrule.ics");
        fs::write(
            &path,
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:outlook-weekly-1\nSUMMARY:Outlook Weekly\nDTSTART;TZID=W. Europe Standard Time:20260505T090000\nDTEND;TZID=W. Europe Standard Time:20260505T100000\nRRULE:FREQ=WEEKLY;COUNT=2\nEND:VEVENT\nEND:VCALENDAR\n",
        )
        .expect("test ics should be written");
        let config = ical_config(format!("file://{}", path.display()));

        let snapshot = load_ical_source(&config)
            .await
            .expect("ics file should load");

        assert_eq!(snapshot.events.len(), 2);
        assert_eq!(snapshot.events[0].start, "2026-05-05T07:00:00+00:00");
        assert_eq!(snapshot.events[0].end, "2026-05-05T08:00:00+00:00");
        assert_eq!(snapshot.events[1].start, "2026-05-12T07:00:00+00:00");
        assert_eq!(snapshot.events[1].end, "2026-05-12T08:00:00+00:00");
    }

    #[tokio::test]
    async fn load_ical_source_expands_quoted_outlook_windows_timezone_rrule_occurrences() {
        let path = temp_path("outlook-weekly-quoted-rrule.ics");
        fs::write(
            &path,
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:outlook-weekly-quoted-1\nSUMMARY:Outlook Weekly Quoted\nDTSTART;TZID=\"W. Europe Standard Time\":20260505T090000\nDTEND;TZID=\"W. Europe Standard Time\":20260505T100000\nRRULE:FREQ=WEEKLY;COUNT=2\nEND:VEVENT\nEND:VCALENDAR\n",
        )
        .expect("test ics should be written");
        let config = ical_config(format!("file://{}", path.display()));

        let snapshot = load_ical_source(&config)
            .await
            .expect("ics file should load");

        assert_eq!(snapshot.events.len(), 2);
        assert_eq!(snapshot.events[0].start, "2026-05-05T07:00:00+00:00");
        assert_eq!(snapshot.events[0].end, "2026-05-05T08:00:00+00:00");
        assert_eq!(snapshot.events[1].start, "2026-05-12T07:00:00+00:00");
        assert_eq!(snapshot.events[1].end, "2026-05-12T08:00:00+00:00");
    }

    #[tokio::test]
    async fn load_ical_source_parses_rich_event_details() {
        let path = temp_path("rich-event.ics");
        fs::write(
            &path,
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:rich-1\nSUMMARY:Sprint Planning\nDTSTART:20260526T143000Z\nDTEND:20260526T151500Z\nLOCATION:Zoom\nDESCRIPTION:Discuss Q2 scope\\nJoin: https://zoom.us/j/123456789\nURL:https://calendar.google.com/event?eid=abc\nSTATUS:TENTATIVE\nTRANSP:OPAQUE\nORGANIZER;CN=Marta Nowak:mailto:marta@example.com\nATTENDEE;CN=Alex;PARTSTAT=ACCEPTED;ROLE=REQ-PARTICIPANT;RSVP=TRUE:mailto:alex@example.com\nLAST-MODIFIED:20260525T100000Z\nSEQUENCE:4\nEND:VEVENT\nEND:VCALENDAR\n",
        )
        .expect("test ics should be written");
        let config = ical_config(format!("file://{}", path.display()));

        let snapshot = load_ical_source(&config)
            .await
            .expect("ics file should load");

        let event = &snapshot.events[0];
        assert_eq!(
            event.description.as_deref(),
            Some("Discuss Q2 scope\nJoin: https://zoom.us/j/123456789")
        );
        assert_eq!(
            event.url.as_deref(),
            Some("https://calendar.google.com/event?eid=abc")
        );
        assert_eq!(
            event.meeting_url.as_deref(),
            Some("https://zoom.us/j/123456789")
        );
        assert_eq!(event.status.as_deref(), Some("TENTATIVE"));
        assert_eq!(event.transparency.as_deref(), Some("OPAQUE"));
        assert_eq!(
            event.last_modified.as_deref(),
            Some("2026-05-25T10:00:00+00:00")
        );
        assert_eq!(event.sequence, Some(4));
        let organizer = event.organizer.as_ref().expect("organizer parsed");
        assert_eq!(organizer.name.as_deref(), Some("Marta Nowak"));
        assert_eq!(organizer.email.as_deref(), Some("marta@example.com"));
        assert_eq!(event.attendees.len(), 1);
        assert_eq!(event.attendees[0].person.name.as_deref(), Some("Alex"));
        assert_eq!(
            event.attendees[0].person.email.as_deref(),
            Some("alex@example.com")
        );
        assert_eq!(
            event.attendees[0].participation_status.as_deref(),
            Some("ACCEPTED")
        );
        assert_eq!(event.attendees[0].role.as_deref(), Some("REQ-PARTICIPANT"));
        assert_eq!(event.attendees[0].rsvp, Some(true));
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
