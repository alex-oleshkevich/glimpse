use super::*;

#[test]
fn a_pick_is_read_from_the_json_line() {
    let stdout = br##"{"format":"hex","value":"#E0563F","red":224,"green":86,"blue":63}"##;

    assert_eq!(answer(Some(0), stdout, b""), Ok(Some([224, 86, 63])));
}

#[test]
fn a_cancel_is_no_color_and_no_error() {
    assert_eq!(answer(Some(4), b"", b"cancelled"), Ok(None));
}

#[test]
fn a_failure_carries_what_the_picker_said() {
    let stderr = b"glimpse-picker: cannot capture the screen: no screencopy\n";

    assert_eq!(
        answer(Some(1), b"", stderr),
        Err("glimpse-picker: cannot capture the screen: no screencopy".to_owned())
    );
    assert_eq!(answer(None, b"", b""), Err("the picker failed".to_owned()));
}

#[test]
fn unreadable_output_is_a_failure_not_a_color() {
    assert!(answer(Some(0), b"#E0563F", b"").is_err());
}
