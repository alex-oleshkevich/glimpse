use super::plain;

#[test]
fn refused_markup_reads_as_text() {
    assert_eq!(
        plain(r#"<b>Alice</b>&nbsp;<a href="https://x">said hello</a>"#),
        "Alice\u{a0}said hello"
    );
    assert_eq!(plain("&whoops; &#9733; &#x2605; &amp;"), "&whoops; ★ ★ &");
    assert_eq!(
        plain(&format!("&{}", "é".repeat(6))),
        format!("&{}", "é".repeat(6))
    );
}
