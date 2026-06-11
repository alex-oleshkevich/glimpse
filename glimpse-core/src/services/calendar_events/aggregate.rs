use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Local, NaiveDate};

use crate::{CalendarConfig, CalendarSourceType};

use super::{
    dedupe::{EventCandidate, dedupe_events},
    ical, local,
    model::{
        CalendarDate, CalendarDaySnapshot, CalendarEvent, CalendarMonthDay, CalendarMonthSnapshot,
        MonthKey,
    },
    source::SourceSnapshot,
};

#[derive(Clone)]
pub struct CalendarAggregator {
    config: CalendarConfig,
}

impl CalendarAggregator {
    pub fn new(config: CalendarConfig) -> Self {
        Self { config }
    }

    pub fn reconfigure(&mut self, config: CalendarConfig) {
        self.config = config;
    }

    pub fn poll_interval(&self) -> std::time::Duration {
        effective_poll_interval(&self.config)
    }

    pub async fn load_month(&self, key: MonthKey) -> anyhow::Result<CalendarMonthSnapshot> {
        let sources = self.load_configured_sources().await;
        build_month_snapshot(key, sources)
    }

    async fn load_configured_sources(&self) -> Vec<SourceSnapshot> {
        let mut snapshots = Vec::new();
        for source in &self.config.sources {
            let result = match source.source_type {
                CalendarSourceType::Ical => ical::load_ical_source(source).await,
                CalendarSourceType::Directory => local::load_directory_source(source),
            };
            match result {
                Ok(snapshot) => {
                    tracing::debug!(
                        source = %source.id,
                        event_count = snapshot.events.len(),
                        "loaded configured calendar source"
                    );
                    snapshots.push(snapshot);
                }
                Err(error) => {
                    tracing::warn!(source = %source.id, %error, "failed to load configured calendar source");
                }
            }
        }
        snapshots
    }
}

pub fn effective_poll_interval(config: &CalendarConfig) -> std::time::Duration {
    let seconds = config
        .sources
        .iter()
        .filter_map(|source| source.poll_interval)
        .chain(std::iter::once(config.poll_interval))
        .min()
        .unwrap_or(config.poll_interval)
        .max(60);
    std::time::Duration::from_secs(seconds)
}

pub fn build_month_snapshot(
    key: MonthKey,
    sources: Vec<SourceSnapshot>,
) -> anyhow::Result<CalendarMonthSnapshot> {
    let month_start = key
        .to_naive_date()
        .ok_or_else(|| anyhow::anyhow!("invalid calendar month"))?;
    let next_month = month_start
        .checked_add_months(chrono::Months::new(1))
        .ok_or_else(|| anyhow::anyhow!("month overflow"))?;
    let mut candidates = Vec::new();

    for snapshot in sources {
        for event in snapshot.events {
            if event_overlaps_month(&event, month_start, next_month) {
                candidates.push(EventCandidate::new(event));
            }
        }
    }

    Ok(summarize_events(
        key,
        month_start,
        next_month,
        dedupe_events(candidates),
    ))
}

fn summarize_events(
    key: MonthKey,
    month_start: NaiveDate,
    next_month: NaiveDate,
    events: Vec<CalendarEvent>,
) -> CalendarMonthSnapshot {
    let mut by_date: BTreeMap<CalendarDate, Vec<CalendarEvent>> = BTreeMap::new();
    let mut colors_by_date: BTreeMap<CalendarDate, BTreeSet<String>> = BTreeMap::new();

    for event in events {
        for date in event_dates_in_month(&event, month_start, next_month) {
            let calendar_date = CalendarDate::from_naive_date(date);
            if let Some(color) = event.source.color.clone() {
                colors_by_date
                    .entry(calendar_date)
                    .or_default()
                    .insert(color);
            }
            by_date
                .entry(calendar_date)
                .or_default()
                .push(event.clone());
        }
    }

    let mut days = Vec::new();
    let mut day = month_start;
    while day < next_month {
        let date = CalendarDate::from_naive_date(day);
        days.push(CalendarMonthDay {
            date,
            colors: colors_by_date
                .remove(&date)
                .map(|colors| colors.into_iter().collect())
                .unwrap_or_default(),
        });
        day = day
            .succ_opt()
            .expect("calendar month iteration should not overflow");
    }

    let day_snapshots = by_date
        .into_iter()
        .map(|(date, events)| (date, CalendarDaySnapshot { date, events }))
        .collect();

    CalendarMonthSnapshot {
        key,
        days,
        day_snapshots,
    }
}

fn event_overlaps_month(
    event: &CalendarEvent,
    month_start: NaiveDate,
    next_month: NaiveDate,
) -> bool {
    event_dates_in_month(event, month_start, next_month)
        .next()
        .is_some()
}

fn event_dates_in_month(
    event: &CalendarEvent,
    month_start: NaiveDate,
    next_month: NaiveDate,
) -> impl Iterator<Item = NaiveDate> {
    let Some(start) = event_start_date(event) else {
        return Vec::new().into_iter();
    };
    let end = event_end_date(event).unwrap_or(start);
    let mut current = if start >= month_start {
        start
    } else {
        month_start
    };
    let last = if end < next_month {
        end
    } else {
        next_month.pred_opt().unwrap_or(month_start)
    };
    let mut dates = Vec::new();
    while current <= last {
        dates.push(current);
        let Some(next) = current.succ_opt() else {
            break;
        };
        current = next;
    }
    dates.into_iter()
}

