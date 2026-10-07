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
    let holders = children_of::<ExpanderRow>(&column);
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
        holders[0].value().as_deref(),
        Some("1 window"),
        "the window count sits in the row's value slot"
    );
    let head = |holder: &ExpanderRow| holder.imp().head.get();
    let marked = |row: &Row| row.imp().end().first_child().is_some();
    assert!(
        marked(head(&holders[1]).upcast_ref()) && !marked(head(&holders[0]).upcast_ref()),
        "the focused workspace carries the check"
    );

    let mut urgent = session("a terminal");
    urgent[0].urgent = true;
    popover.set_workspaces(&urgent);
    assert_eq!(
        holders[0].tone(),
        RowTone::Warning,
        "an urgent workspace takes the warning tone"
    );
    popover.set_workspaces(&session("a terminal"));
    assert_eq!(holders[0].tone(), RowTone::Normal);

    let empty = children_of::<ExpanderRow>(
        &child_named::<gtk4::Box>(&sections[1], "section__content")
            .first_child()
            .and_downcast::<gtk4::Box>()
            .expect("a section holds one column of rows"),
    )[0]
    .clone();
    head(&empty).activate();
    let window_rows = |holder: &ExpanderRow| {
        holder
            .details()
            .last_child()
            .and_downcast::<gtk4::Box>()
            .expect("an opened workspace holds its window rows")
    };
    let placeholder: Vec<Row> = children_of::<Row>(&window_rows(&empty))
        .into_iter()
        .filter(|row| row.get_visible())
        .collect();
    assert!(
        placeholder.len() == 1
            && !placeholder[0].is::<ActionRow>()
            && placeholder[0].title().as_deref() == Some("No windows"),
        "an empty workspace's card says so, in a row that does nothing"
    );
    empty.set_open(false);

    let focus_row = |holder: &ExpanderRow| {
        holder
            .details()
            .first_child()
            .and_downcast::<ActionRow>()
            .expect("a workspace's drawer leads with its focus action")
    };
    focus_row(&holders[1]).emit_by_name::<()>("activated", &[]);
    assert_eq!(
        *chosen.borrow(),
        [2],
        "the focus action reports the workspace it stands for, which is the id the focus \
             command needs"
    );

    assert!(
        !holders[0].open(),
        "a popover opens showing the list, not one workspace's detail"
    );
    head(&holders[0]).activate();
    assert!(holders[0].open(), "the row opens its own drawer");
    assert!(
        popover
            .imp()
            .hero
            .parent()
            .is_some_and(|hero| hero.has_css_class("receded")),
        "the shell dims everything outside the open card, the hero included"
    );

    let windows: Vec<ActionRow> = children_of::<ActionRow>(&window_rows(&holders[0]));
    assert_eq!(
        windows.len(),
        1,
        "the drawer lists the windows of the workspace that was opened"
    );
    assert_eq!(
        windows[0].upcast_ref::<Row>().title().as_deref(),
        Some("a terminal")
    );
    assert!(
        !popover.imp().shell.dismiss_from(windows[0].upcast_ref()),
        "a press inside the open card belongs to the card"
    );

    let focused: Rc<RefCell<Vec<u64>>> = Rc::new(RefCell::new(Vec::new()));
    popover.connect_window_activated({
        let focused = Rc::clone(&focused);
        move |id| focused.borrow_mut().push(id)
    });
    windows[0].emit_by_name::<()>("activated", &[]);
    assert_eq!(*focused.borrow(), [9]);

    let first_row = || {
        let listed = children_of::<WorkspaceSection>(&child_named::<WorkspaceList>(
            &popover,
            "workspace-list",
        ));
        let column = child_named::<gtk4::Box>(&listed[0], "section__content")
            .first_child()
            .and_downcast::<gtk4::Box>()
            .expect("a section holds one column of rows");
        children_of::<ExpanderRow>(&column)[0].clone()
    };

    let standing = first_row();
    popover.set_workspaces(&session("a browser"));
    assert!(
        holders[0].open(),
        "an event arriving must not close a drawer the user opened"
    );
    let retitled = children_of::<ActionRow>(&window_rows(&holders[0]));
    assert_eq!(
        retitled[0].upcast_ref::<Row>().title().as_deref(),
        Some("a browser"),
        "an open drawer follows the session: a window renaming itself shows through"
    );
    assert_eq!(standing, first_row(), "a retitle does not rebuild the list");
    assert_eq!(
        retitled[0], windows[0],
        "and it keeps its own row: the drawer reconciles by window id"
    );

    head(&holders[1]).activate();
    assert!(
        !holders[0].open() && holders[1].open(),
        "opening another workspace closes the one that was open"
    );
    let switched = children_of::<ActionRow>(&window_rows(&holders[1]));
    assert!(
        marked(switched[0].upcast_ref()),
        "the focused window carries the check"
    );
    switched[0].emit_by_name::<()>("activated", &[]);
    assert_eq!(*focused.borrow(), [9, 11]);
}
