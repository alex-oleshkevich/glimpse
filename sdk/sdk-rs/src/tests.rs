use super::*;
use serde_json::Value;
use std::sync::mpsc::{Receiver, channel};

#[derive(Debug)]
enum Msg {
    Toggle(usize, bool),
    Draft(String),
}

struct TestApp {
    items: Vec<(String, String, bool)>,
    draft: String,
}

impl App for TestApp {
    type Msg = Msg;

    fn update(&mut self, _ctx: &Ctx, msg: Msg) {
        match msg {
            Msg::Toggle(index, done) => self.items[index].2 = done,
            Msg::Draft(value) => self.draft = value,
        }
    }

    fn build(&self, _ctx: &Ctx) -> Vec<Widget<Msg>> {
        let rows = self
            .items
            .iter()
            .enumerate()
            .map(|(index, (id, title, done))| {
                toggle_row()
                    .key(id.clone())
                    .title(title.clone())
                    .control(ToggleControl::Check)
                    .active(*done)
                    .on_toggle(move |value| Msg::Toggle(index, value))
            });
        vec![
            indicator()
                .icon("view-list-symbolic")
                .badge("2")
                .text("Todo")
                .into(),
            popover()
                .title("Todo")
                .child(section().title("Todo").children(rows))
                .child(
                    entry()
                        .placeholder("Add")
                        .value(self.draft.clone())
                        .on_change(Msg::Draft),
                )
                .into(),
        ]
    }
}

fn hello() -> Value {
    json!({
        "t": "hello", "v": 1, "name": "test", "state": "/tmp/x",
        "options": {}, "placement": {"output": null, "position": "bottom", "orientation": "horizontal", "zone": "left", "size": 40},
    })
}

struct Host {
    session: Session<TestApp>,
    rx: Receiver<In>,
    tree: Value,
}

impl Host {
    fn new() -> Self {
        let (tx, rx) = channel();
        let mut host = Self {
            session: Session::new(
                TestApp {
                    items: vec![
                        ("a".to_owned(), "Alpha".to_owned(), false),
                        ("b".to_owned(), "Bravo".to_owned(), false),
                    ],
                    draft: String::new(),
                },
                tx,
            ),
            rx,
            tree: json!({"id": 0, "type": "root", "props": {}, "children": []}),
        };
        host.feed(&hello());
        host
    }

    fn feed(&mut self, msg: &Value) -> Vec<Value> {
        self.session.handle(msg);
        self.session.flush().expect("flush");
        let mut sent: Vec<Value> = Vec::new();
        while let Ok(In::Send(line)) = self.rx.try_recv() {
            sent.push(serde_json::from_str(&line).expect("sent line parses"));
        }
        for msg in &sent {
            if msg["t"] == "commit" {
                for op in msg["ops"].as_array().expect("ops") {
                    apply(&mut self.tree, op);
                }
            }
        }
        sent
    }

    fn find(&self, kind: &str, title: Option<&str>) -> Value {
        fn walk(node: &Value, kind: &str, title: Option<&str>, out: &mut Option<Value>) {
            if node["type"] == kind && title.is_none_or(|title| node["props"]["title"] == title) {
                *out = Some(node.clone());
                return;
            }
            for child in node["children"].as_array().into_iter().flatten() {
                walk(child, kind, title, out);
            }
        }
        let mut out = None;
        walk(&self.tree, kind, title, &mut out);
        out.expect("node exists")
    }
}

