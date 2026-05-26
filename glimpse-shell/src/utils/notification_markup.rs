use std::collections::HashSet;
use std::sync::LazyLock;

use ammonia::Builder;

static SANITIZER: LazyLock<Builder<'static>> = LazyLock::new(|| {
    // Pango markup recognises only a small element set (b, big, i, s,
    // sub, sup, small, tt, u, span). Crucially, **it does NOT support
    // <a>**. If we let `<a>` through ammonia, GTK's `set_markup` calls
    // `pango_parse_markup`, parsing fails, and GTK falls back to
    // displaying the entire markup string verbatim — including any
    // `&#NNNN;` numeric character references in the body. That bug is
    // what produced "long numbers in place of special characters" in
    // notifications from apps that link-format (Slack, GitHub CLI,
    // Telegram bridges, etc.).
    //
    // So we restrict to the inline-style tags Pango actually renders.
    // Senders' <a> tags collapse to their link text — the URL itself is
    // lost from the panel display, which is far better than the body
    // breaking entirely. We can revisit by routing URLs to tooltips
    // or actions later if anyone needs it.
    let tags: HashSet<&'static str> = ["b", "i", "u"].into_iter().collect();

    let mut builder = Builder::default();
    builder.tags(tags).link_rel(None).strip_comments(true);
    builder
});

pub fn sanitize_notification_body(body: &str) -> String {
    if body.is_empty() {
        return String::new();
    }
    SANITIZER.clean(body).to_string()
}

