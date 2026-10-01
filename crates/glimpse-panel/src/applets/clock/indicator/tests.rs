use super::{MINUTE, SECOND, formatted, period, ticks_below_a_minute};
use chrono::{TimeZone, Utc};

#[test]
fn a_format_that_shows_seconds_asks_for_a_faster_tick() {
    assert_eq!(period("%H:%M:%S", None), SECOND);
    assert_eq!(period("%T", None), SECOND);
    assert_eq!(period("%H:%M", None), MINUTE);
}

#[test]
fn a_padded_specifier_is_still_the_specifier_it_pads() {
    assert!(
        ticks_below_a_minute("%-S"),
        "a padding modifier hides the specifier from a plain substring search"
    );
    assert!(ticks_below_a_minute("%.3f"));
    assert!(!ticks_below_a_minute("%-d %b"));
}

#[test]
fn an_escaped_percent_does_not_name_a_specifier() {
    assert!(
        !ticks_below_a_minute("100%%S"),
        "`%%` is a literal percent, so the S after it is text"
    );
    assert!(ticks_below_a_minute("100%% %S"));
}

#[test]
fn the_tooltip_can_be_what_asks_for_the_faster_tick() {
    assert_eq!(period("%H:%M", Some("%H:%M:%S")), SECOND);
    assert_eq!(period("%H:%M", Some("%A")), MINUTE);
}

#[test]
fn a_format_that_cannot_render_yields_nothing_rather_than_panicking() {
    let now = Utc.with_ymd_and_hms(2026, 9, 4, 15, 30, 0).unwrap();

    assert_eq!(formatted(&now, "%H:%M").as_deref(), Some("15:30"));
    assert!(
        formatted(&now, "%Q").is_none(),
        "chrono's Display returns an error for an unknown specifier, and to_string() turns \
             that into a panic that would stop the applet for good"
    );
}
