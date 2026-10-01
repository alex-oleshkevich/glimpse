use super::*;

fn item(kind: &str, preview: &str, size: u64) -> ClipboardItem {
    ClipboardItem {
        id: 7,
        kind: kind.to_owned(),
        mime: "image/png".to_owned(),
        pinned: true,
        copied_at_secs: 0,
        size,
        preview: preview.to_owned(),
    }
}

#[test]
fn an_image_reads_as_its_mime_and_size_rather_than_its_preview() {
    assert_eq!(
        preview(&item("image", "ignored", 188_416)),
        "image/png · 184 KiB"
    );
}

#[test]
fn a_text_preview_is_cleaned_and_capped() {
    let long = "a\u{1b}[31m".to_owned() + &"x".repeat(200);
    let shown = preview(&item("text", &long, 0));
    assert!(!shown.contains('\u{1b}'));
    assert!(shown.chars().count() <= PREVIEW + 1);
}

#[test]
fn sizes_step_through_their_units() {
    assert_eq!(size(512), "512 B");
    assert_eq!(size(2048), "2 KiB");
    assert_eq!(size(3 * 1_048_576), "3.0 MiB");
}
