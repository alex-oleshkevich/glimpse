use std::ffi::CStr;

pub const TWENTY_FOUR: &str = "%H:%M";
const TWELVE: &str = "%-I:%M %p";

pub fn clock(twelve_hour: bool) -> &'static str {
    match twelve_hour {
        true => TWELVE,
        false => TWENTY_FOUR,
    }
}

const MEASUREMENT: libc::nl_item = (libc::LC_MEASUREMENT << 16) as libc::nl_item;
const METRIC: libc::c_char = 1;

pub(crate) fn locale_is_twelve_hour() -> bool {
    let marker = unsafe { libc::nl_langinfo(libc::PM_STR) };
    if marker.is_null() {
        return false;
    }
    let meridiem = unsafe { CStr::from_ptr(marker) }.to_string_lossy();
    !meridiem.is_empty() && locale_afternoon().contains(meridiem.as_ref())
}

pub(crate) fn locale_is_metric() -> bool {
    let answer = unsafe { libc::nl_langinfo(MEASUREMENT) };
    if answer.is_null() {
        return true;
    }
    unsafe { *answer == METRIC }
}

fn locale_afternoon() -> String {
    let mut when: libc::tm = unsafe { std::mem::zeroed() };
    when.tm_hour = 15;
    when.tm_min = 30;
    when.tm_mday = 1;
    when.tm_year = 126;

    let mut rendered = [0 as libc::c_char; 64];
    let written = unsafe {
        libc::strftime(
            rendered.as_mut_ptr(),
            rendered.len(),
            c"%X".as_ptr(),
            &raw const when,
        )
    };
    if written == 0 {
        return String::new();
    }

    unsafe { CStr::from_ptr(rendered.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
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
}
