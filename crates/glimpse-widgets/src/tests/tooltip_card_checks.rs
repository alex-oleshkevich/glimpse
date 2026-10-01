use super::*;

pub(super) fn check() {
    let card = TooltipCard::new();
    assert!(
        !card.get_visible(),
        "a card with nothing in it shows no tooltip rather than an empty box"
    );
    card.set_title(Some("Nextcloud"));
    assert!(card.get_visible());
    assert_eq!(card.title().as_deref(), Some("Nextcloud"));

    let long = "ы".repeat(tooltip_card::TITLE_MAX_CHARS * 2);
    card.set_title(Some(long.as_str()));
    assert_eq!(
        card.title().unwrap_or_default().chars().count(),
        tooltip_card::TITLE_MAX_CHARS,
        "a hostile title is cut by characters, not bytes"
    );

    let long_body = "é".repeat(tooltip_card::BODY_MAX_CHARS + 40);
    card.set_body(Some(long_body.as_str()));
    assert_eq!(
        card.body().unwrap_or_default().chars().count(),
        tooltip_card::BODY_MAX_CHARS,
        "the card's own cap is what applies; the shared setter's 128 would silently win"
    );

    card.set_body(Some("Synced\nLast sync 2 minutes ago"));
    assert_eq!(
        card.body().as_deref(),
        Some("Synced\nLast sync 2 minutes ago")
    );
    card.set_status(Some("Needs attention"));
    assert_eq!(card.status().as_deref(), Some("Needs attention"));

    card.set_title(None::<&str>);
    card.set_body(None::<&str>);
    card.set_status(None::<&str>);
    assert!(
        !card.get_visible(),
        "emptying every field hides the card again"
    );
    card.set_icon(Some(&gio::ThemedIcon::new("folder-symbolic").upcast()));
    assert!(card.get_visible(), "an icon alone is still a tooltip");
    assert!(
        !card.imp().text.get_visible(),
        "an icon-only card reserves no text column"
    );
}