fn apply(tree: &mut Value, op: &Value) {
    fn find_mut(node: &mut Value, id: u64) -> Option<&mut Value> {
        if node["id"].as_u64() == Some(id) {
            return Some(node);
        }
        for child in node["children"].as_array_mut()?.iter_mut() {
            if let Some(found) = find_mut(child, id) {
                return Some(found);
            }
        }
        None
    }
    fn detach(node: &mut Value, id: u64) -> Option<Value> {
        let children = node["children"].as_array_mut()?;
        if let Some(position) = children
            .iter()
            .position(|child| child["id"].as_u64() == Some(id))
        {
            return Some(children.remove(position));
        }
        for child in children.iter_mut() {
            if let Some(found) = detach(child, id) {
                return Some(found);
            }
        }
        None
    }
    match op["op"].as_str() {
        Some("insert") => {
            let parent =
                find_mut(tree, op["parent"].as_u64().expect("parent")).expect("parent node");
            let children = parent["children"].as_array_mut().expect("children");
            let position = match op["before"].as_u64() {
                Some(before) => children
                    .iter()
                    .position(|child| child["id"].as_u64() == Some(before))
                    .unwrap_or(children.len()),
                None => children.len(),
            };
            children.insert(position, op["node"].clone());
        }
        Some("remove") => {
            let _ = detach(tree, op["id"].as_u64().expect("id"));
        }
        Some("move") => {
            let node = detach(tree, op["id"].as_u64().expect("id")).expect("moved node");
            let parent =
                find_mut(tree, op["parent"].as_u64().expect("parent")).expect("parent node");
            let children = parent["children"].as_array_mut().expect("children");
            let position = match op["before"].as_u64() {
                Some(before) => children
                    .iter()
                    .position(|child| child["id"].as_u64() == Some(before))
                    .unwrap_or(children.len()),
                None => children.len(),
            };
            children.insert(position, node);
        }
        Some("set") => {
            let node = find_mut(tree, op["id"].as_u64().expect("id")).expect("node");
            for (key, value) in op["props"].as_object().expect("props") {
                if value.is_null() {
                    node["props"].as_object_mut().expect("props").remove(key);
                } else {
                    node["props"][key] = value.clone();
                }
            }
        }
        _ => {}
    }
}

fn ops(sent: &[Value]) -> Vec<Value> {
    sent.iter()
        .filter(|msg| msg["t"] == "commit")
        .flat_map(|msg| msg["ops"].as_array().cloned().unwrap_or_default())
        .collect()
}

#[test]
fn mount_builds_tree() {
    let host = Host::new();
    assert_eq!(host.find("indicator", None)["props"]["text"], "Todo");
    assert_eq!(
        host.find("togglerow", Some("Alpha"))["props"]["onToggle"],
        true
    );
}

#[test]
fn toggle_sets_only_changed_prop_with_seq() {
    let mut host = Host::new();
    let id = host.find("togglerow", Some("Alpha"))["id"].clone();
    let sent =
        host.feed(&json!({"t": "event", "id": id, "name": "onToggle", "args": [true], "seq": 7}));
    let ops = ops(&sent);
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0]["op"], "set");
    assert_eq!(ops[0]["props"], json!({"active": true}));
    assert_eq!(ops[0]["seq"], 7);
    assert!(host.session.app().items[0].2);
}

#[test]
fn stale_seq_is_dropped() {
    let mut host = Host::new();
    let id = host.find("togglerow", Some("Alpha"))["id"].clone();
    host.feed(&json!({"t": "event", "id": id, "name": "onToggle", "args": [true], "seq": 7}));
    host.feed(&json!({"t": "event", "id": id, "name": "onToggle", "args": [false], "seq": 3}));
    assert!(host.session.app().items[0].2);
}

#[test]
fn keyed_reorder_moves() {
    let mut host = Host::new();
    let before = host.find("togglerow", Some("Alpha"))["id"].clone();
    host.session.app_mut().items.reverse();
    let sent = host.feed(&json!({"t": "popover", "open": true}));
    assert!(ops(&sent).iter().any(|op| op["op"] == "move"));
    assert_eq!(host.find("togglerow", Some("Alpha"))["id"], before);
    let section = host.find("section", None);
    assert_eq!(section["children"][0]["props"]["title"], "Bravo");
}

