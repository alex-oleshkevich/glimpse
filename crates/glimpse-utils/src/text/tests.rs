use super::clean;

#[test]
fn text_off_a_feed_is_flattened_and_capped() {
    assert_eq!(clean("  Design\treview\n\n", 120), "Design review");
    assert_eq!(
        clean("a\u{7}b", 120),
        "a b",
        "a control character separates rather than vanishes, so it cannot splice two words"
    );
    assert_eq!(
        clean("Lunch\u{202e}gpj.exe", 120),
        "Lunch gpj.exe",
        "a bidi override is not a control character, and Pango honours it: left in, it \
             reorders the row it lands in"
    );
    assert_eq!(clean("a\u{2066}b\u{2069}c", 120), "a b c");
    assert_eq!(clean("abcdef", 4), "abcd…");
    assert_eq!(
        clean("ééééé", 3),
        "ééé…",
        "the cap counts characters, and slicing bytes would panic here"
    );
    assert_eq!(
        clean("ab cd", 3),
        "ab…",
        "a space pending at the cap counts toward it, so the result never runs past cap plus the ellipsis"
    );
    assert_eq!(clean("ab cd", 4), "ab c…");
    assert_eq!(clean("   ", 120), "");
}
