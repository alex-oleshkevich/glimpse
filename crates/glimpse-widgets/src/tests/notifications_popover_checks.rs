use super::*;

pub(super) fn check() {
    let popover = NotificationsPopover::new();
    let popover_imp = popover.imp();
    let group = |key: &str, app: &str, count: usize| Group {
        key: key.to_owned(),
        app_name: app.to_owned(),
        notifications: (0..count)
            .map(|at| Notification {
                key: format!("{key}-{at}"),
                summary: format!("{app} {at}"),
                ..Notification::default()
            })
            .collect(),
    };

    assert!(
        popover_imp.empty.get_visible()
            && !popover_imp.groups.get_visible()
            && !popover_imp.clear.get_visible(),
        "a popover with nothing in it offers no way to clear it"
    );
    let empty_width = popover.measure(gtk4::Orientation::Horizontal, -1).0;

    assert_eq!(popover_imp.shell.size(), PopoverSize::Wide);
    let scroller = &popover_imp.shell.imp().scroller;
    assert!(
        scroller.propagates_natural_height()
            && scroller.hscrollbar_policy() == gtk4::PolicyType::Never,
        "a long notification feed scrolls vertically without changing the popover width"
    );
    assert_eq!(
        popover_imp.column.parent().as_ref(),
        Some(popover_imp.shell.body().upcast_ref())
    );

    popover.set_groups(&[group("a", "Telegram", 2), group("b", "PagerDuty", 1)]);
    let sections = group_sections(&popover_imp.groups.get());
    let popover_stack = child_named::<NotificationStack>(&sections[0], "notification-stack");
    assert!(
        popover_stack.imp().animated.get(),
        "popover groups enable the stack transition at their single construction site"
    );
    let popover_card = children_of::<NotificationCard>(&popover_stack)[0].clone();
    let popup_entry = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    popup_entry.add_css_class("notification-popup__entry");
    let popup_card = NotificationCard::new();
    popup_card.set_summary(Some("Telegram 0"));
    popup_entry.append(&popup_card);
    assert_eq!(
        empty_width,
        popover.measure(gtk4::Orientation::Horizontal, -1).0,
        "an empty notifications popover keeps the same width as a populated one"
    );
    assert_eq!(
        popover_card.measure(gtk4::Orientation::Horizontal, -1).0,
        popup_card.measure(gtk4::Orientation::Horizontal, -1).0,
        "the notification card owns one width in both the popover and popup hierarchies"
    );
    assert_eq!(sections.len(), 2);
    assert!(
        sections
            .iter()
            .all(|section| section.has_css_class("notifications-popover__group")),
        "notification sections carry the group spacing contract"
    );
    assert!(
        sections.iter().all(|section| section.title().is_none()),
        "the app name lives in each card header rather than a duplicated group title"
    );
    assert_eq!(
        sections[0].count(),
        None,
        "the stack control is the group's only count"
    );
    assert_eq!(
        sections[1].count(),
        None,
        "a section does not duplicate the stack's count slot"
    );
    let group_actions = sections[0]
        .imp()
        .trail
        .first_child()
        .and_downcast::<gtk4::Box>()
        .expect("notification group actions");
    let trail_buttons = children_of::<gtk4::Button>(&group_actions);
    assert_eq!(trail_buttons.len(), 2);
    assert!(trail_buttons[0].has_css_class("notification-stack__chip"));
    assert!(trail_buttons[1].has_css_class("section__clear"));
    assert!(popover_imp.clear.get_visible() && !popover_imp.empty.get_visible());

    popover.set_groups(&[group("a", "", 2)]);
    let anonymous = group_sections(&popover_imp.groups.get())[0].clone();
    assert!(!anonymous.imp().title.get_visible());
    assert!(anonymous.imp().trail.property::<bool>("hexpand"));
    assert_eq!(anonymous.imp().trail.halign(), gtk4::Align::End);

    popover.set_groups(&[group("b", "PagerDuty", 1)]);
    assert_eq!(
        group_sections(&popover_imp.groups.get()).len(),
        1,
        "a group that goes away takes its section with it"
    );

    let cleared = Rc::new(RefCell::new(Vec::<String>::new()));
    popover.connect_clear_group({
        let cleared = Rc::clone(&cleared);
        move |_, key| cleared.borrow_mut().push(key)
    });
    let dismissed = Rc::new(RefCell::new(Vec::<String>::new()));
    popover.connect_dismissed({
        let dismissed = Rc::clone(&dismissed);
        move |_, key| dismissed.borrow_mut().push(key)
    });

    popover.set_groups(&[group("a", "Telegram", 5)]);
    let dense = group_sections(&popover_imp.groups.get())[0].clone();
    let dense_stack = child_named::<NotificationStack>(&dense, "notification-stack");
    let shown = |stack: &NotificationStack| {
        children_of::<NotificationCard>(stack)
            .iter()
            .filter(|row| row.get_visible())
            .count()
    };
    let dense_chip = dense_stack
        .imp()
        .chip
        .get()
        .expect("a notification stack builds its chip once")
        .clone();

    assert!(
        dense_stack.is_collapsed()
            && shown(&dense_stack) == 1
            && dense_stack.imp().strips.borrow().len() == crate::notification_stack::MAX_DEPTH
            && dense_chip.get_visible(),
        "a notification group is rendered by the collapsed stack widget"
    );
    assert!(
        child_named::<gtk4::Button>(&dense, "section__clear").get_visible(),
        "a stacked notification group offers a group clear button"
    );

    popover.set_groups(&[group("single", "Telegram", 1)]);
    let single = group_sections(&popover_imp.groups.get())[0].clone();
    assert!(
        !child_named::<gtk4::Button>(&single, "section__clear").get_visible(),
        "an unstacked notification group does not offer a group clear button"
    );

    popover.set_groups(&[group("a", "Telegram", 5)]);
    let dense = group_sections(&popover_imp.groups.get())[0].clone();
    let dense_stack = child_named::<NotificationStack>(&dense, "notification-stack");
    let dense_chip = dense_stack
        .imp()
        .chip
        .get()
        .expect("a notification stack builds its chip once")
        .clone();

    secondary_click(&children_of::<NotificationCard>(&dense_stack)[0]);
    assert_eq!(
        *cleared.borrow(),
        vec!["a".to_owned()],
        "right-clicking the collapsed stack reports its group"
    );

    dense_chip.emit_clicked();
    assert!(
        !dense_stack.is_collapsed() && shown(&dense_stack) == 5,
        "the stack control opens the group"
    );
    let dense_rows = children_of::<NotificationCard>(&dense_stack);
    secondary_click(&dense_rows[1]);
    child_named::<gtk4::Button>(&dense_rows[2], "notification__close").emit_clicked();
    assert_eq!(
        *dismissed.borrow(),
        vec!["a-1".to_owned(), "a-2".to_owned()],
        "an expanded stack forwards both right-click and close with the selected card key"
    );

    dense_chip.emit_clicked();
    assert!(
        dense_stack.is_collapsed() && shown(&dense_stack) == 1,
        "the same stack control closes the group"
    );

    popover.set_groups(&[group("a", "Telegram", 1)]);
    assert!(
        !dense_chip.get_visible() && shown(&dense_stack) == 1,
        "a group reduced to one notification takes the stack control away"
    );

    let group_clear = child_named::<gtk4::Button>(&dense, "section__clear");
    assert_eq!(
        group_clear.icon_name().as_deref(),
        Some("window-close-symbolic")
    );
    group_clear.emit_clicked();
    assert_eq!(
        *cleared.borrow(),
        vec!["a".to_owned(), "a".to_owned()],
        "a section reports the group it stands for rather than where it sits"
    );
    let toggles = Rc::new(RefCell::new(Vec::new()));
    popover.connect_dnd_toggled({
        let toggles = Rc::clone(&toggles);
        move |_, silenced| toggles.borrow_mut().push(silenced)
    });

    popover.set_dnd(true);
    assert!(popover.dnd());
    assert!(!popover_imp.notifications.is_active());
    assert_eq!(
        *toggles.borrow(),
        Vec::<bool>::new(),
        "showing the state the caller already knows about must not report it back, or the \
             two ends chase each other"
    );

    popover.set_dnd(false);
    assert!(!popover.dnd());
    assert!(popover_imp.notifications.is_active());
    assert!(toggles.borrow().is_empty());

    popover_imp.notifications.set_active(false);
    assert_eq!(
        *toggles.borrow(),
        [true],
        "turning notifications off reports do-not-disturb on"
    );
    popover_imp.notifications.set_active(true);
    assert_eq!(
        *toggles.borrow(),
        [true, false],
        "turning notifications on reports do-not-disturb off"
    );

    assert!(!popover_imp.trouble.get_visible());
    popover.set_trouble(Some("org.freedesktop.Notifications is taken."));
    assert!(popover_imp.trouble.get_visible());
    popover.set_trouble(None);
    assert!(!popover_imp.trouble.get_visible());

    popover.set_groups(&[]);
    assert!(
        popover_imp.empty.get_visible() && !popover_imp.clear.get_visible(),
        "clearing the last notification puts the placeholder back and takes the row away"
    );

    let fired = Rc::new(RefCell::new(Vec::<&str>::new()));
    popover.connect_clear_all({
        let fired = Rc::clone(&fired);
        move |_| fired.borrow_mut().push("clear")
    });

    popover_imp.clear.activate();

    assert_eq!(*fired.borrow(), ["clear"]);
}