#[test]
fn all_host_requests_reach_the_wire() {
    let host = Host::new();
    let ctx = host.session.ctx();
    ctx.notify("Hi", Some("b"), Some("i"), Some(Urgency::Low.as_str()));
    ctx.copy("x");
    ctx.open_uri("https://example.org");
    ctx.launch("org.example.App");
    ctx.close_popover();
    ctx.mpris_control("p", "play-pause");
    ctx.mpris_seek("p", -5_000_000);
    ctx.mpris_set_position("p", 12_000_000);
    ctx.focus_workspace(json!({"index": 1}));
    ctx.focus_window(json!("next"));
    ctx.move_window(7, json!({"name": "web"}));
    ctx.rename_workspace(3, None);
    ctx.close_window(7);
    let mut kinds = Vec::new();
    while let Ok(In::Send(line)) = host.rx.try_recv() {
        let value: Value = serde_json::from_str(&line).expect("sent line parses");
        kinds.push(value["t"].as_str().unwrap_or("").to_owned());
    }
    assert_eq!(
        kinds.join(","),
        "notify,copy,open-uri,launch,close-popover,mpris,mpris,mpris,compositor,compositor,compositor,compositor,compositor"
    );
}

#[test]
fn string_caps_match_the_host() {
    let widget: Widget<Msg> = indicator()
        .badge("123456789")
        .tooltip("x".repeat(300))
        .into();
    assert_eq!(widget.props["badge"].as_str().map(str::len), Some(8));
    assert_eq!(widget.props["tooltip"].as_str().map(str::len), Some(256));
    let entry: Widget<Msg> = entry().value("y".repeat(5000)).into();
    assert_eq!(entry.props["value"].as_str().map(str::len), Some(4096));
}

#[test]
fn typed_wire_matches_previous_json() {
    let typed: Widget<Msg> = toggle_row()
        .key("a")
        .title("Alpha")
        .control(ToggleControl::Check)
        .active(true)
        .tone(RowTone::Warning)
        .on_toggle(|_| Msg::Draft(String::new()))
        .into();
    assert_eq!(typed.kind, "togglerow");
    assert_eq!(typed.key, Some(Key::S("a".to_owned())));
    assert_eq!(
        json!(typed.props),
        json!({"title": "Alpha", "control": "check", "active": true, "tone": "warning", "onToggle": true})
    );
    let typed: Widget<Msg> = notice()
        .icon("i")
        .title("T")
        .severity(Severity::Error)
        .class_name(ClassName::DimLabel)
        .into();
    assert_eq!(
        json!(typed.props),
        json!({"icon": "i", "title": "T", "severity": "error", "className": "dim-label"})
    );
    let typed: Widget<Msg> = calendar()
        .year(2026)
        .month(10)
        .selected("2026-10-15")
        .events([CalendarEvent::new("2026-10-15").colors([Tone::Accent, Tone::Success])])
        .into();
    assert_eq!(
        json!(typed.props),
        json!({"year": 2026, "month": 10, "selected": "2026-10-15", "events": [{"date": "2026-10-15", "colors": ["accent", "success"]}]})
    );
    let typed: Widget<Msg> = bars()
        .values([Some(0.0), None, Some(1.5)])
        .maximum(Some(2.0))
        .highlight_last(Some(false))
        .tooltips([Some("a"), None])
        .into();
    assert_eq!(
        json!(typed.props),
        json!({"values": [0.0, null, 1.5], "maximum": 2.0, "highlightLast": false, "tooltips": ["a", null]})
    );
    let typed: Widget<Msg> = fact_list()
        .facts([Fact::new("K", "V"), Fact::new("W", "1").warning(true)])
        .into();
    assert_eq!(
        json!(typed.props),
        json!({"facts": [{"label": "K", "value": "V"}, {"label": "W", "value": "1", "warning": true}]})
    );
    let typed: Widget<Msg> = choice_list()
        .choices([
            Choice::new("First").detail("d"),
            Choice::new("Second").icon("ok"),
        ])
        .selected(Some(0))
        .into();
    assert_eq!(
        json!(typed.props),
        json!({"choices": [{"label": "First", "detail": "d"}, {"label": "Second", "icon": "ok"}], "selected": 0})
    );
}

