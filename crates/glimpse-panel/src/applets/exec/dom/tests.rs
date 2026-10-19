use super::*;
use glimpse_services::{Op, WireNode};
use serde_json::json;

fn init() {
    gtk4::init().expect("GTK display");
    glimpse_widgets::register_resources().expect("widget resources");
}

fn node(id: u32, kind: &str, props: Value, children: Vec<WireNode>) -> WireNode {
    WireNode {
        id,
        kind: kind.to_owned(),
        props: serde_json::from_value(props).expect("props"),
        children,
    }
}

fn tree(children: Vec<WireNode>) -> Tree {
    Tree::default()
        .apply(vec![Op::Insert {
            parent: 0,
            node: node(1, "popover", json!({}), children),
            before: None,
        }])
        .expect("tree")
}

fn setup(tree: &Tree) -> (PopoverShell, Dom, Rc<RefCell<Vec<UserEvent>>>) {
    let shell = PopoverShell::new();
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = events.clone();
    let (sender, _receiver) = relm4::channel();
    let opener = crate::applet::Ctx::new("test".into(), None, sender).opener();
    let mut dom = Dom::new(
        &shell,
        7,
        Rc::new(move |event| seen.borrow_mut().push(event)),
        opener,
        Rc::new(RefCell::new(HashMap::new())),
    );
    assert!(dom.reconcile(tree, 1));
    (shell, dom, events)
}

fn widget(dom: &Dom, generation: u64, id: u32, kind: ElementKind) -> gtk4::Widget {
    dom.nodes
        .get(&(generation, id, kind))
        .expect("mounted widget")
        .widget
        .clone()
}

#[test]
#[ignore = "needs a display"]
fn containers_sit_directly_in_the_body() {
    init();
    let built = tree(vec![
        node(
            2,
            "container",
            json!({"height": 48, "background": "red"}),
            vec![node(3, "label", json!({"text": "body 1"}), vec![])],
        ),
        node(
            4,
            "addon",
            json!({}),
            vec![node(
                5,
                "container",
                json!({"width": 64, "height": 32, "background": "purple"}),
                vec![node(6, "label", json!({"text": "addon"}), vec![])],
            )],
        ),
    ]);
    let (shell, dom, _) = setup(&built);
    let body_container = widget(&dom, 1, 2, ElementKind::Container);
    assert_eq!(
        body_container.parent().as_ref(),
        Some(shell.body().upcast_ref())
    );
    let header = first_with_class(shell.upcast_ref(), "popover-shell__header").expect("header");
    let slot = first_with_class(&header, "hero__slot").expect("slot");
    let addon = slot.first_child().expect("addon");
    assert_eq!(addon, widget(&dom, 1, 5, ElementKind::Container));
}

fn first_with_class(widget: &gtk4::Widget, class: &str) -> Option<gtk4::Widget> {
    if widget.has_css_class(class) {
        return Some(widget.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = first_with_class(&current, class) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

fn descendant_label(widget: &gtk4::Widget, text: &str) -> Option<gtk4::Label> {
    if let Some(label) = widget.clone().downcast::<gtk4::Label>().ok()
        && label.text() == text
    {
        return Some(label);
    }
    let mut child = widget.first_child();
    while let Some(next) = child {
        if let Some(label) = descendant_label(&next, text) {
            return Some(label);
        }
        child = next.next_sibling();
    }
    None
}

#[test]
fn signal_dispatch_keeps_name_args_and_generation() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = events.clone();
    let dispatch: Dispatch = Rc::new(move |event| seen.borrow_mut().push(event));
    emit(
        &dispatch,
        7,
        (3, 42, ElementKind::Button),
        "onClick",
        vec![json!("go")],
        Some(2),
    );
    let events = events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].slot, 7);
    assert_eq!(events[0].generation, 3);
    assert_eq!(events[0].id, 42);
    assert_eq!(events[0].name, "onClick");
    assert_eq!(events[0].args, vec![json!("go")]);
    assert_eq!(events[0].seq, Some(2));
}

#[test]
#[ignore = "needs a display"]
fn empty_popover_later_shows_row() {
    init();
    let first = tree(vec![]);
    let (_shell, mut dom, _) = setup(&first);
    assert!(!dom.body.get_visible());
    let next = first
        .apply(vec![Op::Insert {
            parent: 1,
            node: node(2, "row", json!({"title":"Ready"}), vec![]),
            before: None,
        }])
        .expect("row");
    assert!(dom.reconcile(&next, 1));
    assert!(dom.body.get_visible());
    assert!(widget(&dom, 1, 2, ElementKind::Row).parent().is_some());
}

#[test]
#[ignore = "needs a display"]
fn reorder_keeps_widget_identity() {
    init();
    let first = tree(vec![
        node(2, "row", json!({"title":"A"}), vec![]),
        node(3, "row", json!({"title":"B"}), vec![]),
    ]);
    let (_shell, mut dom, _) = setup(&first);
    let a = widget(&dom, 1, 2, ElementKind::Row);
    let b = widget(&dom, 1, 3, ElementKind::Row);
    let next = first
        .apply(vec![Op::Move {
            parent: 1,
            id: 3,
            before: Some(2),
        }])
        .expect("move");
    assert!(dom.reconcile(&next, 1));
    assert_eq!(widget(&dom, 1, 2, ElementKind::Row), a);
    assert_eq!(widget(&dom, 1, 3, ElementKind::Row), b);
    assert_eq!(dom.body.first_child(), Some(b));
}

