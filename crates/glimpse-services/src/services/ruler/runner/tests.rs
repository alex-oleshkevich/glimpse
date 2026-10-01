use super::*;

fn line(distance: f64) -> String {
    format!(
        r##"{{"from_x":1,"from_y":2,"to_x":3,"to_y":4,"dx":2,"dy":2,"distance":{distance},"angle":45.0}}"##
    )
}

#[test]
fn every_ndjson_line_is_read_in_order() {
    let stdout = format!("{}\n{}\n", line(1.0), line(2.0));

    let measurements = answer(Some(0), stdout.as_bytes(), b"").unwrap();

    assert_eq!(measurements.len(), 2);
    assert_eq!(measurements[0].distance, 1.0);
    assert_eq!(measurements[1].distance, 2.0);
}

#[test]
fn no_lines_on_exit_zero_is_an_empty_batch() {
    assert_eq!(answer(Some(0), b"", b""), Ok(Vec::new()));
}

#[test]
fn a_cancel_is_no_measurements_and_no_error() {
    assert_eq!(answer(Some(4), b"", b"cancelled"), Ok(Vec::new()));
}

#[test]
fn a_failure_carries_what_the_ruler_said() {
    let stderr = b"glimpse-ruler: cannot capture the screen: no screencopy\n";

    assert_eq!(
        answer(Some(1), b"", stderr),
        Err("glimpse-ruler: cannot capture the screen: no screencopy".to_owned())
    );
    assert_eq!(answer(None, b"", b""), Err("the ruler failed".to_owned()));
}

#[test]
fn unreadable_output_is_a_failure_not_a_batch() {
    assert!(answer(Some(0), b"not json", b"").is_err());
}

#[test]
fn one_bad_line_fails_the_whole_batch() {
    let stdout = format!("{}\nnot json\n", line(1.0));

    assert!(answer(Some(0), stdout.as_bytes(), b"").is_err());
}
