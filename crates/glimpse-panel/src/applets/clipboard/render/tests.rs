use chrono::DateTime;

use super::*;

#[test]
fn a_title_is_capped_by_character_and_survives_multi_byte_text() {
    let entry = text_entry(&"Марта🙂".repeat(50));

    let rendered = title(&entry, 12);

    assert!(rendered.chars().count() <= 13, "got {rendered}");
    assert!(rendered.starts_with("Марта🙂"));
}

#[test]
fn sizes_step_through_bytes_kilobytes_and_megabytes() {
    assert_eq!(size(58), "58 B");
    assert_eq!(size(2048), "2 kB");
    assert_eq!(size(1_468_006), "1.4 MB");
}

#[test]
fn an_empty_history_takes_no_room_on_the_bar() {
    assert!(!shown(0, false, false));
    assert!(shown(1, false, false));
    assert!(shown(0, true, false));
}

#[test]
fn a_clipboard_that_cannot_be_watched_is_still_reachable() {
    assert!(
        shown(0, false, true),
        "a compositor with no data-control protocol must still get a chip to press"
    );
}

#[test]
fn a_tooltip_without_a_template_is_absent_rather_than_empty() {
    assert_eq!(filled(None, 4), None);
    assert_eq!(filled(Some("{count} items"), 4).as_deref(), Some("4 items"));
}

#[test]
fn an_unknown_token_survives_into_the_label() {
    assert_eq!(
        filled(Some("{count}/{nonesuch}"), 2).as_deref(),
        Some("2/{nonesuch}")
    );
}

#[test]
fn a_web_address_reads_as_its_path_under_its_host() {
    assert_eq!(
        shape(" https://www.github.com/niri-wm/niri/pull/2141 "),
        Shape::Link {
            url: "https://www.github.com/niri-wm/niri/pull/2141".to_owned(),
            host: "github.com".to_owned(),
            rest: "niri-wm/niri/pull/2141".to_owned(),
        }
    );
    assert_eq!(shape("ftp://example.com/file"), Shape::Plain);
    assert_eq!(shape("see https://example.com"), Shape::Plain);
}

#[test]
fn a_hex_color_is_recognized_in_both_lengths_and_nothing_else_is() {
    assert_eq!(shape("#1e88e5"), Shape::Color([30, 136, 229]));
    assert_eq!(shape("#fff"), Shape::Color([255, 255, 255]));
    assert_eq!(shape("#12345"), Shape::Plain);
    assert_eq!(shape("#ggg"), Shape::Plain);
    assert_eq!(shape("#"), Shape::Plain);
}

#[test]
fn paths_and_several_lines_are_told_apart() {
    assert_eq!(shape("~/notes.txt"), Shape::Path("~/notes.txt".to_owned()));
    assert_eq!(shape("/etc/hosts"), Shape::Path("/etc/hosts".to_owned()));
    assert_eq!(shape("one\ntwo\nthree"), Shape::Lines(3));
    assert_eq!(shape("just words"), Shape::Plain);
    assert_eq!(
        home_path("~/notes.txt", std::path::Path::new("/home/me")),
        std::path::PathBuf::from("/home/me/notes.txt")
    );
}

#[test]
fn an_excerpt_keeps_lines_but_cleans_and_caps_each() {
    assert_eq!(
        excerpt("  first\u{202e}\n\tsecond line\nthird\n", 2, 5),
        "first\nsecon…"
    );
}

#[test]
fn a_search_matches_text_without_case_and_never_an_image() {
    assert!(matches(&text_entry("Code Review"), "review"));
    assert!(matches(&text_entry("anything"), "  "));
    assert!(!matches(&text_entry("anything"), "else"));
    let mut image = text_entry("");
    image.kind = ClipboardKind::Image;
    assert!(!matches(&image, "png"));
    assert!(matches(&image, ""));
}

fn text_entry(body: &str) -> ClipboardEntry {
    ClipboardEntry {
        id: 1,
        kind: ClipboardKind::Text,
        mime: "text/plain".to_owned(),
        preview: body.to_owned(),
        pinned: false,
        at: DateTime::from_timestamp(1_700_000_000, 0).expect("an instant"),
        data: std::sync::Arc::from(body.as_bytes()),
    }
}