#[test]
#[ignore = "needs a display"]
fn set_dresses_only_touched_widget() {
    init();
    let first = tree(vec![
        node(2, "row", json!({"title":"A"}), vec![]),
        node(3, "row", json!({"title":"B"}), vec![]),
    ]);
    let (_shell, mut dom, _) = setup(&first);
    let a = widget(&dom, 1, 2, ElementKind::Row)
        .downcast::<UiRow>()
        .expect("row");
    let b = widget(&dom, 1, 3, ElementKind::Row)
        .downcast::<UiRow>()
        .expect("row");
    let notifications = Rc::new(RefCell::new(0));
    let count = notifications.clone();
    b.connect_notify_local(Some("title"), move |_, _| *count.borrow_mut() += 1);
    let next = first
        .apply(vec![Op::Set {
            id: 2,
            props: serde_json::from_value(json!({"title":"C"})).expect("props"),
            seq: None,
        }])
        .expect("set");
    assert!(dom.reconcile(&next, 1));
    assert_eq!(
        widget(&dom, 1, 2, ElementKind::Row),
        a.clone().upcast::<gtk4::Widget>()
    );
    assert_eq!(
        widget(&dom, 1, 3, ElementKind::Row),
        b.clone().upcast::<gtk4::Widget>()
    );
    assert_eq!(*notifications.borrow(), 0);
    assert_eq!(a.property::<Option<String>>("title").as_deref(), Some("C"));
}

#[test]
#[ignore = "needs a display"]
fn text_button_keeps_its_label_with_icon() {
    init();
    let first = tree(vec![node(
        2,
        "button",
        json!({"text":"Run","icon":"media-playback-start-symbolic"}),
        vec![],
    )]);
    let (_shell, dom, _) = setup(&first);
    let button = widget(&dom, 1, 2, ElementKind::Button)
        .downcast::<gtk4::Button>()
        .expect("button");
    let content = button.child().and_downcast::<gtk4::Box>().expect("content");
    let label = content
        .last_child()
        .and_downcast::<gtk4::Label>()
        .expect("label");
    assert_eq!(label.text(), "Run");
    assert_eq!(label.ellipsize(), gtk4::pango::EllipsizeMode::End);
    assert_eq!(label.max_width_chars(), 48);
    assert!(gtk4::test_accessible_has_property(
        &button,
        gtk4::AccessibleProperty::Label
    ));
}

#[test]
#[ignore = "needs a display"]
fn row_handler_is_connected_once_and_survives_a_set() {
    init();
    let first = tree(vec![node(2, "actionrow", json!({"title":"Open"}), vec![])]);
    let (_shell, mut dom, events) = setup(&first);
    let host = widget(&dom, 1, 2, ElementKind::ActionRow);
    let row = host.clone().downcast::<ActionRow>().expect("row");
    row.emit_by_name::<()>("activated", &[]);
    assert_eq!(events.borrow().len(), 1);
    let next = first
        .apply(vec![Op::Set {
            id: 2,
            props: serde_json::from_value(json!({"onActivate":true})).expect("props"),
            seq: None,
        }])
        .expect("set");
    assert!(dom.reconcile(&next, 1));
    assert_eq!(widget(&dom, 1, 2, ElementKind::ActionRow), host);
    row.emit_by_name::<()>("activated", &[]);
    assert_eq!(events.borrow().len(), 2);
    assert_eq!(events.borrow()[1].name, "onActivate");
}

#[test]
#[ignore = "needs a display"]
fn moving_row_to_earlier_section_keeps_it_visible() {
    init();
    let first = tree(vec![
        node(2, "section", json!({"title":"A"}), vec![]),
        node(
            3,
            "section",
            json!({"title":"B"}),
            vec![node(4, "row", json!({"title":"Moved"}), vec![])],
        ),
    ]);
    let (_shell, mut dom, _) = setup(&first);
    let row = widget(&dom, 1, 4, ElementKind::Row);
    let next = first
        .apply(vec![Op::Move {
            parent: 2,
            id: 4,
            before: None,
        }])
        .expect("move");
    assert!(dom.reconcile(&next, 1));
    assert_eq!(widget(&dom, 1, 4, ElementKind::Row), row);
    assert_eq!(
        row.parent(),
        dom.nodes
            .get(&(1, 2, ElementKind::Section))
            .and_then(|m| m.inner.as_ref())
            .map(|inner| inner.clone().upcast())
    );
}

#[test]
#[ignore = "needs a display"]
fn seq_survives_dom_reopen() {
    init();
    let first = tree(vec![node(2, "switch", json!({"onToggle":true}), vec![])]);
    let (shell, dom, events) = setup(&first);
    let seq = dom.seq.clone();
    widget(&dom, 1, 2, ElementKind::Switch)
        .downcast::<gtk4::Switch>()
        .expect("switch")
        .set_active(true);
    let prior = events.borrow()[0].seq.expect("seq");
    drop(dom);
    let seen = events.clone();
    let (sender, _receiver) = relm4::channel();
    let opener = crate::applet::Ctx::new("test".into(), None, sender).opener();
    let mut reopened = Dom::new(
        &shell,
        7,
        Rc::new(move |event| seen.borrow_mut().push(event)),
        opener,
        seq,
    );
    assert!(reopened.reconcile(&first, 1));
    widget(&reopened, 1, 2, ElementKind::Switch)
        .downcast::<gtk4::Switch>()
        .expect("switch")
        .set_active(true);
    assert!(
        events
            .borrow()
            .last()
            .and_then(|event| event.seq)
            .expect("seq")
            > prior
    );
}