fn event_start_date(event: &CalendarEvent) -> Option<NaiveDate> {
    if event.all_day {
        // All-day events are stored as UTC midnight; convert to Local would
        // shift the date in negative-UTC-offset timezones.
        DateTime::parse_from_rfc3339(&event.start)
            .ok()
            .map(|t| t.date_naive())
    } else {
        parse_event_time(&event.start).map(|t| t.date_naive())
    }
}

fn event_end_date(event: &CalendarEvent) -> Option<NaiveDate> {
    if event.all_day {
        let date = DateTime::parse_from_rfc3339(&event.end)
            .ok()
            .map(|t| t.date_naive())?;
        Some(date.pred_opt().unwrap_or(date))
    } else {
        parse_event_time(&event.end).map(|t| t.date_naive())
    }
}

fn parse_event_time(value: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&Local))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::calendar_events::{
        model::{CalendarDate, CalendarEvent, CalendarSource, MonthKey},
        source::SourceSnapshot,
    };
    use crate::{CalendarConfig, CalendarSourceConfig, CalendarSourceType};
    use std::time::Duration;

    fn event(id: &str, title: &str, source_id: &str) -> CalendarEvent {
        CalendarEvent {
            event_id: id.into(),
            title: title.into(),
            start: "2026-05-26T09:00:00+00:00".into(),
            end: "2026-05-26T09:30:00+00:00".into(),
            location: None,
            all_day: false,
            source: CalendarSource {
                source_id: source_id.into(),
                display_name: source_id.into(),
                color: Some("#4285f4".into()),
            },
            ..CalendarEvent::default()
        }
    }

    fn snapshot(source_id: &str, events: Vec<CalendarEvent>) -> SourceSnapshot {
        SourceSnapshot {
            source: CalendarSource {
                source_id: source_id.into(),
                display_name: source_id.into(),
                color: Some("#4285f4".into()),
            },
            events,
        }
    }

    #[test]
    fn build_month_snapshot_dedupes_configured_sources_by_name_and_time() {
        let month = MonthKey {
            year: 2026,
            month: 5,
        };
        let snapshot = build_month_snapshot(
            month,
            vec![
                snapshot("google", vec![event("google-1", "Team Standup", "google")]),
                snapshot("work", vec![event("work-1", "team  standup", "work")]),
            ],
        )
        .expect("month snapshot should build");
        let date = CalendarDate {
            year: 2026,
            month: 5,
            day: 26,
        };

        let day = snapshot
            .day_snapshots
            .get(&date)
            .expect("event day should be present");

        assert_eq!(day.events.len(), 1);
        assert_eq!(day.events[0].event_id, "google-1");
        assert_eq!(
            snapshot
                .days
                .iter()
                .filter(|day| !day.colors.is_empty())
                .count(),
            1
        );
    }

    #[test]
    fn effective_poll_interval_uses_shortest_source_interval_with_sixty_second_floor() {
        let config = CalendarConfig {
            poll_interval: 900,
            sources: vec![
                CalendarSourceConfig {
                    id: "fast".into(),
                    source_type: CalendarSourceType::Ical,
                    uri: "file:///tmp/fast.ics".into(),
                    name: None,
                    poll_interval: Some(30),
                    color: None,
                },
                CalendarSourceConfig {
                    id: "normal".into(),
                    source_type: CalendarSourceType::Ical,
                    uri: "file:///tmp/normal.ics".into(),
                    name: None,
                    poll_interval: Some(300),
                    color: None,
                },
            ],
            ..CalendarConfig::default()
        };

        assert_eq!(effective_poll_interval(&config), Duration::from_secs(60));
    }

    #[test]
    fn build_month_snapshot_includes_all_day_events_with_exclusive_end_date() {
        let month = MonthKey {
            year: 2026,
            month: 5,
        };
        let mut event = event("all-day", "Holiday", "local");
        event.start = "2026-05-26T00:00:00+00:00".into();
        event.end = "2026-05-27T00:00:00+00:00".into();
        event.all_day = true;

        let snapshot = build_month_snapshot(month, vec![snapshot("local", vec![event])])
            .expect("month snapshot should build");

        assert!(snapshot.day_snapshots.contains_key(&CalendarDate {
            year: 2026,
            month: 5,
            day: 26
        }));
        assert!(!snapshot.day_snapshots.contains_key(&CalendarDate {
            year: 2026,
            month: 5,
            day: 27
        }));
    }

    #[tokio::test]
    async fn load_month_without_sources_returns_empty_snapshot() {
        let aggregator = CalendarAggregator::new(CalendarConfig::default());
        let month = MonthKey {
            year: 2026,
            month: 5,
        };

        let snapshot = aggregator
            .load_month(month)
            .await
            .expect("empty calendar config should still load");

        assert_eq!(snapshot.key, month);
        assert_eq!(snapshot.days.len(), 31);
        assert!(snapshot.day_snapshots.is_empty());
    }
}
