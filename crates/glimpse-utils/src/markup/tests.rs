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

#[test]
fn a_non_breaking_space_survives_as_a_character_rather_than_as_an_entity() {
    assert_eq!(sanitize_body("&nbsp;"), "\u{a0}");
    assert_eq!(sanitize_body("a\u{a0}b"), "a\u{a0}b");
}

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

#[test]
fn a_br_tag_is_a_line_break() {
    assert_eq!(
        sanitize_body("one<br/>two<BR>three<br />four"),
        "one\ntwo\nthree four"
    );
}

#[test]
fn sanitizing_twice_changes_nothing() {
    let once = sanitize_body("<b>Alice</b>&nbsp;AT&T<br/>5 < 10");

    assert_eq!(sanitize_body(&once), once);
}

#[test]
fn line_breaks_past_the_body_line_limit_become_spaces() {
    let body = sanitize_body(&"x\n".repeat(1024));
    assert_eq!(body.matches('\n').count(), BODY_MAX_LINES - 1);
    assert_eq!(
        sanitize_body(&"x<br>".repeat(8)).matches('\n').count(),
        BODY_MAX_LINES - 1
    );
    assert_eq!(sanitize_body(&body), body);
}

#[test]
fn generic_attributes_are_stripped_from_kept_tags() {
    assert_eq!(sanitize_body(r#"<b lang="en" title="t">x</b>"#), "<b>x</b>");
}

#[test]
fn numeric_references_to_hostile_characters_do_not_survive() {
    for body in [
        "Lunch&#x202e;gpj.exe",
        "a&#x2066;b&#x2069;c",
        "one&#x1b;two",
    ] {
        let once = sanitize_body(body);
        assert!(
            !once.chars().any(|c| c != '\n' && super::hostile(c)),
            "{once:?}"
        );
        assert_eq!(sanitize_body(&once), once);
    }
}
