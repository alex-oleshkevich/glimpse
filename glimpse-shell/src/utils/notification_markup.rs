use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use ammonia::Builder;

static SANITIZER: LazyLock<Builder<'static>> = LazyLock::new(|| {
    let tags: HashSet<&'static str> = ["b", "i", "u", "a"].into_iter().collect();
    let url_schemes: HashSet<&'static str> = ["http", "https", "mailto"].into_iter().collect();
    let mut tag_attributes: HashMap<&'static str, HashSet<&'static str>> = HashMap::new();
    tag_attributes.insert("a", ["href"].into_iter().collect());

    let mut builder = Builder::default();
    builder
        .tags(tags)
        .url_schemes(url_schemes)
        .tag_attributes(tag_attributes)
        .link_rel(None)
        .strip_comments(true);
    builder
});

pub fn sanitize_notification_body(body: &str) -> String {
    if body.is_empty() {
        return String::new();
    }
    SANITIZER.clean(body).to_string()
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

    #[test]
    fn allowed_link_preserved() {
        let out = sanitize_notification_body(r#"<a href="https://example.com">site</a>"#);
        assert!(out.contains(r#"href="https://example.com""#));
        assert!(out.contains(">site</a>"));
    }

    #[test]
    fn javascript_scheme_dropped_text_kept() {
        let out = sanitize_notification_body(r#"<a href="javascript:alert(1)">click</a>"#);
        assert!(!out.contains("javascript"));
        assert!(out.contains("click"));
    }

    #[test]
    fn file_scheme_dropped() {
        let out = sanitize_notification_body(r#"<a href="file:///etc/passwd">x</a>"#);
        assert!(!out.contains("file:"));
        assert!(out.contains("x"));
    }

    #[test]
    fn mailto_scheme_preserved() {
        let out = sanitize_notification_body(r#"<a href="mailto:a@b.c">mail</a>"#);
        assert!(out.contains(r#"href="mailto:a@b.c""#));
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

    #[test]
    fn link_extra_attributes_stripped() {
        let out = sanitize_notification_body(
            r#"<a href="https://x" onclick="bad()" target="_blank">x</a>"#,
        );
        assert!(out.contains(r#"href="https://x""#));
        assert!(!out.contains("onclick"));
        assert!(!out.contains("target"));
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
}