#[test]
fn enums_serialize_to_wire_strings() {
    assert_eq!(Severity::Info.as_str(), "info");
    assert_eq!(Tone::Accent.as_str(), "accent");
    assert_eq!(RowTone::Destructive.as_str(), "destructive");
    assert_eq!(RowTypography::Monospace.as_str(), "monospace");
    assert_eq!(Paint::Blue.as_str(), "blue");
    assert_eq!(Align::Baseline.as_str(), "baseline");
    assert_eq!(PopoverSize::Wide.as_str(), "wide");
    assert_eq!(ToggleControl::Switch.as_str(), "switch");
    assert_eq!(Orientation::Vertical.as_str(), "vertical");
    assert_eq!(Ellipsize::Middle.as_str(), "middle");
    assert_eq!(SwatchSize::Large.as_str(), "large");
    assert_eq!(Transition::SlideDown.as_str(), "slide-down");
    assert_eq!(Transition::None.as_str(), "none");
    assert_eq!(Urgency::Critical.as_str(), "critical");
    assert_eq!(ClassName::DimLabel.as_str(), "dim-label");
    assert_eq!(ClassName::Title1.as_str(), "title-1");
    assert_eq!(
        ClassName::NetworkPopoverRule.as_str(),
        "network-popover__rule"
    );
    assert_eq!(ClassName::NetworkPopover.as_str(), "network-popover");
    let widget: Widget<Msg> = label().text("x").class_name(ClassName::Title2).into();
    assert_eq!(widget.props["className"], json!("title-2"));
    let unset: Widget<Msg> = label().text("x").into();
    assert!(!unset.props.contains_key("className"));
    let cleared: Widget<Msg> = image().pixel_size(Some(32)).pixel_size(None).into();
    assert!(!cleared.props.contains_key("pixelSize"));
    let kept: Widget<Msg> = switch().sensitive(Some(false)).into();
    assert_eq!(kept.props["sensitive"], json!(false));
}

#[test]
fn typed_handlers_dispatch() {
    let mut host = Host::new();
    let id = host.find("entry", None)["id"].clone();
    host.feed(&json!({"t": "event", "id": id, "name": "onChange", "args": ["hi"], "seq": 1}));
    assert_eq!(host.session.app().draft, "hi");
    let widget: Widget<Msg> = drop_down()
        .options(["a", "b"])
        .selected(Some(1))
        .on_select(|index| Msg::Toggle(index, true))
        .into();
    assert_eq!(widget.props["onSelect"], json!(true));
    assert!(matches!(
        widget.handlers["onSelect"](&[json!(3)]),
        Some(Msg::Toggle(3, true))
    ));
    let widget: Widget<Msg> = calendar().on_select(Msg::Draft).into();
    assert!(matches!(
        widget.handlers["onSelect"](&[json!("2026-01-01")]).as_ref(),
        Some(Msg::Draft(date)) if date == "2026-01-01"
    ));
    let widget: Widget<Msg> = scale()
        .on_change(|value| Msg::Draft(value.to_string()))
        .into();
    assert!(matches!(
        widget.handlers["onChange"](&[json!(2.5)]).as_ref(),
        Some(Msg::Draft(text)) if text == "2.5"
    ));
}

#[test]
fn position_now_advances_a_playing_track() {
    let player =
        json!({"playback": "playing", "positionUs": 1_000_000, "positionAt": 1000, "rate": 1});
    assert_eq!(position_now_at(&player, 3000.0), 3_000_000.0);
}

struct WideApp;

impl App for WideApp {
    type Msg = Msg;

    fn update(&mut self, _ctx: &Ctx, _msg: Msg) {}

    fn build(&self, _ctx: &Ctx) -> Vec<Widget<Msg>> {
        vec![
            section()
                .title("x")
                .children((0..250).map(|_| label().text("x")))
                .into(),
        ]
    }
}

