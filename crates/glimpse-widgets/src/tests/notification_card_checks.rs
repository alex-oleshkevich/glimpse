use super::*;

pub(super) fn check() {
    let item = NotificationCard::new();
    let item_summary = child_named::<gtk4::Label>(&item, "notification__summary");
    let item_body = child_named::<gtk4::Label>(&item, "notification__body");
    let item_icon = child_named::<gtk4::Image>(&item, "notification__app-icon");
    let item_actions = child_named::<gtk4::Box>(&item, "notification__actions");
    let item_close = child_named::<gtk4::Button>(&item, "notification__close");
    assert!(
        !item_summary.get_visible()
            && !item_body.get_visible()
            && !item_icon.get_visible()
            && !item_actions.get_visible(),
        "a notification with no content reserves no content slot"
    );
    assert!(
        item_close.get_visible(),
        "every notification exposes its close control without waiting for unread state"
    );
    item.set_activatable(false);
    assert!(
        !item.is_focusable() && !item.has_css_class("notification--activatable"),
        "a card with no default action neither takes a click nor advertises one"
    );
    item.set_activatable(true);
    assert!(item.is_focusable() && item.has_css_class("notification--activatable"));

    let content = child_named::<gtk4::Box>(&item, "notification__content");
    let header = child_named::<NotificationHeader>(&item, "notification__header");
    let text = child_named::<NotificationTextBody>(&item, "notification__text-body");
    let image = child_named::<NotificationImageBody>(&item, "notification__image-body");
    let body_group = content.parent().expect("body group");
    assert!(
        text.parent().as_ref() == Some(content.upcast_ref())
            && image.parent().as_ref() == Some(&body_group)
            && body_group.parent() == header.parent(),
        "the text and optional image share a body row below the full-width header"
    );

    {
        let shot = NotificationCard::new();
        shot.set_summary(Some("Screenshot captured"));
        shot.set_body(Some("Saved to Pictures"));
        let shot_picture = child_named::<gtk4::Picture>(&shot, "notification__image");

        shot.set_image(Some(&texture(64, 64)));
        assert!(
            child_named::<gtk4::Label>(&shot, "notification__summary").get_visible()
                && child_named::<gtk4::Label>(&shot, "notification__body").get_visible()
                && shot_picture.get_visible(),
            "an image notification keeps its title and body beside the thumbnail"
        );
        let bounded = shot_picture.paintable().expect("an image");
        assert_eq!(
            (bounded.intrinsic_width(), bounded.intrinsic_height()),
            (64, 64),
            "the thumbnail occupies the former 64px media slot"
        );

        let same = texture(64, 64);
        shot.set_image(Some(&same));
        let first = shot_picture.paintable().expect("an image");
        shot.set_image(Some(&same));
        assert!(
            shot_picture.paintable().expect("an image").eq(&first),
            "bound builds a new texture every time it resamples, so an unchanged image must \
                 be compared on the source rather than on the result"
        );

        shot.set_image(Some(&texture(8000, 6000)));
        assert!(
            !shot.imp().image.get_visible()
                && child_named::<gtk4::Label>(&shot, "notification__summary").get_visible(),
            "download copies the whole image, and the image is somebody else's: past the \
                 ceiling the thumbnail is dropped without hiding the notification title"
        );

        shot.set_image(None);
        assert!(
            !shot.imp().image.get_visible(),
            "a notification without an image leaves no thumbnail gap"
        );
    }

    let item_icon_changes = Rc::new(Cell::new(0u32));
    item.set_icon_name(Some("dialog-information-symbolic"));
    item_icon.connect_icon_name_notify({
        let item_icon_changes = Rc::clone(&item_icon_changes);
        move |_| item_icon_changes.set(item_icon_changes.get() + 1)
    });
    item.set_icon_name(Some("dialog-information-symbolic"));
    assert_eq!(item_icon_changes.get(), 0, "an equal icon is not reapplied");

    item.set_summary(Some("é".repeat(TEXT_MAX_CHARS * 2).as_str()));
    assert_eq!(
        item_summary.text().chars().count(),
        TEXT_MAX_CHARS,
        "a summary is another application's string, capped by character so it cannot be cut \
             mid-codepoint"
    );

    item.set_body_markup(Some("<b>Alice</b>\nHey there"));
    assert_eq!(
        *item_body.text(),
        *"Alice\nHey there",
        "markup pango accepts is rendered, not shown as tags"
    );

    item.set_body_markup(Some("AT&T"));
    assert_eq!(
        *item_body.text(),
        *"AT&T",
        "markup pango refuses still reads; without the gate GtkLabel renders an empty label \
             and the body disappears"
    );
    assert!(item_body.get_visible());

    item.set_body_markup(Some(
        r#"<b>Alice</b>&nbsp;<a href="https://x">said hello</a>"#,
    ));
    assert_eq!(
        *item_body.text(),
        *"Alice\u{a0}said hello",
        "a refused body reads as its text, not as its tags: handing the markup itself to \
             set_text shows the reader tag soup and looks like a broken application"
    );

    assert_eq!(
        crate::notification_text_body::plain("&whoops; &#9733; &#x2605; &amp;"),
        "&whoops; \u{2605} \u{2605} &",
        "an unrecognised reference is left as written rather than silently swallowed"
    );

    item.set_body_markup(Some("<b>bold</b>"));
    item.set_body(Some("bold"));
    assert!(
        !item_body.uses_markup(),
        "a body switching from markup to the same plain string reads as unchanged by text \
             alone, and short-circuiting there would leave the bold still applied"
    );

    item.set_body(Some(
        "a".repeat(glimpse_utils::markup::BODY_MAX_CHARS * 2)
            .as_str(),
    ));
    assert_eq!(
        item_body.text().chars().count(),
        glimpse_utils::markup::BODY_MAX_CHARS
    );

    assert_eq!(item.urgency(), Urgency::Normal);
    item.set_urgency(Urgency::Critical);
    assert_eq!(item.urgency(), Urgency::Critical);
    assert!(
        !item.has_css_class("notification--critical"),
        "urgency is behaviour, and a class no rule in the sheet ever matched was not appearance \
             either"
    );

    let spoken = NotificationCard::new();
    spoken.set_app_name(Some("Telegram"));
    spoken.set_summary(Some("Marta Kaz"));
    spoken.set_body(Some("Are we still on for 14:00?"));
    spoken.set_when(Some("2m"));
    assert_eq!(
        *spoken.imp().accessible_name.borrow(),
        "Telegram. Marta Kaz. Are we still on for 14:00?. 2m",
        "every leaf is presentation, so the whole card reaches a screen reader through the one \
             label the activatable child carries — and the sender and the age are part of it"
    );

    spoken.set_unread(true);
    assert_eq!(
        *spoken.imp().accessible_name.borrow(),
        "Unread. Telegram. Marta Kaz. Are we still on for 14:00?. 2m",
        "unread remains available to assistive technology without a visual adornment"
    );

    assert_eq!(
        *spoken.imp().dismiss_name.borrow(),
        "Dismiss Marta Kaz",
        "twenty cards otherwise give twenty tab stops that each read Dismiss and name nothing"
    );
    spoken.set_summary(None::<&str>);
    assert_eq!(
        *spoken.imp().dismiss_name.borrow(),
        "Dismiss",
        "with no summary to name there is nothing to interpolate"
    );

    let action = |key: &str| Action {
        key: key.to_owned(),
        label: key.to_owned(),
    };
    item.set_actions(&[
        action("reply"),
        action("mute"),
        action("open"),
        action("archive"),
        action("delete"),
    ]);
    let buttons = all_named(&item, "notification__action");
    assert_eq!(
        buttons.len(),
        3,
        "three is where the GNOME HIG and KDE both stop; a longer list grows the card sideways"
    );
    assert!(
        buttons.iter().all(|button| button.has_css_class("flat")),
        "every notification action uses the ghost-button treatment"
    );
    assert!(
        buttons
            .iter()
            .all(|button| !button.has_css_class("notification__action--primary")),
        "notification actions do not invent a visual priority the sender did not declare"
    );
    assert!(item_actions.get_visible());

    let fired = Rc::new(RefCell::new(Vec::<String>::new()));
    item.connect_activated({
        let fired = Rc::clone(&fired);
        move |_| fired.borrow_mut().push("activated".to_owned())
    });
    item.connect_dismissed({
        let fired = Rc::clone(&fired);
        move |_| fired.borrow_mut().push("dismissed".to_owned())
    });
    item.connect_action_invoked({
        let fired = Rc::clone(&fired);
        move |_, key| fired.borrow_mut().push(key)
    });

    item.emit_by_name::<()>("activated", &[]);
    buttons[1]
        .clone()
        .downcast::<gtk4::Button>()
        .expect("an action is a button")
        .emit_clicked();
    child_named::<gtk4::Button>(&item, "notification__close").emit_clicked();
    assert_eq!(
        *fired.borrow(),
        vec![
            "activated".to_owned(),
            "mute".to_owned(),
            "dismissed".to_owned()
        ],
        "a key is read back when the button fires, not captured when it was built"
    );

    let relabelled = [
        Action {
            key: "reply".to_owned(),
            label: "Reply now".to_owned(),
        },
        action("mute"),
        action("open"),
    ];
    item.set_actions(&relabelled);
    assert_eq!(
        all_named(&item, "notification__action")[0]
            .downcast_ref::<gtk4::Button>()
            .and_then(|button| button.label())
            .map(|label| label.to_string()),
        Some("Reply now".to_owned()),
        "a sender that relabels an action without changing its key must still redraw it"
    );

    item.set_actions(&[]);
    assert!(
        all_named(&item, "notification__action").is_empty() && !item_actions.get_visible(),
        "a notification with no actions leaves no strip behind"
    );

    let long_markup = format!("<b>{}</b>", "a".repeat(600));
    item.set_body_markup(Some(long_markup.as_str()));
    assert!(
        item_body.uses_markup() && item_body.text().chars().count() == 600,
        "a long valid markup body must keep its closing tag and render as markup"
    );

    item.set_body_markup(Some(""));
    assert!(
        !item_body.get_visible(),
        "a body that sanitized to nothing leaves no blank line"
    );
}
