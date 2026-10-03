use super::*;

pub(super) fn check() {
    let popover = WorkspacesPopover::new();
    let session = |title: &str| {
        vec![
            Workspace {
                id: 1,
                label: "chats".to_owned(),
                detail: "1 window".to_owned(),
                output: "DP-2".to_owned(),
                display: "Dell U2723QE".to_owned(),
                focused: false,
                urgent: false,
                windows: vec![WorkspaceWindow {
                    id: 9,
                    title: title.to_owned(),
                    app_id: "ghostty".to_owned(),
                    focused: false,
                    urgent: false,
                }],
            },
            Workspace {
                id: 2,
                label: "browser".to_owned(),
                detail: "1 window".to_owned(),
                output: "DP-2".to_owned(),
                display: "Dell U2723QE".to_owned(),
                focused: true,
                urgent: false,
                windows: vec![WorkspaceWindow {
                    id: 11,
                    title: "a browser".to_owned(),
                    app_id: "chrome".to_owned(),
                    focused: true,
                    urgent: false,
                }],
            },
            Workspace {
                id: 5,
                label: "1".to_owned(),
                detail: "Empty".to_owned(),
                output: "eDP-1".to_owned(),
                display: String::new(),
                focused: false,
                urgent: false,
                windows: Vec::new(),
            },
        ]
    };
    popover.set_workspaces(&session("a terminal"));

    let sections =
        children_of::<WorkspaceSection>(&child_named::<WorkspaceList>(&popover, "workspace-list"));
    assert_eq!(
        sections.len(),
        2,
        "workspaces are grouped by the display they are on, which is half of what the list \
             is for"
    );

    let chosen: Rc<RefCell<Vec<u64>>> = Rc::new(RefCell::new(Vec::new()));
    popover.connect_activated({
        let chosen = Rc::clone(&chosen);
        move |id| chosen.borrow_mut().push(id)
    });
    let column = child_named::<gtk4::Box>(&sections[0], "section__content")
        .first_child()
        .and_downcast::<gtk4::Box>()
        .expect("a section holds one column of rows");
    let holders = children_of::<Expandable>(&column);
    assert_eq!(holders.len(), 2, "DP-2 carries two of the three workspaces");
    assert_eq!(
        sections[0].imp().section.title().as_deref(),
        Some("Dell U2723QE"),
        "a section names its display, not its connector"
    );
    assert_eq!(
        sections[1].imp().section.title().as_deref(),
        Some("eDP-1"),
        "a display with no name falls back to its connector rather than an empty header"
    );
    assert_eq!(popover.imp().hero.subtitle(), None);
    assert_eq!(
        holders[0]
            .head::<SplitRow>()
            .and_then(|split| split.row().value())
            .as_deref(),
        Some("1 window"),
        "the window count sits in the row's value slot"
    );
    let empty = children_of::<Expandable>(
        &child_named::<gtk4::Box>(&sections[1], "section__content")
            .first_child()
            .and_downcast::<gtk4::Box>()
            .expect("a section holds one column of rows"),
    )[0]
    .clone();
    empty
        .head::<SplitRow>()
        .expect("an empty workspace keeps its chevron")
        .emit_by_name::<()>("details", &[]);
    let placeholder = children_of::<LegacyRow>(
        &empty
            .details::<gtk4::Box>()
            .expect("an empty workspace's card is filled"),
    );
    assert!(
        placeholder.len() == 1
            && placeholder[0].get_visible()
            && placeholder[0].title().as_deref() == Some("No windows"),
        "an empty workspace's card says so rather than opening on nothing"
    );
    empty.set_expanded(false);

    let head = |holder: &Expandable| {
        holder
            .head::<SplitRow>()
            .expect("a holder leads with its row")
    };
    let panel = |holder: &Expandable| {
        holder
            .last_child()
            .and_downcast::<gtk4::Revealer>()
            .expect("a holder ends with its own detail revealer")
    };

    head(&holders[1]).emit_by_name::<()>("activated", &[]);
    assert_eq!(
        *chosen.borrow(),
        [2],
        "a row reports the workspace it stands for, which is the id the focus command needs"
    );

    assert!(
        !panel(&holders[0]).reveals_child(),
        "a popover opens showing the list, not one workspace's detail"
    );
    assert_eq!(
        panel(&holders[0]).transition_type(),
        gtk4::RevealerTransitionType::SlideDown,
        "a detail unfolds under its own row, never beside the list"
    );

    head(&holders[0]).emit_by_name::<()>("details", &[]);
    assert!(
        panel(&holders[0]).reveals_child(),
        "the chevron is the only way into a workspace's windows, so it has to open its own \
             row's panel"
    );
    assert!(
        holders[0].has_css_class("open")
            && holders[0].has_css_class("card")
            && !head(&holders[0]).has_css_class("receded"),
        "the open row keeps full weight, and it and its windows become one card"
    );
    assert!(
        holders[1].has_css_class("receded") && !holders[1].has_css_class("open"),
        "and every other row recedes"
    );
    assert!(
        popover
            .imp()
            .hero
            .parent()
            .is_some_and(|hero| hero.has_css_class("receded")),
        "the shell dims everything outside the open card, the hero included, with no list \
             of what to dim"
    );
    let other = child_named::<gtk4::Box>(&sections[1], "section__header");
    assert!(
        !other.has_css_class("receded")
            && child_named::<gtk4::Box>(&sections[1], "section__content").has_css_class("receded"),
        "another display's rows dim, its header stays lit: headers are the frame"
    );

    let rows_box = |holder: &Expandable| {
        holder
            .details::<gtk4::Box>()
            .expect("the panel holds a rows box once it has been filled")
    };
    let windows: Vec<LegacyRow> = children_of::<LegacyRow>(&rows_box(&holders[0]))
        .into_iter()
        .filter(|row| row.get_visible())
        .collect();
    assert_eq!(
        windows.len(),
        1,
        "the panel lists the windows of the workspace whose chevron was pressed"
    );
    assert_eq!(windows[0].title().as_deref(), Some("a terminal"));
    assert!(
        !popover.imp().shell.dismiss_from(windows[0].upcast_ref()),
        "a press inside the open card belongs to the card"
    );
    assert!(
        popover
            .imp()
            .shell
            .dismiss_from(head(&holders[1]).upcast_ref())
            && !holders[0].expanded()
            && !holders[1].expanded(),
        "a press on anything dimmed closes the card and is claimed, so the row never acts"
    );
    head(&holders[0]).emit_by_name::<()>("details", &[]);
    assert!(
        !popover
            .imp()
            .shell
            .dismiss_from(head(&holders[1]).detail().upcast_ref())
            && holders[0].expanded(),
        "a press on another card's chevron goes through, so it switches cards in one click"
    );

    let focused: Rc<RefCell<Vec<u64>>> = Rc::new(RefCell::new(Vec::new()));
    popover.connect_window_activated({
        let focused = Rc::clone(&focused);
        move |id| focused.borrow_mut().push(id)
    });
    windows[0].emit_by_name::<()>("clicked", &[]);
    assert_eq!(
        *focused.borrow(),
        [9],
        "a window in the panel is the only place a single window can be focused from, now \
             that the strip itself opens the popover"
    );

    let first_row = || {
        let listed = children_of::<WorkspaceSection>(&child_named::<WorkspaceList>(
            &popover,
            "workspace-list",
        ));
        let column = child_named::<gtk4::Box>(&listed[0], "section__content")
            .first_child()
            .and_downcast::<gtk4::Box>()
            .expect("a section holds one column of rows");
        children_of::<Expandable>(&column)[0].clone()
    };

    let standing = first_row();
    popover.set_workspaces(&session("a browser"));
    assert!(
        panel(&holders[0]).reveals_child(),
        "an event arriving must not close a panel the user opened"
    );
    assert_eq!(
        children_of::<LegacyRow>(&rows_box(&holders[0]))[0]
            .title()
            .as_deref(),
        Some("a browser"),
        "an open panel follows the session: a window renaming itself shows through"
    );
    assert_eq!(
        standing,
        first_row(),
        "a window retitling itself must not rebuild the workspace list under the pointer, and \
             a title changes on every keystroke"
    );
    assert_eq!(
        children_of::<LegacyRow>(&rows_box(&holders[0]))[0],
        windows[0],
        "and it keeps its own row too: the panel reconciles by window id, so a retitle \
             rewrites a label rather than replacing the row under the pointer"
    );

    head(&holders[1]).emit_by_name::<()>("details", &[]);
    assert!(
        !panel(&holders[0]).reveals_child(),
        "opening another workspace's detail closes the one that was open"
    );
    let switched = children_of::<LegacyRow>(&rows_box(&holders[1]));
    assert_ne!(
        switched[0], windows[0],
        "another workspace's window is another row, not the first one refilled"
    );
    switched[0].emit_by_name::<()>("clicked", &[]);
    assert_eq!(
        *focused.borrow(),
        [9, 11],
        "a row is only ever reused for the window it was built for, so it can report that \
             window without consulting anything"
    );

    head(&holders[1]).emit_by_name::<()>("details", &[]);
    assert!(
        !panel(&holders[1]).reveals_child(),
        "the same chevron closes what it opened"
    );
}
