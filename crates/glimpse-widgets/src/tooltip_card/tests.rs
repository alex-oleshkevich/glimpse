use super::{BODY_MAX_CHARS, BODY_MAX_LINES, clamp_body, truncate};

#[test]
fn a_multibyte_body_is_cut_by_characters_rather_than_bytes() {
    let body = "é".repeat(BODY_MAX_CHARS * 2);
    assert_eq!(clamp_body(&body).chars().count(), BODY_MAX_CHARS);
}

#[test]
fn a_body_of_many_lines_keeps_only_the_first_few() {
    let body = (0..40)
        .map(|line| line.to_string())
        .collect::<Vec<String>>()
        .join("\n");
    assert_eq!(clamp_body(&body).lines().count(), BODY_MAX_LINES);
}

#[test]
fn a_short_body_is_left_alone() {
    assert_eq!(clamp_body("Synced\nJust now"), "Synced\nJust now");
    assert_eq!(truncate("Synced", 128), "Synced");
}
