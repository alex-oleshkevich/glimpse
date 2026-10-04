use super::*;

pub(super) fn check() {
    let inhibitor_list = InhibitorList::new();
    assert!(children_of::<ExpanderRow>(&inhibitor_list).is_empty());
    assert_eq!(
        inhibitor_list.measure(gtk4::Orientation::Vertical, -1).1,
        0,
        "an empty list reserves no height"
    );

    inhibitor_list.set_inhibitors(&[inhibitor(1, "Zoom", true), inhibitor(2, "OBS", false)]);
    let holders = children_of::<ExpanderRow>(&inhibitor_list);
    assert_eq!(holders.len(), 2);
    let head = |item: &ExpanderRow| item.imp().head.get();
    let first = head(&holders[0]);
    let release_of = |item: &ExpanderRow| -> ActionRow {
        item.details()
            .last_child()
            .and_downcast::<ActionRow>()
            .expect("a card ends with Release")
    };
    let release = release_of(&holders[0]);
    assert_eq!(
        release.upcast_ref::<Row>().tone(),
        RowTone::Destructive,
        "Release ends another app's hold, so it reads as destructive"
    );
    assert!(!holders[0].open());

    let touched_row = holders[0].clone();
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
    first.activate();
    assert!(holders[0].open());
    assert!(
        reported.borrow().is_empty(),
        "opening details does not release"
    );
    assert!(
        holders[1].subtitle().is_none() && !holders[1].open(),
        "a hold's reason is in its card, so the row stays one line"
    );
    release.activate();
    assert_eq!(*reported.borrow(), vec![1]);

    inhibitor_list.set_inhibitors(&[inhibitor(9, "Zoom", true), inhibitor(2, "OBS", false)]);
    let reused = children_of::<ExpanderRow>(&inhibitor_list);
    assert_ne!(
        reused[0], holders[0],
        "another hold is another row, so a release can only ever name its own id"
    );
    release_of(&reused[0]).activate();
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
    let holder_without_tabs = children_of::<ExpanderRow>(&inhibitor_list).remove(0);
    let row_without_tabs = head(&holder_without_tabs);
    let chip_icon = child_named::<gtk4::Image>(&row_without_tabs, "row__icon");
    assert_eq!(
        chip_icon.icon_name().as_deref(),
        Some("system-run-symbolic")
    );
    row_without_tabs.activate();
    assert!(
        holder_without_tabs.open(),
        "a hold that prevents more than idle opens to say what, even when it cannot be released"
    );
    assert!(
        !release_of(&holder_without_tabs).get_visible(),
        "an external inhibitor cannot be released"
    );
    let card = holder_without_tabs.details();
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
    let hostile_holder = children_of::<ExpanderRow>(&inhibitor_list).remove(0);
    let hostile_row = head(&hostile_holder);
    let hostile_title = child_named::<gtk4::Label>(&hostile_row, "row__title");
    assert_eq!(hostile_title.text().chars().count(), TEXT_MAX_CHARS);
    let hostile_facts = children_of::<FactList>(&hostile_holder.details()).remove(0);
    assert!(children_of::<LegacyRow>(&hostile_facts).iter().all(|row| {
        row.value()
            .is_none_or(|value| value.chars().count() <= TEXT_MAX_CHARS)
    }));
}
