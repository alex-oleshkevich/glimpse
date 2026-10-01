use super::*;

/// `nl_langinfo(T_FMT_AMPM)` looks like the way to ask this and is not: it answers
/// `%I:%M:%S %p` even under `C`, because it reports whether a locale *has* a twelve-hour
/// rendering rather than whether it prefers one. Parsing `T_FMT` fails too — `en_US` answers
/// `%r`, which contains neither `%I` nor `%p`. Only the rendered `%X` distinguishes them.
#[test]
fn the_meridiem_is_looked_for_in_a_rendered_time_rather_than_in_a_pattern() {
    let shown = locale_afternoon();

    assert!(
        shown.contains("30"),
        "a locale's own time must render the minute, got {shown:?}"
    );
}

/// The item is `_NL_ITEM(LC_MEASUREMENT, 0)`, which glibc composes as a shift. `libc` exports
/// the category but not the composed item, so the shift is written here and pinned.
#[test]
fn the_measurement_item_is_the_category_shifted_into_place() {
    assert_eq!(MEASUREMENT, 720_896);
}
