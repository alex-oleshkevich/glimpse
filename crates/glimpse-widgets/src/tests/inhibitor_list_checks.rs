use super::*;

pub(super) fn check() {
    let inhibitor_list = InhibitorList::new();
    assert!(children_of::<Expandable>(&inhibitor_list).is_empty());
    assert_eq!(
        inhibitor_list.measure(gtk4::Orientation::Vertical, -1).1,
        0,
        "an empty list reserves no height"
    );

    inhibitor_list.set_inhibitors(&[inhibitor(1, "Zoom", true), inhibitor(2, "OBS", false)]);
    let holders = children_of::<Expandable>(&inhibitor_list);
    assert_eq!(holders.len(), 2);
    let first = holders[0].head::<LegacyRow>().expect("an inhibitor row");
    let second = holders[1].head::<LegacyRow>().expect("an inhibitor row");
    let _chevron = child_named::<gtk4::Image>(&first, "drawer-chevron");
    let release_of = |holder: &Expandable| -> LegacyRow {
        holder
            .details::<gtk4::Box>()
            .and_then(|card| card.last_child())
            .and_downcast::<LegacyRow>()
            .expect("a card ends with Release")
    };
    let release = release_of(&holders[0]);
    assert!(
        release.activatable() && release.has_css_class("row--destructive"),
        "Release ends another app's hold, so it reads as destructive"
    );
    assert!(!holders[0].expanded());

    let touched_row = first.clone();
    touched_row.set_title(Some("Touched by hand"));
    inhibitor_list.set_inhibitors(&[inhibitor(1, "Zoom", true), inhibitor(2, "OBS", false)]);
    assert_eq!(
        touched_row.title().as_deref(),
        Some("Touched by hand"),
        "an unchanged slice is not re-applied: re-rendering it would overwrite what was \
             touched by hand"
    );

    let title_label = child_named::<gtk4::Label>(&first, "row__title");
    let title_changes = Rc::new(Cell::new(0u32));
    title_label.connect_label_notify({
        let title_changes = Rc::clone(&title_changes);
        move |_| title_changes.set(title_changes.get() + 1)
    });
    inhibitor_list.set_inhibitors(&[
        inhibitor(1, "Zoom (sharing)", true),
        inhibitor(2, "OBS", false),
    ]);
    assert_eq!(
        title_changes.get(),
        1,
        "a changed slice re-applies the row it changed, undoing the hand touch"
    );

    let reported = Rc::new(RefCell::new(Vec::<u64>::new()));
    inhibitor_list.connect_release_requested({
        let reported = Rc::clone(&reported);
        move |_, id| reported.borrow_mut().push(id)
    });
    first.emit_clicked();
    assert!(holders[0].expanded());
    assert!(
        reported.borrow().is_empty(),
        "opening details does not release"
    );
    assert!(
        second.activatable() && second.subtitle().is_none() && !holders[1].expanded(),
        "a hold's reason is in its card, so even one that only prevents idle opens"
    );
    release.emit_clicked();
    assert_eq!(*reported.borrow(), vec![1]);

    inhibitor_list.set_inhibitors(&[inhibitor(9, "Zoom", true), inhibitor(2, "OBS", false)]);
    let reused = children_of::<Expandable>(&inhibitor_list);
    assert_ne!(
        reused[0], holders[0],
        "another hold is another row, so a release can only ever name its own id"
    );
    release_of(&reused[0]).emit_clicked();
    assert_eq!(*reported.borrow(), vec![1, 9]);

    let entry_without_tabs = InhibitorEntry {
        id: 5,
        source: InhibitorSource::Login1,
        label: "systemd-inhibit".to_owned(),
        status: "installing updates".to_owned(),
        targets: InhibitorTargets {
            idle: true,
            shutdown: true,
            power_key: true,
            ..InhibitorTargets::default()
        },
        can_release: false,
    };
    inhibitor_list.set_inhibitors(&[entry_without_tabs]);
    let holder_without_tabs = children_of::<Expandable>(&inhibitor_list).remove(0);
    let row_without_tabs = holder_without_tabs.head::<LegacyRow>().expect("a row");
    let chip_icon = child_named::<gtk4::Image>(&row_without_tabs, "row__icon");
    assert_eq!(
        chip_icon.icon_name().as_deref(),
        Some("system-run-symbolic")
    );
    row_without_tabs.emit_clicked();
    assert!(
        holder_without_tabs.expanded(),
        "a hold that prevents more than idle opens to say what, even when it cannot be released"
    );
    assert!(
        !release_of(&holder_without_tabs).get_visible(),
        "an external inhibitor cannot be released"
    );
    let card = holder_without_tabs.details::<gtk4::Box>().expect("a card");
    let facts = children_of::<FactList>(&card).remove(0);
    let values = children_of::<LegacyRow>(&facts)
        .iter()
        .filter_map(LegacyRow::value)
        .collect::<Vec<_>>();
    assert_eq!(
        values.len(),
        2,
        "the card says why the hold is taken, then what it prevents"
    );
    assert!(values[1].contains("Shutdown"));
    let hostile = "ё".repeat(TEXT_MAX_CHARS * 2);
    inhibitor_list.set_inhibitors(&[InhibitorEntry {
        id: 7,
        source: InhibitorSource::Portal,
        label: hostile.clone(),
        status: hostile,
        targets: InhibitorTargets::default(),
        can_release: false,
    }]);
    let hostile_holder = children_of::<Expandable>(&inhibitor_list).remove(0);
    let hostile_row = hostile_holder.head::<LegacyRow>().expect("a row");
    let hostile_title = child_named::<gtk4::Label>(&hostile_row, "row__title");
    assert_eq!(hostile_title.text().chars().count(), TEXT_MAX_CHARS);
    let hostile_facts =
        children_of::<FactList>(&hostile_holder.details::<gtk4::Box>().expect("a card")).remove(0);
    assert!(children_of::<LegacyRow>(&hostile_facts).iter().all(|row| {
        row.value()
            .is_none_or(|value| value.chars().count() <= TEXT_MAX_CHARS)
    }));
}
