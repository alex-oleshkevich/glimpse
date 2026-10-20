use super::*;

#[test]
fn every_windows_zone_names_an_iana_zone_chrono_knows() {
    for (windows, iana) in WINDOWS {
        assert!(
            iana.parse::<chrono_tz::Tz>().is_ok(),
            "{windows} maps to unknown {iana}"
        );
    }
}

#[test]
fn only_the_tzid_value_is_rewritten() {
    let text = "DTSTART;TZID=W. Europe Standard Time:20260907T100000\nSUMMARY:W. Europe Standard Time\nDTEND;TZID=Mars/Base:20260907T110000\n";
    assert_eq!(
        iana_names(text),
        "DTSTART;TZID=Europe/Berlin:20260907T100000\nSUMMARY:W. Europe Standard Time\nDTEND;TZID=Mars/Base:20260907T110000\n"
    );
}