#[test]
#[ignore = "needs a display"]
fn hostile_text_and_icon_only_button() {
    init();
    let first = tree(vec![
        node(
            2,
            "label",
            json!({"text":"Long","ellipsize":"none"}),
            vec![],
        ),
        node(
            4,
            "button",
            json!({"icon":"media-playback-start-symbolic","tooltip":"Play"}),
            vec![],
        ),
        node(
            5,
            "image",
            json!({"icon":"dialog-warning-symbolic","tooltip":"Warning"}),
            vec![],
        ),
    ]);
    let (_shell, dom, _) = setup(&first);
    let label = widget(&dom, 1, 2, ElementKind::Label)
        .downcast::<gtk4::Label>()
        .expect("label");
    assert_eq!(label.ellipsize(), gtk4::pango::EllipsizeMode::None);
    let button = widget(&dom, 1, 4, ElementKind::Button)
        .downcast::<gtk4::Button>()
        .expect("button");
    assert_eq!(button.tooltip_text().as_deref(), Some("Play"));
    assert!(gtk4::test_accessible_has_property(
        &button,
        gtk4::AccessibleProperty::Label
    ));
    let image = widget(&dom, 1, 5, ElementKind::Image)
        .downcast::<gtk4::Image>()
        .expect("image");
    assert_eq!(image.tooltip_text().as_deref(), Some("Warning"));
    assert!(gtk4::test_accessible_has_property(
        &image,
        gtk4::AccessibleProperty::Label
    ));
}

#[test]
#[ignore = "needs a display"]
fn unsupported_row_matches_warning_design() {
    init();
    let first = tree(vec![node(2, "canvas", json!({}), vec![])]);
    let (_shell, dom, _) = setup(&first);
    let row = widget(&dom, 1, 2, ElementKind::Unsupported)
        .downcast::<UiRow>()
        .expect("row");
    assert!(row.property::<bool>("inactive"));
    assert_eq!(
        row.property::<Option<String>>("title"),
        Some(gettext("Unsupported: {name}").replace("{name}", "canvas"))
    );
    assert_eq!(
        row.property::<Option<String>>("subtitle"),
        Some(gettext(
            "This applet asked for an element glimpse does not render"
        ))
    );
    assert_eq!(
        row.property::<Option<String>>("icon-name").as_deref(),
        Some("dialog-warning-symbolic")
    );
}

#[test]
#[ignore = "needs a display"]
fn reload_replaces_widgets() {
    init();
    let first = tree(vec![node(2, "row", json!({"title":"A"}), vec![])]);
    let (_shell, mut dom, _) = setup(&first);
    let old = widget(&dom, 1, 2, ElementKind::Row);
    assert!(dom.reconcile(&first, 2));
    assert_ne!(widget(&dom, 2, 2, ElementKind::Row), old);
    assert!(old.parent().is_none());
}

#[test]
#[ignore = "needs a display"]
fn dressing_controls_emits_no_events() {
    init();
    let first = tree(vec![
        node(2, "switch", json!({"active":false,"onToggle":true}), vec![]),
        node(
            3,
            "scale",
            json!({"min":0,"max":100,"step":1,"value":4,"onChange":true}),
            vec![],
        ),
        node(
            4,
            "entry",
            json!({"value":"A","onChange":true,"onSubmit":true}),
            vec![],
        ),
        node(
            5,
            "actionrow",
            json!({"title":"Open","onActivate":true}),
            vec![],
        ),
        node(6, "button", json!({"text":"Run","onClick":true}), vec![]),
        node(
            8,
            "togglerow",
            json!({"title":"Mode","onToggle":true}),
            vec![],
        ),
        node(9, "scale", json!({"min":0,"max":1,"value":0.5}), vec![]),
    ]);
    let (_shell, mut dom, events) = setup(&first);
    let next = first
        .apply(vec![
            Op::Set {
                id: 2,
                props: serde_json::from_value(json!({"active":true})).expect("props"),
                seq: None,
            },
            Op::Set {
                id: 3,
                props: serde_json::from_value(json!({"value":50})).expect("props"),
                seq: None,
            },
            Op::Set {
                id: 4,
                props: serde_json::from_value(json!({"value":"B"})).expect("props"),
                seq: None,
            },
        ])
        .expect("set");
    assert!(dom.reconcile(&next, 1));
    assert!(events.borrow().is_empty());
    widget(&dom, 1, 2, ElementKind::Switch)
        .downcast::<gtk4::Switch>()
        .expect("switch")
        .set_active(false);
    assert_eq!(events.borrow().len(), 1);
    assert_eq!(events.borrow()[0].name, "onToggle");
    widget(&dom, 1, 3, ElementKind::Scale)
        .downcast::<gtk4::Scale>()
        .expect("scale")
        .set_value(60.0);
    let entry = widget(&dom, 1, 4, ElementKind::Entry)
        .downcast::<gtk4::Entry>()
        .expect("entry");
    entry.emit_by_name::<()>("changed", &[]);
    entry.emit_by_name::<()>("activate", &[]);
    widget(&dom, 1, 5, ElementKind::ActionRow).emit_by_name::<()>("activated", &[]);
    widget(&dom, 1, 6, ElementKind::Button).emit_by_name::<()>("clicked", &[]);
    widget(&dom, 1, 8, ElementKind::ToggleRow).emit_by_name::<()>("toggled", &[&true]);
    let events = events.borrow();
    assert_eq!(
        events
            .iter()
            .map(|event| event.name.as_str())
            .collect::<Vec<_>>(),
        [
            "onToggle",
            "onChange",
            "onChange",
            "onSubmit",
            "onActivate",
            "onClick",
            "onToggle"
        ]
    );
    assert_eq!(events[1].args, vec![json!(60.0)]);
    assert_eq!(events[2].args, vec![json!("B")]);
    assert_eq!(events[3].args, vec![json!("B")]);
}

