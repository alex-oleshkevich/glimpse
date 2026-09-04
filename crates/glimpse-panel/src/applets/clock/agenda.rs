use chrono::{DateTime, Local, NaiveDate, TimeDelta};
use gtk4::gdk;

const SOON: i64 = 15;
const NEAR: i64 = 60;
const HOUR: i64 = 60;

#[derive(Debug, Clone, PartialEq)]
pub struct Occasion {
    pub summary: String,
    pub detail: String,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub all_day: bool,
    pub color: Option<gdk::RGBA>,
}

pub fn when(now: DateTime<Local>, day: NaiveDate, event: &Occasion, clock: &str) -> String {
    let first = event.start.date_naive();
    let last = event.end.date_naive();
    let days = (last - first).num_days() + 1;

    if event.all_day {
        return match days > 1 {
            true => format!("All day · day {} of {days}", (day - first).num_days() + 1),
            false => "All day".to_owned(),
        };
    }

    let started = event.start.format(clock);
    let length = span(event.end - event.start);

    if days > 1 {
        return format!(
            "{started} · until {} {}",
            event.end.format("%a"),
            event.end.format(clock)
        );
    }

    if day != now.date_naive() {
        return format!("{started} · {length}");
    }

    if now >= event.end {
        let ago = now - event.end;
        return match ago >= TimeDelta::minutes(NEAR) {
            true => format!("{started} · over"),
            false => format!("ended {} ago", span(ago)),
        };
    }

    if now >= event.start {
        let left = event.end - now;
        return match left <= TimeDelta::minutes(SOON) {
            true => format!("now · ends in {}", span(left)),
            false => format!("now · ends {}", event.end.format(clock)),
        };
    }

    let until = event.start - now;
    if until < TimeDelta::minutes(1) {
        return format!("starting now · {length}");
    }
    if until <= TimeDelta::minutes(NEAR) {
        return format!("in {} · {length}", span(until));
    }
    format!("{started} · {length}")
}

fn span(length: TimeDelta) -> String {
    let minutes = length.num_minutes().max(0);
    if minutes < HOUR {
        return format!("{minutes} min");
    }

    let hours = minutes / HOUR;
    if hours < 24 {
        return match minutes % HOUR {
            0 => format!("{hours} h"),
            rest => format!("{hours} h {rest} min"),
        };
    }

    let days = hours / 24;
    match hours % 24 {
        0 => format!("{days} d"),
        rest => format!("{days} d {rest} h"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const CLOCK: &str = "%H:%M";

    fn at(hour: u32, minute: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 9, 4, hour, minute, 0)
            .single()
            .expect("an unambiguous local time")
    }

    fn event(start: DateTime<Local>, end: DateTime<Local>) -> Occasion {
        Occasion {
            summary: "Standup".to_owned(),
            detail: String::new(),
            start,
            end,
            all_day: false,
            color: None,
        }
    }

    fn read(now: DateTime<Local>, event: &Occasion) -> String {
        when(now, now.date_naive(), event, CLOCK)
    }

    #[test]
    fn an_all_day_event_says_so_and_a_multi_day_one_says_where_it_is() {
        let mut all_day = event(at(0, 0), at(23, 59));
        all_day.all_day = true;
        assert_eq!(read(at(12, 0), &all_day), "All day");

        let mut conference = event(at(0, 0), at(0, 0) + TimeDelta::days(2));
        conference.all_day = true;
        assert_eq!(
            when(at(12, 0), at(12, 0).date_naive(), &conference, CLOCK),
            "All day · day 1 of 3"
        );
        assert_eq!(
            when(
                at(12, 0),
                (at(12, 0) + TimeDelta::days(1)).date_naive(),
                &conference,
                CLOCK
            ),
            "All day · day 2 of 3"
        );
    }

    #[test]
    fn a_timed_event_running_past_midnight_names_the_day_it_ends() {
        let overnight = event(at(22, 0), at(22, 0) + TimeDelta::hours(16));
        assert_eq!(read(at(23, 0), &overnight), "22:00 · until Sat 14:00");
    }

    #[test]
    fn a_row_under_another_date_reads_as_a_time_and_a_length() {
        let meeting = event(at(9, 0), at(10, 30));
        let elsewhere = at(9, 0).date_naive() - TimeDelta::days(1);

        assert_eq!(
            when(at(12, 0), elsewhere, &meeting, CLOCK),
            "09:00 · 1 h 30 min"
        );
    }

    #[test]
    fn an_event_that_ended_keeps_its_recency_for_an_hour() {
        let meeting = event(at(9, 0), at(10, 0));

        assert_eq!(read(at(10, 12), &meeting), "ended 12 min ago");
        assert_eq!(
            read(at(11, 30), &meeting),
            "09:00 · over",
            "an hour on, when it ended stops being the useful fact"
        );
    }

    #[test]
    fn a_running_event_says_when_it_ends_and_counts_down_near_the_end() {
        let meeting = event(at(9, 0), at(10, 0));

        assert_eq!(read(at(9, 30), &meeting), "now · ends 10:00");
        assert_eq!(read(at(9, 52), &meeting), "now · ends in 8 min");
    }

    #[test]
    fn an_event_about_to_start_counts_down_rather_than_saying_in_zero_minutes() {
        let meeting = event(at(9, 0), at(10, 0));

        assert_eq!(read(at(8, 48), &meeting), "in 12 min · 1 h");
        assert_eq!(
            read(at(9, 0) - TimeDelta::seconds(30), &meeting),
            "starting now · 1 h",
            "`in 0 min` is what this exists to avoid"
        );
        assert_eq!(read(at(7, 0), &meeting), "09:00 · 1 h");
    }

    #[test]
    fn a_length_grows_through_minutes_hours_and_days() {
        assert_eq!(span(TimeDelta::minutes(45)), "45 min");
        assert_eq!(span(TimeDelta::hours(1)), "1 h");
        assert_eq!(span(TimeDelta::minutes(90)), "1 h 30 min");
        assert_eq!(span(TimeDelta::hours(48)), "2 d");
        assert_eq!(span(TimeDelta::hours(51)), "2 d 3 h");
        assert_eq!(
            span(TimeDelta::seconds(-10)),
            "0 min",
            "a negative span is not a huge one"
        );
    }
}
