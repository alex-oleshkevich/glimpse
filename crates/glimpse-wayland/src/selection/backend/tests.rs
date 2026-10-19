use super::*;

fn mimes(list: &[&str]) -> Vec<String> {
    list.iter().map(|mime| (*mime).to_owned()).collect()
}

#[test]
fn the_verdict_on_a_selection_nobody_wants_is_unwanted() {
    assert_eq!(verdict(&mimes(&["text/plain"]), false), Verdict::Unwanted);
    assert_eq!(
        verdict(&mimes(&["text/plain"]), true),
        Verdict::Read("text/plain".to_owned())
    );
}

#[test]
fn a_sensitive_selection_someone_wants_is_still_not_read() {
    assert_eq!(
        verdict(&mimes(&["text/plain", "x-kde-passwordManagerHint"]), true),
        Verdict::Sensitive
    );
}

#[test]
fn the_ladder_prefers_utf8_text_over_every_other_spelling() {
    let chosen = choose(&mimes(&[
        "STRING",
        "text/plain",
        "text/plain;charset=utf-8",
    ]));

    assert_eq!(chosen.as_deref(), Some("text/plain;charset=utf-8"));
}

#[test]
fn an_offer_of_only_an_image_is_read_as_an_image() {
    assert_eq!(choose(&mimes(&["image/png"])).as_deref(), Some("image/png"));
}

#[test]
fn text_is_preferred_to_an_image_offered_beside_it() {
    assert_eq!(
        choose(&mimes(&["image/png", "text/plain"])).as_deref(),
        Some("text/plain")
    );
}

#[test]
fn an_offer_of_nothing_we_understand_is_not_read() {
    assert_eq!(choose(&mimes(&["application/x-private"])), None);
    assert_eq!(choose(&[]), None);
}

#[test]
fn a_mime_is_matched_without_regard_to_casing() {
    assert_eq!(
        choose(&mimes(&["TEXT/PLAIN"])).as_deref(),
        Some("text/plain")
    );
}
