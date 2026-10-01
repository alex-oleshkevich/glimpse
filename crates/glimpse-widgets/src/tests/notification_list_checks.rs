use super::*;

pub(super) fn check() -> [Notification; 4] {
    let list = NotificationList::new();
    let note = |key: &str, summary: &str| Notification {
        key: key.to_owned(),
        summary: summary.to_owned(),
        when: "now".to_owned(),
        ..Notification::default()
    };
    let rows_of = |list: &NotificationList| children_of::<NotificationCard>(list);

    assert!(rows_of(&list).is_empty() && !list.get_visible());

    list.set_notifications(&[note("a", "First"), note("b", "Second")]);
    let rows = rows_of(&list);
    assert_eq!(rows.len(), 2);
    assert!(list.get_visible());
    assert_eq!(rows[0].summary().as_deref(), Some("First"));

    let first = rows[0].clone();
    list.set_notifications(&[note("b", "Second"), note("a", "First again")]);
    let reordered = rows_of(&list);
    assert_eq!(
        reordered.len(),
        2,
        "a reorder moves rows, it does not build more of them"
    );
    assert!(
        reordered[1] == first,
        "a notification that moves keeps its own widget: by_key matches on the key, so the \
             row carrying `a` follows it rather than staying where it was"
    );
    assert_eq!(reordered[1].summary().as_deref(), Some("First again"));
    assert_eq!(reordered[0].summary().as_deref(), Some("Second"));

    let fired = Rc::new(RefCell::new(Vec::<String>::new()));
    list.connect_activated({
        let fired = Rc::clone(&fired);
        move |_, key| fired.borrow_mut().push(format!("activated {key}"))
    });
    list.connect_dismissed({
        let fired = Rc::clone(&fired);
        move |_, key| fired.borrow_mut().push(format!("dismissed {key}"))
    });
    list.connect_action_invoked({
        let fired = Rc::clone(&fired);
        move |_, key, action| fired.borrow_mut().push(format!("{key}/{action}"))
    });

    reordered[1].emit_by_name::<()>("activated", &[]);
    child_named::<gtk4::Button>(&reordered[0], "notification__close").emit_clicked();
    assert_eq!(
        *fired.borrow(),
        vec!["activated a".to_owned(), "dismissed b".to_owned()],
        "the key a row reports is its own, whatever position it has ended up in"
    );

    let mut image_note = note("image", "Screenshot captured");
    image_note.image = Some(texture(64, 64));
    list.set_notifications(&[image_note]);
    let image_row = children_of::<NotificationCard>(&list)
        .into_iter()
        .next()
        .expect("an image notification builds a card");
    assert!(
        child_named::<NotificationImageBody>(&image_row, "notification__image-body").get_visible(),
        "an image notification reveals the shared card's thumbnail slot"
    );
    image_row.emit_by_name::<()>("activated", &[]);
    child_named::<gtk4::Button>(&image_row, "notification__close").emit_clicked();
    assert_eq!(
        &fired.borrow()[2..],
        ["activated image", "dismissed image"],
        "an image notification forwards the card events"
    );

    list.set_notifications(&[note("image", "Now text")]);
    assert!(
        children_of::<NotificationCard>(&list)[0] == image_row
            && !child_named::<NotificationImageBody>(&image_row, "notification__image-body")
                .get_visible(),
        "removing an image reuses the same card and collapses its thumbnail slot"
    );

    list.set_notifications(&[]);
    assert!(
        rows_of(&list).is_empty() && !list.get_visible(),
        "an emptied list unparents its rows and takes no space"
    );

    list.set_notifications(&[note("z", "Only")]);
    let only = rows_of(&list)[0].clone();
    only.set_summary(Some("Touched by hand"));
    list.set_notifications(&[note("z", "Only")]);
    assert_eq!(
        only.summary().as_deref(),
        Some("Touched by hand"),
        "an unchanged slice is not re-applied: the stored copy exists to answer that, and \
             writing it back over an untouched row is the work this avoids"
    );

    let stack = NotificationStack::new();
    let stack_rows = |stack: &NotificationStack| children_of::<NotificationCard>(stack);
    let strips = |stack: &NotificationStack| stack.imp().strips.borrow().len();
    let chip = stack
        .imp()
        .chip
        .get()
        .expect("the chip is built once")
        .clone();
    let chip_text = || {
        stack
            .imp()
            .chip_label
            .get()
            .expect("the chip is built once")
            .text()
            .to_string()
    };

    assert!(
        stack_rows(&stack).is_empty() && !stack.get_visible() && stack.is_collapsed(),
        "a stack lands on screen collapsed and takes no room until it is filled"
    );

    let three = [note("a", "First"), note("b", "Second"), note("c", "Third")];
    stack.set_items(&three);
    assert!(stack.get_visible());
    assert_eq!(stack_rows(&stack).len(), 3);
    stack.set_collapsed(true);
    assert!(!stack.is_collapsed());
    assert_eq!(strips(&stack), 0);
    assert_eq!(
        stack_rows(&stack)
            .iter()
            .map(|row| row.get_visible())
            .collect::<Vec<_>>(),
        vec![true, true, true],
        "three notifications remain individual cards"
    );
    assert!(
        stack_rows(&stack)
            .iter()
            .all(|row| { child_named::<gtk4::Button>(row, "notification__close").get_visible() }),
        "every card keeps a close button whether it is unread or history"
    );
    assert!(!chip.get_visible());

    let mut four = [
        note("a", "First"),
        note("b", "Second"),
        note("c", "Third"),
        note("d", "Fourth"),
    ];
    four[0].actions = vec![Action {
        key: "reply".to_owned(),
        label: "Reply".to_owned(),
    }];
    stack.set_items(&four);
    assert!(stack.is_collapsed());
    assert_eq!(strips(&stack), 2);
    assert_eq!(
        stack_rows(&stack)
            .iter()
            .map(|row| row.get_visible())
            .collect::<Vec<_>>(),
        vec![true, false, false, false],
        "four notifications collapse to a front card and decorative edges"
    );

    assert!(
        stack
            .first_child()
            .is_some_and(|child| child.has_css_class("notification-stack__strip")),
        "paint order is child order, so the strips must be parented ahead of the front card — \
             one parented after it covers the card it is meant to sit behind"
    );

    assert!(chip.get_visible());
    assert_eq!(chip_text(), "4 notifications");
    let front = &stack_rows(&stack)[0];
    assert!(
        !child_named::<gtk4::Button>(front, "notification__close").get_visible()
            && !child_named::<gtk4::Box>(front, "notification__actions").get_visible(),
        "the collapsed card is a preview, so it exposes no controls for one notification"
    );

    chip.emit_clicked();
    assert!(!stack.is_collapsed(), "the chip opens the stack");
    assert_eq!(
        strips(&stack),
        0,
        "a fanned stack has no edges left to show"
    );
    assert!(stack_rows(&stack).iter().all(|row| row.get_visible()));
    let front = &stack_rows(&stack)[0];
    assert!(
        child_named::<gtk4::Button>(front, "notification__close").get_visible()
            && child_named::<gtk4::Box>(front, "notification__actions").get_visible(),
        "expanding restores the front notification's controls"
    );
    assert_eq!(chip_text(), "Collapse");

    chip.emit_clicked();
    assert!(
        stack.is_collapsed() && strips(&stack) == 2,
        "the same chip closes it: a control that only opens leaves the reader no way back"
    );

    let width = 600;
    let _header_control = stack.header_control();
    let front_height = stack_rows(&stack)[0]
        .measure(gtk4::Orientation::Vertical, width)
        .1;
    let stack_height = stack.measure(gtk4::Orientation::Vertical, width).1;
    assert_eq!(
        stack_height - front_height,
        6,
        "two backplates add only a shallow peek below the front card"
    );

    stack.set_items(&[
        note("a", "First"),
        note("b", "Second"),
        note("c", "Third"),
        note("d", "Fourth"),
        note("e", "Fifth"),
    ]);
    assert_eq!(
        strips(&stack),
        crate::notification_stack::MAX_DEPTH,
        "the stack shows a fixed number of edges however many are behind the front card"
    );

    stack.set_items(&three);
    assert_eq!(stack.imp().depth(), 0);
    stack.set_items(&four);
    assert_eq!(
        stack.imp().depth(),
        crate::notification_stack::MAX_DEPTH,
        "the fourth notification is the point where decorative stack depth appears"
    );

    let front = stack_rows(&stack)[0].clone();
    stack.set_items(&[
        note("b", "Second"),
        note("a", "First again"),
        note("c", "Third"),
        note("d", "Fourth"),
    ]);
    let moved = stack_rows(&stack);
    assert!(
        moved[1] == front,
        "a notification that moves keeps its own widget, so the front card can become one of \
             the cards behind without being rebuilt"
    );

    let stack_fired = Rc::new(RefCell::new(Vec::<String>::new()));
    stack.connect_activated({
        let fired = Rc::clone(&stack_fired);
        move |_, key| fired.borrow_mut().push(format!("activated {key}"))
    });
    stack.connect_dismissed({
        let fired = Rc::clone(&stack_fired);
        move |_, key| fired.borrow_mut().push(format!("dismissed {key}"))
    });
    stack.connect_action_invoked({
        let fired = Rc::clone(&stack_fired);
        move |_, key, action| fired.borrow_mut().push(format!("{key}/{action}"))
    });
    let stack_cleared = Rc::new(Cell::new(0u32));
    stack.connect_clear_requested({
        let cleared = Rc::clone(&stack_cleared);
        move |_| cleared.set(cleared.get() + 1)
    });

    stack.set_collapsed(true);
    secondary_click(&moved[0]);
    assert_eq!(stack_cleared.get(), 1);
    assert!(stack_fired.borrow().is_empty());
    let collapsed_preview = moved[0].clone();
    assert!(
        collapsed_preview.is_focusable(),
        "a collapsed preview owns the card click even without a default action"
    );
    collapsed_preview.emit_by_name::<()>("activated", &[]);
    assert!(
        !stack.is_collapsed() && stack_fired.borrow().is_empty(),
        "the collapsed front card opens the preview without activating its notification"
    );
    assert!(
        !collapsed_preview.is_focusable(),
        "expanding restores the card's actual non-activatable state"
    );
    secondary_click(&moved[0]);
    moved[1].emit_by_name::<()>("activated", &[]);
    child_named::<gtk4::Button>(&moved[2], "notification__close").emit_clicked();
    assert_eq!(
        *stack_fired.borrow(),
        vec![
            "dismissed b".to_owned(),
            "activated a".to_owned(),
            "dismissed c".to_owned(),
        ],
        "right-click and close dismiss expanded cards while activation keeps its own key"
    );

    stack.set_items(&four);
    stack.set_collapsed(true);
    let touched = stack_rows(&stack)[0].clone();
    touched.set_summary(Some("Touched by hand"));
    stack.set_items(&four);
    stack.set_collapsed(true);
    assert_eq!(
        touched.summary().as_deref(),
        Some("Touched by hand"),
        "neither an unchanged slice nor an unchanged collapse state rebuilds the cards"
    );

    stack.set_items(&[note("z", "Only")]);
    assert!(
        !chip.get_visible() && strips(&stack) == 0,
        "one notification has nothing to collapse, so it gets no chip and no edges"
    );

    stack.set_items(&[]);
    assert!(
        stack_rows(&stack).is_empty() && !stack.get_visible(),
        "an emptied stack unparents its cards and takes no space"
    );

    let animated = NotificationStack::new();
    animated.set_items(&four);
    assert!(
        animated.imp().animation.get().is_none(),
        "a standalone stack pays for no animation it did not opt into"
    );
    animated.set_animated(true);
    animated.set_collapsed(false);
    assert!(
        animated.imp().animation.get().is_some()
            && !animated.is_collapsed()
            && stack_rows(&animated).iter().all(|row| row.get_visible()),
        "the explicit opt-in drives the real stack to its expanded state"
    );
    four
}
