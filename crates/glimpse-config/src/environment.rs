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

pub fn twelve_hour() -> bool {
    let marker = unsafe { libc::nl_langinfo(libc::PM_STR) };
    if marker.is_null() {
        return false;
    }
    let meridiem = unsafe { CStr::from_ptr(marker) }.to_string_lossy();
    !meridiem.is_empty() && locale_afternoon().contains(meridiem.as_ref())
}

pub fn metric() -> bool {
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
mod tests;
