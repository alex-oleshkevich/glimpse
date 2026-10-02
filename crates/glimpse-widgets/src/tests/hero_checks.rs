use super::*;

pub(super) fn check() {
    let hero = Hero::new();
    let title = child_named::<gtk4::Label>(&hero, "hero__title");
    let subtitle = child_named::<gtk4::Label>(&hero, "hero__subtitle");
    assert!(!title.is_visible() && !subtitle.is_visible());

    hero.set_title(Some("Wi-Fi"));
    assert!(title.is_visible());
    hero.set_subtitle(Some("Connected"));
    assert_eq!(subtitle.text(), "Connected");

    hero.set_title(Some("ё".repeat(TEXT_MAX_CHARS * 2)));
    assert_eq!(
        title.text().chars().count(),
        TEXT_MAX_CHARS,
        "an unbounded title is capped without slicing a multi-byte character"
    );

    let hero_icon = child_named::<gtk4::Image>(&hero, "hero__icon");
    let icon_changes = Rc::new(Cell::new(0u32));
    hero_icon.connect_gicon_notify({
        let icon_changes = Rc::clone(&icon_changes);
        move |_| icon_changes.set(icon_changes.get() + 1)
    });
    hero.set_icon(Some(&gio::ThemedIcon::new("network-wireless").upcast()));
    hero.set_icon(Some(&gio::ThemedIcon::new("network-wireless").upcast()));
    assert_eq!(icon_changes.get(), 1, "an equal icon is not reapplied");

    let switch = gtk4::Switch::new();
    hero.set_slot(&switch);
    assert_eq!(
        switch.parent().and_then(|slot| slot.parent()),
        Some(hero.clone().upcast())
    );
    hero.clear_slot();
    assert!(
        switch.parent().is_none(),
        "a cleared slot unparents its child"
    );

    let shell = PopoverShell::new();
    let hero_box = child_named::<gtk4::Box>(&shell, "popover-shell__hero");
    let footer_box = child_named::<gtk4::Box>(&shell, "popover-shell__footer");
    let rules = separators(shell.upcast_ref());
    assert_eq!(
        rules.len(),
        2,
        "one hairline above the footer, one below the hero"
    );
    assert!(
        !hero_box.is_visible() && !rules[0].is_visible(),
        "an absent hero leaves neither space nor a stray hairline"
    );
    assert!(!footer_box.is_visible() && !rules[1].is_visible());

    shell.set_hero(&hero);
    assert!(hero_box.is_visible() && rules[0].is_visible());
    shell.clear_hero();
    assert!(
        !hero_box.is_visible() && !rules[0].is_visible(),
        "the hairline goes back with the section it belongs to"
    );
    assert!(
        hero.parent().is_none(),
        "a cleared hero unparents its widget"
    );

    let plain = gtk4::Label::new(Some("a hero the shell has never heard of"));
    shell.set_hero(&plain);
    assert!(
        hero_box.is_visible(),
        "any widget is a hero; the shell does not require its own type"
    );

    let content = gtk4::Label::new(Some("body"));
    shell.set_content(&content);
    let replacement = gtk4::Label::new(Some("body again"));
    shell.set_content(&replacement);
    assert!(
        content.parent().is_none(),
        "content is one child, so a second setter unparents the first"
    );

    let button = gtk4::Button::new();
    shell.append_to_footer(&button);
    assert!(footer_box.is_visible() && rules[1].is_visible());
    shell.clear_footer();
    assert!(!footer_box.is_visible() && !rules[1].is_visible());
    assert!(button.parent().is_none());
}

fn separators(widget: &gtk4::Widget) -> Vec<gtk4::Separator> {
    let mut found = Vec::new();
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Ok(rule) = current.clone().downcast::<gtk4::Separator>() {
            found.push(rule);
        }
        found.extend(separators(&current));
        child = current.next_sibling();
    }
    found
}

pub(super) fn heights() {
    let hero_height = |subtitle: Option<&str>, slot: Option<gtk4::Widget>| {
        let hero = Hero::new();
        hero.set_title(Some("Wi-Fi"));
        hero.set_subtitle(subtitle);
        if let Some(slot) = slot {
            hero.set_slot(&slot);
        }
        hero.measure(gtk4::Orientation::Vertical, -1).1
    };
    let reading = Readout::new();
    reading.set_value(Some("72"));
    reading.set_unit(Some("%"));
    reading.set_visible(true);
    let swatch = Swatch::default();
    swatch.add_css_class("swatch--hero");
    let bare = hero_height(None, None);
    assert!(
        [
            hero_height(Some("Connected"), None),
            hero_height(Some("Connected"), Some(gtk4::Switch::new().upcast())),
            hero_height(Some("Connected"), Some(reading.upcast())),
            hero_height(Some("Connected"), Some(swatch.upcast())),
        ]
        .iter()
        .all(|height| *height == bare),
        "every header is one height, whatever its subtitle and slot hold"
    );
}
