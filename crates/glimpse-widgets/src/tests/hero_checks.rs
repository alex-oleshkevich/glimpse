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
