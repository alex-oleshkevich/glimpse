use chrono::TimeZone;

use super::*;

#[test]
fn file_name_expands_the_default_template_at_a_fixed_time() {
    let template = FileTemplate::try_from("Screenshot from %Y-%m-%d %H-%M-%S.png".to_owned())
        .expect("valid template");
    let at = chrono::Local
        .with_ymd_and_hms(2026, 9, 24, 14, 3, 11)
        .unwrap();

    assert_eq!(
        file_name(&template, at),
        "Screenshot from 2026-09-24 14-03-11.png"
    );
}

#[test]
fn candidates_yields_the_name_then_numbered_suffixes_before_the_extension() {
    let mut it = candidates("Screenshot from 2026-09-24 14-03-11.png");
    assert_eq!(
        it.next().as_deref(),
        Some("Screenshot from 2026-09-24 14-03-11.png")
    );
    assert_eq!(
        it.next().as_deref(),
        Some("Screenshot from 2026-09-24 14-03-11 (2).png")
    );
    assert_eq!(
        it.next().as_deref(),
        Some("Screenshot from 2026-09-24 14-03-11 (3).png")
    );
}
