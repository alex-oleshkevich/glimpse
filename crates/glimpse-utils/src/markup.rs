use std::collections::HashSet;
use std::sync::LazyLock;

use ammonia::Builder;

pub const BODY_MAX_CHARS: usize = 2048;

const NBSP: &str = "&nbsp;";

static SANITIZER: LazyLock<Builder<'static>> = LazyLock::new(|| {
    let tags: HashSet<&'static str> = ["b", "i", "u"].into_iter().collect();
    let mut builder = Builder::default();
    builder.tags(tags).link_rel(None).strip_comments(true);
    builder
});

fn hostile(character: char) -> bool {
    (character.is_control() && character != '\n')
        || matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

pub fn sanitize_body(body: &str) -> String {
    let bounded: String = body
        .chars()
        .take(BODY_MAX_CHARS)
        .map(|character| match hostile(character) {
            true => ' ',
            false => character,
        })
        .collect();

    SANITIZER
        .clean(&bounded)
        .to_string()
        .replace(NBSP, "\u{a0}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_telegram_shape_keeps_its_bold_and_its_newline() {
        assert_eq!(
            sanitize_body("<b>Alice</b>\nHey there"),
            "<b>Alice</b>\nHey there"
        );
    }

    #[test]
    fn a_bare_ampersand_is_escaped_rather_than_left_to_break_the_parse() {
        assert_eq!(sanitize_body("AT&T"), "AT&amp;T");
        assert_eq!(sanitize_body("5 < 10 && true"), "5 &lt; 10 &amp;&amp; true");
    }

    #[test]
    fn a_span_is_stripped_with_its_attributes_and_its_text_kept() {
        assert_eq!(
            sanitize_body(r#"<span foreground="red" size="50pt">huge</span>"#),
            "huge"
        );
    }

    #[test]
    fn a_link_collapses_to_its_text_and_the_star_beside_it_decodes() {
        assert_eq!(
            sanitize_body(r#"New <a href="https://example.com">message</a> &#9733; from Bob"#),
            "New message \u{2605} from Bob"
        );
    }

    #[test]
    fn a_script_goes_with_its_content() {
        assert_eq!(sanitize_body("<script>alert(1)</script>hi"), "hi");
        assert_eq!(
            sanitize_body(r#"<a href="javascript:alert(1)">click</a>"#),
            "click"
        );
    }

    #[test]
    fn tags_are_normalised_nested_and_balanced() {
        assert_eq!(sanitize_body("<B>foo</B>"), "<b>foo</b>");
        assert_eq!(sanitize_body("<b><i>both</i></b>"), "<b><i>both</i></b>");
        assert_eq!(sanitize_body("<b>hello"), "<b>hello</b>");
    }

    /// html5ever's serializer emits exactly one named entity, `&nbsp;`, and it is the one entity
    /// Pango does not know — a body carrying a non-breaking space otherwise fails to parse and
    /// `GtkLabel` renders it as nothing at all. Measured in both directions: a literal U+00A0 in
    /// the input comes back out as the entity, so the replacement covers both.
    #[test]
    fn a_non_breaking_space_survives_as_a_character_rather_than_as_an_entity() {
        assert_eq!(sanitize_body("&nbsp;"), "\u{a0}");
        assert_eq!(sanitize_body("a\u{a0}b"), "a\u{a0}b");
    }

    /// Every other named entity is decoded by ammonia itself, which is why no entity table is
    /// needed here.
    #[test]
    fn the_other_named_entities_decode_to_characters() {
        assert_eq!(
            sanitize_body("&mdash;&hellip;&rsquo;&copy;&euro;"),
            "\u{2014}\u{2026}\u{2019}\u{a9}\u{20ac}"
        );
        assert_eq!(sanitize_body("&whoops;"), "&amp;whoops;");
    }

    #[test]
    fn a_bidi_override_does_not_survive_into_the_markup() {
        let cleaned = sanitize_body("<b>Lunch\u{202e}gpj.exe</b>");

        assert!(!cleaned.contains('\u{202e}'), "{cleaned}");
        assert_eq!(cleaned, "<b>Lunch gpj.exe</b>");
    }

    #[test]
    fn a_newline_survives_where_every_other_control_character_becomes_a_space() {
        assert_eq!(sanitize_body("one\ntwo"), "one\ntwo");
        assert_eq!(sanitize_body("one\u{7}two"), "one two");
    }

    #[test]
    fn a_body_past_the_cap_is_bounded_by_characters_and_not_by_bytes() {
        let long = "é".repeat(BODY_MAX_CHARS * 2);

        assert_eq!(sanitize_body(&long).chars().count(), BODY_MAX_CHARS);
    }
}
