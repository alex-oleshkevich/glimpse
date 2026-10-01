use super::*;

pub(super) fn check() {
    let placeholder = Placeholder::new();
    let empty_icon = child_named::<gtk4::Image>(&placeholder, "placeholder__icon");
    let empty_title = child_named::<gtk4::Label>(&placeholder, "placeholder__title");
    let empty_body = child_named::<gtk4::Label>(&placeholder, "placeholder__description");

    assert!(
        !empty_icon.is_visible() && !empty_title.is_visible() && !empty_body.is_visible(),
        "a placeholder with nothing to say occupies no space"
    );
    placeholder.set_icon_name(Some("network-wireless-offline-symbolic"));
    placeholder.set_title(Some("Wi-Fi is off"));
    placeholder.set_description(Some("Turn it on to see networks."));
    assert!(empty_icon.is_visible() && empty_title.is_visible() && empty_body.is_visible());
    placeholder.set_icon_name(None::<&str>);
    assert!(
        !empty_icon.is_visible(),
        "a cleared icon gives its space back rather than leaving a gap above the title"
    );
    placeholder.set_icon_name(Some("network-wireless-offline-symbolic"));

    assert!(!placeholder.error());
    placeholder.set_error(true);
    assert!(
        placeholder.has_css_class("placeholder--error"),
        "an unreachable service is the same block in a different colour, not a different \
             widget: the shape a user learned for `empty` is the one they read for `broken`"
    );
    placeholder.set_error(false);
    assert!(!placeholder.has_css_class("placeholder--error"));

    placeholder.set_description(Some("W ".repeat(30)));
    let bounded = placeholder.measure(gtk4::Orientation::Horizontal, -1).1;
    placeholder.set_description(Some("W ".repeat(60)));
    assert_eq!(
        placeholder.measure(gtk4::Orientation::Horizontal, -1).1,
        bounded,
        "a placeholder wraps rather than widening the popover around it"
    );
}