#[test]
fn fresh_subtree_over_children_cap_is_a_limit_error() {
    let (tx, _rx) = channel();
    let mut session = Session::new(WideApp, tx);
    session.handle(&hello());
    assert!(matches!(session.flush(), Err(Fail::Limit(_))));
}

struct StartApp {
    starts: usize,
    seen_state_dir: String,
    seen_options: Value,
}

impl App for StartApp {
    type Msg = Msg;

    fn start(&mut self, ctx: &Ctx) {
        self.starts += 1;
        self.seen_state_dir = ctx.state_dir.clone();
        self.seen_options = ctx.options.clone();
    }

    fn update(&mut self, _ctx: &Ctx, _msg: Msg) {}

    fn build(&self, _ctx: &Ctx) -> Vec<Widget<Msg>> {
        vec![indicator().text("hi").into()]
    }
}

#[test]
fn start_runs_once_after_hello_with_options_and_state_dir() {
    let (tx, _rx) = channel();
    let mut session = Session::new(
        StartApp {
            starts: 0,
            seen_state_dir: String::new(),
            seen_options: json!(null),
        },
        tx,
    );
    assert_eq!(session.app().starts, 0);
    session.handle(&json!({
        "t": "hello", "v": 1, "name": "test", "state": "/tmp/glimpse-start",
        "options": {"title": "Hi"}, "placement": null,
    }));
    session.handle(&hello());
    assert_eq!(session.app().starts, 1);
    assert_eq!(session.app().seen_state_dir, "/tmp/glimpse-start");
    assert_eq!(session.app().seen_options, json!({"title": "Hi"}));
    session.flush().expect("flush");
}

fn state_ctx(dir: &std::path::Path) -> (Ctx, Receiver<In>) {
    let (tx, rx) = channel();
    (
        Ctx {
            name: String::new(),
            state_dir: dir.to_string_lossy().into_owned(),
            options: json!({}),
            placement: None,
            popover_open: false,
            services: serde_json::Map::new(),
            tx,
        },
        rx,
    )
}

fn temp_state_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("glimpse-sdk-rs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp state dir");
    dir
}

#[test]
fn waker_wakes_from_another_thread() {
    let host = Host::new();
    let waker = host.session.ctx().waker();
    let clone = waker.clone();
    std::thread::spawn(move || clone.wake())
        .join()
        .expect("thread");
    waker.wake();
    let mut wakes = 0;
    while let Ok(input) = host.rx.try_recv() {
        if matches!(input, In::Wake) {
            wakes += 1;
        }
    }
    assert_eq!(wakes, 2);
}

#[test]
fn state_round_trips_through_the_state_file() {
    let dir = temp_state_dir("round-trip");
    let (ctx, _rx) = state_ctx(&dir);
    let saved = std::collections::HashMap::from([("count".to_owned(), 42_i64)]);
    ctx.save_state(&saved).expect("save state");
    let loaded: Option<std::collections::HashMap<String, i64>> = ctx.load_state();
    assert_eq!(loaded, Some(saved));
    let as_value: Option<Value> = ctx.load_state();
    assert_eq!(as_value, Some(json!({"count": 42})));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_and_corrupt_state_files_are_ignored() {
    let dir = temp_state_dir("corrupt");
    let (ctx, _rx) = state_ctx(&dir);
    assert_eq!(ctx.load_state::<Value>(), None);
    std::fs::write(dir.join("state.json"), "{nope").expect("write corrupt state");
    assert_eq!(ctx.load_state::<Value>(), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn state_is_a_no_op_without_a_state_dir() {
    let (tx, _rx) = channel();
    let ctx = Ctx {
        name: String::new(),
        state_dir: String::new(),
        options: json!({}),
        placement: None,
        popover_open: false,
        services: serde_json::Map::new(),
        tx,
    };
    assert_eq!(ctx.load_state::<Value>(), None);
    ctx.save_state(&json!({"count": 1}))
        .expect("save is a no-op");
}