#[test]
#[ignore = "needs a display"]
fn older_seq_keeps_entry_text_and_caret() {
    init();
    let first = tree(vec![node(
        2,
        "entry",
        json!({"value":"A","onChange":true}),
        vec![],
    )]);
    let (_shell, mut dom, events) = setup(&first);
    let entry = widget(&dom, 1, 2, ElementKind::Entry)
        .downcast::<gtk4::Entry>()
        .expect("entry");
    entry.set_text("Ab");
    entry.set_position(1);
    assert_eq!(events.borrow()[0].seq, Some(1));
    let next = first
        .apply(vec![Op::Set {
            id: 2,
            props: serde_json::from_value(json!({"value":"A"})).expect("props"),
            seq: Some(0),
        }])
        .expect("set");
    assert!(dom.reconcile(&next, 1));
    assert_eq!(entry.text(), "Ab");
    assert_eq!(entry.position(), 1);
}

fn set(id: u32, props: Value) -> Op {
    Op::Set {
        id,
        props: serde_json::from_value(props).expect("props"),
        seq: None,
    }
}

#[test]
#[ignore = "needs a display"]
fn widget_elements_render_update_without_echo_and_emit_user_events() {
    init();
    let label = |id: u32| node(id, "label", json!({"text":"t"}), vec![]);
    let first = tree(vec![
        node(
            2,
            "factlist",
            json!({"facts":[{"label":"A","value":"1"}]}),
            vec![],
        ),
        node(
            3,
            "choicelist",
            json!({"choices":[{"label":"x"},{"label":"y"}],"selected":0,"onSelect":true}),
            vec![],
        ),
        node(4, "readout", json!({"value":"5","unit":"C"}), vec![]),
        node(
            5,
            "notice",
            json!({"title":"T","severity":"warning","onActivate":true}),
            vec![],
        ),
        node(
            6,
            "calendar",
            json!({"year":2026,"month":6,"selected":"2026-06-04","today":"2026-06-01",
                "events":[{"date":"2026-06-05","colors":["error"]}],"onMonth":true,"onSelect":true}),
            vec![],
        ),
        node(
            7,
            "dots",
            json!({"colors":["accent","error"],"size":6}),
            vec![],
        ),
        node(
            8,
            "dropdown",
            json!({"options":["a","b","c"],"selected":1,"onSelect":true}),
            vec![],
        ),
        node(
            9,
            "togglebutton",
            json!({"label":"T","active":true,"onToggle":true}),
            vec![],
        ),
        node(
            10,
            "checkbutton",
            json!({"label":"C","onToggle":true}),
            vec![],
        ),
        node(
            11,
            "revealer",
            json!({"revealed":true,"transition":"none"}),
            vec![label(12)],
        ),
        node(
            13,
            "stack",
            json!({"visible":1}),
            vec![label(14), label(15)],
        ),
        node(
            18,
            "rangebar",
            json!({"min":0,"max":10,"low":2,"high":4,"now":3}),
            vec![],
        ),
        node(
            20,
            "bars",
            json!({"values":[1,null,3],"maximum":5,"highlightLast":false,"label":"Cost","tooltips":["a",null,"c"]}),
            vec![],
        ),
        node(
            21,
            "sparkline",
            json!({"values":[1,2,null,4],"fill":true,"showLast":true,"label":"Trend"}),
            vec![],
        ),
    ]);
    let (_shell, mut dom, events) = setup(&first);
    assert!(events.borrow().is_empty());
    let get = |dom: &Dom, id: u32, kind: ElementKind| widget(dom, 1, id, kind);

    let list = get(&dom, 3, ElementKind::ChoiceList)
        .downcast::<ChoiceList>()
        .expect("list");
    assert_eq!(list.selected(), Some(0));
    let readout = get(&dom, 4, ElementKind::Readout);
    assert_eq!(
        readout.property::<Option<String>>("value").as_deref(),
        Some("5")
    );
    let notice = get(&dom, 5, ElementKind::Notice);
    assert_eq!(
        notice.property::<glimpse_widgets::Severity>("severity"),
        glimpse_widgets::Severity::Warning
    );
    assert!(notice.property::<bool>("activatable"));
    let calendar = get(&dom, 6, ElementKind::Calendar)
        .downcast::<Calendar>()
        .expect("calendar");
    assert_eq!(calendar.shown(), (2026, 6));
    assert_eq!(calendar.selected(), Some(Ymd::new(2026, 6, 4)));
    assert_eq!(calendar.events(Ymd::new(2026, 6, 5)).len(), 1);
    let drop_down = get(&dom, 8, ElementKind::DropDown)
        .downcast::<gtk4::DropDown>()
        .expect("dropdown");
    assert_eq!(drop_down.selected(), 1);
    let toggle = get(&dom, 9, ElementKind::ToggleButton)
        .downcast::<gtk4::ToggleButton>()
        .expect("toggle");
    assert!(toggle.is_active());
    let check = get(&dom, 10, ElementKind::CheckButton)
        .downcast::<gtk4::CheckButton>()
        .expect("check");
    assert!(!check.is_active());
    assert_eq!(
        check
            .child()
            .and_downcast::<gtk4::Label>()
            .expect("label")
            .text(),
        "C"
    );
    let revealer = get(&dom, 11, ElementKind::Revealer)
        .downcast::<gtk4::Revealer>()
        .expect("revealer");
    assert!(revealer.reveals_child());
    assert!(get(&dom, 12, ElementKind::Label).parent().is_some());
    let stack = get(&dom, 13, ElementKind::Stack)
        .downcast::<gtk4::Stack>()
        .expect("stack");
    assert_eq!(stack.visible_child_name().as_deref(), Some("1"));
    let bars = get(&dom, 20, ElementKind::Bars)
        .downcast::<Bars>()
        .expect("bars");
    assert_eq!(bars.values(), vec![Some(1.0), None, Some(3.0)]);
    assert_eq!(bars.maximum(), Some(5.0));
    assert!(!bars.highlight_last());
    assert_eq!(bars.label(), "Cost");
    let line = get(&dom, 21, ElementKind::Sparkline)
        .downcast::<Sparkline>()
        .expect("sparkline");
    assert_eq!(line.values(), vec![Some(1.0), Some(2.0), None, Some(4.0)]);
    assert!(line.fill() && line.show_last());
    assert_eq!(line.label(), "Trend");

    let next = first
        .apply(vec![
            set(3, json!({"selected":1})),
            set(6, json!({"month":7,"selected":"2026-07-09"})),
            set(8, json!({"options":["x","y"],"selected":0})),
            set(9, json!({"active":false,"label":"U"})),
            set(10, json!({"active":true})),
            set(11, json!({"revealed":false})),
            set(13, json!({"visible":0})),
        ])
        .expect("set");
    assert!(dom.reconcile(&next, 1));
    assert!(
        events.borrow().is_empty(),
        "{:?}",
        events.borrow().iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert_eq!(list.selected(), Some(1));
    assert_eq!(calendar.shown(), (2026, 7));
    assert_eq!(drop_down.selected(), 0);
    assert!(!toggle.is_active() && check.is_active());
    assert!(!revealer.reveals_child());
    assert_eq!(stack.visible_child_name().as_deref(), Some("0"));

    list.emit_by_name::<()>("activated", &[&0u32]);
    notice.emit_by_name::<()>("clicked", &[]);
    calendar.emit_by_name::<()>("month-shown", &[&2026i32, &8u32]);
    calendar.emit_by_name::<()>("day-selected", &[&2026i32, &8u32, &9u32]);
    drop_down.set_selected(1);
    toggle.set_active(true);
    check.set_active(false);
    let events = events.borrow();
    assert_eq!(
        events
            .iter()
            .map(|event| event.name.as_str())
            .collect::<Vec<_>>(),
        [
            "onSelect",
            "onActivate",
            "onMonth",
            "onSelect",
            "onSelect",
            "onToggle",
            "onToggle"
        ]
    );
    assert_eq!(events[0].args, vec![json!(0)]);
    assert_eq!(events[2].args, vec![json!(2026), json!(8)]);
    assert_eq!(events[3].args, vec![json!("2026-08-09")]);
    assert_eq!(events[4].args, vec![json!(1)]);
    assert_eq!(events[5].args, vec![json!(true)]);
    assert_eq!(events[6].args, vec![json!(false)]);
}

#[test]
#[ignore = "needs a display"]
fn static_row_dresses_value_tone_and_end_readout() {
    init();
    let first = tree(vec![node(
        2,
        "row",
        json!({"title":"T","value":"V","tone":"warning"}),
        vec![node(3, "readout", json!({"value":"5"}), vec![])],
    )]);
    let (_shell, dom, _) = setup(&first);
    let row = widget(&dom, 1, 2, ElementKind::Row)
        .downcast::<UiRow>()
        .expect("row");
    assert_eq!(
        row.property::<Option<String>>("value").as_deref(),
        Some("V")
    );
    assert_eq!(row.property::<RowTone>("tone"), RowTone::Warning);
    let end = dom
        .nodes
        .get(&(1, 2, ElementKind::Row))
        .and_then(|m| m.inner.clone())
        .expect("end slot");
    assert!(end.is_ancestor(&row));
    assert_eq!(
        widget(&dom, 1, 3, ElementKind::Readout).parent().as_ref(),
        Some(end.upcast_ref())
    );
}

#[test]
#[ignore = "needs a display"]
fn revealer_with_no_child_stays_empty_until_one_is_inserted() {
    init();
    let first = tree(vec![node(
        2,
        "revealer",
        json!({"revealed": true, "transition": "none"}),
        vec![],
    )]);
    let (_shell, mut dom, _) = setup(&first);
    let revealer = widget(&dom, 1, 2, ElementKind::Revealer)
        .downcast::<gtk4::Revealer>()
        .expect("revealer");
    let content = revealer
        .child()
        .and_downcast::<gtk4::Box>()
        .expect("content");
    assert!(content.first_child().is_none());
    let next = first
        .apply(vec![Op::Insert {
            parent: 2,
            node: node(3, "label", json!({"text": "body"}), vec![]),
            before: None,
        }])
        .expect("insert");
    assert!(dom.reconcile(&next, 1));
    assert!(content.first_child().is_some());
    assert_eq!(
        widget(&dom, 1, 3, ElementKind::Label).parent().as_ref(),
        Some(content.upcast_ref())
    );
}

#[test]
#[ignore = "needs a display"]
fn dim_tone_is_softer_than_accent_and_markers_stop_at_three() {
    init();
    let palette = tone_palette();
    let dim = palette[tone_index(Tone::Dim)];
    let accent = palette[tone_index(Tone::Accent)];
    assert!(dim.alpha() < accent.alpha());
    assert!((dim.alpha() - DIM_ALPHA).abs() < 0.001);
    let painted = paint_tones(
        &palette,
        &[
            Tone::Error,
            Tone::Accent,
            Tone::Warning,
            Tone::Success,
            Tone::Dim,
        ],
    );
    assert_eq!(painted.len(), 3);
    assert_eq!(painted[0], palette[tone_index(Tone::Error)]);
}

#[test]
#[ignore = "needs a display"]
fn long_option_and_check_label_stay_narrow() {
    init();
    let medium = "W".repeat(80);
    let long = "W".repeat(200);
    let first = tree(vec![
        node(
            2,
            "dropdown",
            json!({"options": [medium], "selected": 0}),
            vec![],
        ),
        node(
            3,
            "dropdown",
            json!({"options": [long], "selected": 0}),
            vec![],
        ),
        node(4, "checkbutton", json!({"label": medium}), vec![]),
        node(5, "checkbutton", json!({"label": long}), vec![]),
    ]);
    let (shell, dom, _) = setup(&first);
    let window = gtk4::Window::new();
    window.set_child(Some(&shell));
    window.present();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !shell.is_mapped() && std::time::Instant::now() < deadline {
        if !gtk4::glib::MainContext::default().iteration(false) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    let narrow = widget(&dom, 1, 2, ElementKind::DropDown)
        .downcast::<gtk4::DropDown>()
        .expect("dropdown");
    let wide = widget(&dom, 1, 3, ElementKind::DropDown)
        .downcast::<gtk4::DropDown>()
        .expect("dropdown");
    assert!(narrow.factory().is_some());
    assert!(narrow.list_factory().is_some());
    assert!(wide.list_factory().is_some());
    let narrow_label = descendant_label(narrow.upcast_ref(), &medium).expect("option label");
    let wide_label = descendant_label(wide.upcast_ref(), &long).expect("option label");
    assert_eq!(narrow_label.ellipsize(), gtk4::pango::EllipsizeMode::End);
    assert_eq!(narrow_label.max_width_chars(), OPTION_CHARS);
    assert_eq!(narrow_label.text().as_str(), medium);
    assert_eq!(
        narrow_label.measure(gtk4::Orientation::Horizontal, -1).1,
        wide_label.measure(gtk4::Orientation::Horizontal, -1).1
    );
    let short = widget(&dom, 1, 4, ElementKind::CheckButton)
        .downcast::<gtk4::CheckButton>()
        .expect("check");
    let tall = widget(&dom, 1, 5, ElementKind::CheckButton)
        .downcast::<gtk4::CheckButton>()
        .expect("check");
    let short_label = short.child().and_downcast::<gtk4::Label>().expect("label");
    assert_eq!(short_label.ellipsize(), gtk4::pango::EllipsizeMode::End);
    assert_eq!(short_label.max_width_chars(), 48);
    assert_eq!(
        short_label.accessible_role(),
        gtk4::AccessibleRole::Presentation
    );
    assert!(gtk4::test_accessible_has_property(
        &short,
        gtk4::AccessibleProperty::Label
    ));
    assert_eq!(short_label.text().as_str(), medium);
    assert_eq!(
        short.measure(gtk4::Orientation::Horizontal, -1).1,
        tall.measure(gtk4::Orientation::Horizontal, -1).1
    );
    window.close();
}

#[test]
#[ignore = "needs a display"]
fn dropping_shell_drops_the_next_dom() {
    init();
    let first = tree(vec![node(2, "row", json!({"title":"A"}), vec![])]);
    let (shell, mut dom, _) = setup(&first);
    let child = widget(&dom, 1, 2, ElementKind::Row);
    let weak = child.downgrade();
    drop(child);
    drop(shell);
    assert!(!dom.reconcile(&first, 1));
    drop(dom);
    assert!(weak.upgrade().is_none());
}

#[test]
#[ignore = "needs a display"]
fn row_family_mounts_props_and_end_in_the_head() {
    init();
    let built = tree(vec![
        node(
            2,
            "actionrow",
            json!({"icon":"folder-symbolic","title":"Open","tone":"destructive","onActivate":true}),
            vec![node(
                3,
                "image",
                json!({"icon":"emblem-ok-symbolic"}),
                vec![],
            )],
        ),
        node(
            4,
            "togglerow",
            json!({"title":"Wi-Fi","control":"check","active":true,"inactive":true,"subtitle":"Device out of range","onToggle":true}),
            vec![],
        ),
        node(
            5,
            "meterrow",
            json!({"title":"CPU","level":0.3,"value":"30%"}),
            vec![],
        ),
        node(
            6,
            "expanderrow",
            json!({"title":"Device","open":true,"tint":true,"titleLines":3}),
            vec![
                node(
                    7,
                    "end",
                    json!({}),
                    vec![node(
                        8,
                        "image",
                        json!({"icon":"emblem-ok-symbolic"}),
                        vec![],
                    )],
                ),
                node(9, "actionrow", json!({"title":"Connect"}), vec![]),
            ],
        ),
        node(
            10,
            "sliderrow",
            json!({"value":0.5,"min":0.1,"max":0.9,"step":0.05,"inactive":true}),
            vec![node(
                11,
                "start",
                json!({}),
                vec![node(
                    12,
                    "button",
                    json!({"icon":"audio-volume-muted-symbolic","tooltip":"Unmute","className":"flat","onClick":true}),
                    vec![],
                )],
            )],
        ),
    ]);
    let (shell, dom, events) = setup(&built);
    assert!(events.borrow().is_empty());
    let action = widget(&dom, 1, 2, ElementKind::ActionRow)
        .downcast::<ActionRow>()
        .expect("action");
    assert_eq!(action.parent().as_ref(), Some(shell.body().upcast_ref()));
    assert_eq!(
        action.property::<Option<String>>("title").as_deref(),
        Some("Open")
    );
    assert_eq!(action.property::<RowTone>("tone"), RowTone::Destructive);
    let toggle = widget(&dom, 1, 4, ElementKind::ToggleRow)
        .downcast::<ToggleRow>()
        .expect("toggle");
    assert!(toggle.active() && toggle.property::<bool>("inactive"));
    assert_eq!(toggle.control(), ToggleControl::Check);
    let window = gtk4::Window::new();
    window.set_child(Some(&shell));
    window.present();
    while glib::MainContext::default().iteration(false) {}
    let controllers = toggle.observe_controllers();
    let click = (0..controllers.n_items())
        .filter_map(|i| controllers.item(i))
        .find_map(|c| c.downcast::<gtk4::GestureClick>().ok())
        .expect("click");
    click.emit_by_name::<()>(
        "released",
        &[
            &1i32,
            &(f64::from(toggle.width()) / 2.0),
            &(f64::from(toggle.height()) / 2.0),
        ],
    );
    assert!(toggle.active());
    assert!(events.borrow().is_empty());
    window.close();
    let meter = widget(&dom, 1, 5, ElementKind::MeterRow)
        .downcast::<MeterRow>()
        .expect("meter");
    assert_eq!(meter.level(), 0.3);
    let expander = widget(&dom, 1, 6, ElementKind::ExpanderRow)
        .downcast::<ExpanderRow>()
        .expect("expander");
    assert!(expander.open() && expander.tint());
    assert_eq!(expander.title_lines(), 3);
    let end = widget(&dom, 1, 7, ElementKind::End);
    assert!(end.is_ancestor(&expander));
    assert!(!end.is_ancestor(&expander.details()));
    assert!(widget(&dom, 1, 9, ElementKind::ActionRow).is_ancestor(&expander.details()));
    let slider = widget(&dom, 1, 10, ElementKind::SliderRow)
        .downcast::<SliderRow>()
        .expect("slider");
    assert_eq!(slider.value(), 0.5);
    assert_eq!(slider.step(), 0.05);
    let start = widget(&dom, 1, 11, ElementKind::Start);
    assert!(start.is_ancestor(&slider));
    let button = widget(&dom, 1, 12, ElementKind::Button)
        .downcast::<gtk4::Button>()
        .expect("button");
    assert!(button.is_sensitive());
    button.emit_clicked();
    assert!(
        events
            .borrow()
            .iter()
            .any(|event| event.id == 12 && event.name == "onClick")
    );
}

#[test]
#[ignore = "needs a display"]
fn row_family_keeps_local_values_and_reports_shell_close() {
    init();
    let first = tree(vec![
        node(
            2,
            "togglerow",
            json!({"active":false,"onToggle":true}),
            vec![],
        ),
        node(
            3,
            "expanderrow",
            json!({"title":"A","open":true,"onToggle":true}),
            vec![node(4, "actionrow", json!({"title":"A1"}), vec![])],
        ),
        node(
            5,
            "expanderrow",
            json!({"title":"B","open":false,"onToggle":true}),
            vec![node(6, "actionrow", json!({"title":"B1"}), vec![])],
        ),
        node(7, "sliderrow", json!({"value":0.2,"onChange":true}), vec![]),
    ]);
    let (_shell, mut dom, events) = setup(&first);
    let toggle = widget(&dom, 1, 2, ElementKind::ToggleRow)
        .downcast::<ToggleRow>()
        .expect("toggle");
    toggle.set_active(true);
    toggle.emit_by_name::<()>("toggled", &[&true]);
    let slider = widget(&dom, 1, 7, ElementKind::SliderRow)
        .downcast::<SliderRow>()
        .expect("slider");
    slider.set_value(0.8);
    slider.emit_by_name::<()>("changed", &[&0.8f64]);
    let echo = first
        .apply(vec![
            Op::Set {
                id: 2,
                props: serde_json::from_value(json!({"active":false})).expect("props"),
                seq: Some(0),
            },
            Op::Set {
                id: 7,
                props: serde_json::from_value(json!({"value":0.2})).expect("props"),
                seq: Some(0),
            },
        ])
        .expect("echo");
    assert!(dom.reconcile(&echo, 1));
    assert!(toggle.active());
    assert_eq!(slider.value(), 0.8);
    let open = widget(&dom, 1, 3, ElementKind::ExpanderRow)
        .downcast::<ExpanderRow>()
        .expect("open");
    let shut = widget(&dom, 1, 5, ElementKind::ExpanderRow)
        .downcast::<ExpanderRow>()
        .expect("shut");
    let next = echo
        .apply(vec![set(5, json!({"open":true}))])
        .expect("open other");
    assert!(dom.reconcile(&next, 1));
    assert!(!open.open() && shut.open());
    let close = events
        .borrow()
        .iter()
        .find(|event| event.id == 3 && event.name == "onToggle" && event.args == vec![json!(false)])
        .map(|event| event.seq.expect("seq"))
        .expect("shell close");
    let stale = next
        .apply(vec![Op::Set {
            id: 3,
            props: serde_json::from_value(json!({"open":true})).expect("props"),
            seq: Some(close - 1),
        }])
        .expect("stale open");
    assert!(dom.reconcile(&stale, 1));
    assert!(!open.open());
}

#[test]
#[ignore = "needs a display"]
fn icon_only_toggle_has_an_accessible_name() {
    init();
    let first = tree(vec![node(
        2,
        "togglebutton",
        json!({"icon": "media-playback-start-symbolic"}),
        vec![],
    )]);
    let (_shell, mut dom, _) = setup(&first);
    let toggle = widget(&dom, 1, 2, ElementKind::ToggleButton)
        .downcast::<gtk4::ToggleButton>()
        .expect("toggle");
    assert!(gtk4::test_accessible_has_property(
        &toggle,
        gtk4::AccessibleProperty::Label
    ));
    let next = first
        .apply(vec![Op::Set {
            id: 2,
            props: serde_json::from_value(json!({"label": "Play"})).expect("props"),
            seq: None,
        }])
        .expect("set");
    assert!(dom.reconcile(&next, 1));
    assert!(gtk4::test_accessible_has_property(
        &toggle,
        gtk4::AccessibleProperty::Label
    ));
    let empty = next
        .apply(vec![Op::Set {
            id: 2,
            props: serde_json::from_value(json!({"label": null, "icon": null})).expect("props"),
            seq: None,
        }])
        .expect("clear");
    assert!(dom.reconcile(&empty, 1));
    assert!(!gtk4::test_accessible_has_property(
        &toggle,
        gtk4::AccessibleProperty::Label
    ));
}

#[test]
#[ignore = "needs a display"]
fn style_notifications_redress_unchanged_calendar_markers() {
    init();
    let first = tree(vec![node(
        2,
        "calendar",
        json!({"events": [{"date": "2026-06-05", "colors": ["accent"]}]}),
        vec![],
    )]);
    let (_shell, mut dom, events) = setup(&first);
    let calendar = widget(&dom, 1, 2, ElementKind::Calendar)
        .downcast::<Calendar>()
        .expect("calendar");
    let date = Ymd::new(2026, 6, 5);
    for property in ["dark", "accent-color"] {
        calendar.set_events(&[]);
        assert!(dom.reconcile(&first, 1));
        assert!(calendar.events(date).is_empty());
        dom.style_manager.notify(property);
        assert!(dom.needs_redress());
        assert!(dom.reconcile(&first, 1));
        assert_eq!(calendar.events(date), vec![tone_palette()[0]]);
        assert!(!dom.needs_redress());
        assert!(events.borrow().is_empty());
    }
    let dirty = dom.tones_dirty.clone();
    let manager = dom.style_manager.clone();
    drop(dom);
    manager.notify("dark");
    assert!(!dirty.get());
}

#[test]
#[ignore = "needs a display"]
fn swatch_takes_its_color_and_the_large_size() {
    init();
    let built = tree(vec![node(
        2,
        "addon",
        json!({}),
        vec![node(
            3,
            "swatch",
            json!({"color": "#e0563f", "size": "large", "tooltip": "Latest"}),
            vec![],
        )],
    )]);
    let (_shell, mut dom, _) = setup(&built);
    let swatch = widget(&dom, 1, 3, ElementKind::Swatch)
        .downcast::<glimpse_widgets::Swatch>()
        .expect("swatch");
    assert_eq!(swatch.color(), gdk::RGBA::parse("#e0563f").ok());
    assert!(swatch.has_css_class("swatch--hero"));
    assert_eq!(swatch.tooltip_text().as_deref(), Some("Latest"));
    let regular = built
        .apply(vec![Op::Set {
            id: 3,
            props: serde_json::from_value(json!({"color": "not a color", "size": null}))
                .expect("props"),
            seq: None,
        }])
        .expect("set");
    assert!(dom.reconcile(&regular, 1));
    assert_eq!(swatch.color(), None);
    assert!(!swatch.has_css_class("swatch--hero"));
}
