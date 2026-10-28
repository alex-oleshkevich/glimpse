use super::*;

#[test]
fn neither_content_nor_preview_reaches_a_debug_line() {
    let capture = Capture {
        mime: "text/plain".to_owned(),
        data: Arc::from(&b"hunter2"[..]),
    };

    let rendered = format!("{capture:?}");

    assert!(!rendered.contains("hunter2"), "got {rendered}");
    assert!(rendered.contains("bytes: 7"), "got {rendered}");
}