/// Decodes the 5 XML named entities (`&amp;`, `&lt;`, `&gt;`, `&quot;`,
/// `&apos;`) plus decimal (`&#9733;`) and hex (`&#x2605;`) numeric
/// character references into their UTF-8 form. Anything not recognised
/// (including malformed `&...;` sequences) is preserved verbatim so the
/// user sees their literal text instead of silent corruption.
///
/// Used for notification fields rendered as plain text — `summary` and
/// `app_name` — where `gtk_label_set_text` won't decode entities for
/// us. Without this, an app sending `"Reminder &#9733;"` in the summary
/// would show the literal `"Reminder &#9733;"` on the panel.
pub fn decode_text_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_owned();
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while !rest.is_empty() {
        let Some(amp) = rest.find('&') else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        // Look for the closing `;` within a reasonable window — entities
        // and NCRs are short. Limiting the search prevents a stray `&` at
        // the start of a long string from doing O(n) scans per char.
        let window_end = rest.len().min(16);
        match rest[..window_end].find(';') {
            Some(end) => {
                let body = &rest[1..end];
                match decode_one_reference(body) {
                    Some(ch) => {
                        out.push(ch);
                        rest = &rest[end + 1..];
                    }
                    None => {
                        // Unknown reference — preserve literally.
                        out.push('&');
                        rest = &rest[1..];
                    }
                }
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out
}

fn decode_one_reference(body: &str) -> Option<char> {
    if let Some(hex) = body.strip_prefix("#x").or_else(|| body.strip_prefix("#X")) {
        return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
    }
    if let Some(dec) = body.strip_prefix('#') {
        return dec.parse::<u32>().ok().and_then(char::from_u32);
    }
    match body {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_entities_escaped() {
        assert_eq!(
            sanitize_notification_body("5 < 10 && true"),
            "5 &lt; 10 &amp;&amp; true"
        );
    }

    #[test]
    fn telegram_style_bold_sender_preserved() {
        let out = sanitize_notification_body("<b>Alice</b>\nHey there");
        assert!(out.contains("<b>Alice</b>"));
        assert!(out.contains("Hey there"));
    }

    #[test]
    fn script_tag_and_content_stripped() {
        assert_eq!(
            sanitize_notification_body("<script>alert(1)</script>hi"),
            "hi"
        );
    }

    /// `<a>` tags must be stripped (link text retained) because Pango
    /// markup does not recognise the element. Letting them through
    /// caused `pango_parse_markup` to fail on the whole body and GTK to
    /// fall back to rendering the raw markup verbatim — including any
    /// `&#NNNN;` NCRs ammonia would have decoded.
    #[test]
    fn link_tag_stripped_text_retained() {
        let out = sanitize_notification_body(r#"<a href="https://example.com">site</a>"#);
        assert!(!out.contains("<a"), "unexpected <a> survived: {out}");
        assert!(!out.contains("href"), "href attribute leaked: {out}");
        assert!(out.contains("site"), "link text missing: {out}");
    }

    /// Anchor-scheme filtering still works (the link text survives,
    /// the dangerous scheme doesn't) — and crucially Pango isn't given
    /// an `<a>` to choke on.
    #[test]
    fn javascript_scheme_dropped_text_kept() {
        let out = sanitize_notification_body(r#"<a href="javascript:alert(1)">click</a>"#);
        assert!(!out.contains("javascript"));
        assert!(!out.contains("<a"));
        assert!(out.contains("click"));
    }

    /// Regression test for the original bug: a notification body that
    /// links AND contains an NCR must render correctly. With `<a>`
    /// stripped, Pango can parse the markup, and ammonia decodes the
    /// NCR to its UTF-8 form before serialisation.
    #[test]
    fn link_plus_ncr_in_body_decodes_correctly() {
        let out = sanitize_notification_body(
            r#"New <a href="https://example.com">message</a> &#9733; from Bob"#,
        );
        assert!(!out.contains("<a"), "<a> survived: {out}");
        assert!(!out.contains("&#9733;"), "NCR not decoded: {out}");
        assert!(out.contains('\u{2605}'), "★ missing from {out}");
        assert!(out.contains("message"), "link text missing: {out}");
    }

    #[test]
    fn nested_allowed_tags_preserved() {
        let out = sanitize_notification_body("<b><i>both</i></b>");
        assert!(out.contains("<b>"));
        assert!(out.contains("<i>"));
        assert!(out.contains("</i>"));
        assert!(out.contains("</b>"));
    }

    #[test]
    fn span_attribute_injection_stripped() {
        let out = sanitize_notification_body(r#"<span foreground="red" size="50pt">huge</span>"#);
        assert!(!out.contains("<span"));
        assert!(!out.contains("foreground"));
        assert!(out.contains("huge"));
    }

    /// Even when the sender pads `<a>` with extra attributes, the whole
    /// tag goes away and only the inner text remains.
    #[test]
    fn link_extra_attributes_stripped() {
        let out = sanitize_notification_body(
            r#"<a href="https://x" onclick="bad()" target="_blank">x</a>"#,
        );
        assert!(!out.contains("<a"));
        assert!(!out.contains("onclick"));
        assert!(!out.contains("target"));
        assert!(!out.contains("href"));
        assert!(out.contains('x'));
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(sanitize_notification_body(""), "");
    }

    #[test]
    fn lone_ampersand_escaped() {
        assert_eq!(sanitize_notification_body("AT&T"), "AT&amp;T");
    }

    #[test]
    fn uppercase_tag_normalized() {
        let out = sanitize_notification_body("<B>foo</B>");
        assert!(out.contains("<b>foo</b>"));
    }

    // -- decode_text_entities ----------------------------------------------

    /// The headline use case: a summary like `"Reminder &#9733;"` from a
    /// sender that HTML-encodes its unicode must render as `"Reminder ★"`
    /// in the plain-text title row.
    #[test]
    fn decode_text_entities_resolves_decimal_ncr() {
        assert_eq!(
            decode_text_entities("Reminder &#9733;"),
            "Reminder \u{2605}"
        );
    }

    #[test]
    fn decode_text_entities_resolves_hex_ncr() {
        assert_eq!(
            decode_text_entities("Star &#x2605; here"),
            "Star \u{2605} here"
        );
    }

    #[test]
    fn decode_text_entities_resolves_five_named_entities() {
        assert_eq!(
            decode_text_entities("a &amp; b &lt; c &gt; d &quot;e&quot; &apos;f&apos;"),
            "a & b < c > d \"e\" 'f'"
        );
    }

    /// Multi-byte / 5-digit NCRs (emojis: 🎉 = U+1F389 = 127881) must
    /// round-trip — these are the "8-len numbers" symptom from the bug
    /// report.
    #[test]
    fn decode_text_entities_resolves_emoji_ncr() {
        assert_eq!(decode_text_entities("&#127881; party"), "\u{1f389} party");
    }

    /// Unknown entity names are preserved verbatim — better that the
    /// user sees `&unknownname;` literally than silent text corruption.
    #[test]
    fn decode_text_entities_preserves_unknown_entity() {
        assert_eq!(
            decode_text_entities("hello &whoops; world"),
            "hello &whoops; world"
        );
    }

    /// A lone `&` with no closing `;` is preserved literally; it's not
    /// the decoder's job to escape it.
    #[test]
    fn decode_text_entities_preserves_lone_ampersand() {
        assert_eq!(decode_text_entities("AT&T forever"), "AT&T forever");
    }

    /// Strings with no `&` short-circuit (fast path); just verify the
    /// path returns the right value.
    #[test]
    fn decode_text_entities_passes_plain_text_through() {
        assert_eq!(decode_text_entities("just text 🎉"), "just text 🎉");
        assert_eq!(decode_text_entities(""), "");
    }

    /// An out-of-range NCR (no Unicode scalar above U+10FFFF, surrogates
    /// excluded) is preserved literally rather than producing a
    /// replacement char.
    #[test]
    fn decode_text_entities_preserves_invalid_ncr() {
        assert_eq!(decode_text_entities("&#999999999;"), "&#999999999;");
    }
}
