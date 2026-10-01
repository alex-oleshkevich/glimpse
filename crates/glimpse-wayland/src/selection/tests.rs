use super::*;

fn mimes(list: &[&str]) -> Vec<String> {
    list.iter().map(|mime| (*mime).to_owned()).collect()
}

#[test]
fn a_sensitive_selection_reads_nothing_and_skips_every_mime() {
    let offered = mimes(&["text/plain", "x-kde-passwordManagerHint", "UTF8_STRING"]);

    let (wanted, skipped) = classify(&offered);

    assert!(wanted.is_empty(), "nothing may be read, got {wanted:?}");
    assert_eq!(
        skipped,
        offered
            .iter()
            .map(|mime| (mime.clone(), SkipReason::Sensitive))
            .collect::<Vec<_>>()
    );
}

#[test]
fn an_ordinary_selection_reads_every_offered_mime() {
    let offered = mimes(&["text/plain", "image/png"]);

    let (wanted, skipped) = classify(&offered);

    assert_eq!(wanted, offered);
    assert!(skipped.is_empty());
}

#[test]
fn neither_content_reaches_a_debug_line() {
    let entry = Entry {
        mime: "text/plain".to_owned(),
        data: Arc::from(&b"hunter2"[..]),
    };

    let rendered = format!("{entry:?}");

    assert!(!rendered.contains("hunter2"), "got {rendered}");
    assert!(rendered.contains("bytes: 7"), "got {rendered}");
}
