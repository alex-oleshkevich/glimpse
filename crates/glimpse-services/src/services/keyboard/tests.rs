use super::*;

fn backend(names: &[&str], codes: &[&str], current: Option<usize>) -> BackendLayouts {
    BackendLayouts {
        names: names.iter().map(|name| (*name).to_owned()).collect(),
        codes: codes.iter().map(|code| (*code).to_owned()).collect(),
        current,
    }
}

fn window(id: u64, app_id: &str, focused: bool) -> WindowInfo {
    WindowInfo {
        id,
        title: None,
        app_id: Some(app_id.to_owned()),
        workspace: None,
        focused,
        floating: false,
        urgent: false,
        order: None,
    }
}

#[test]
fn two_layouts_publish_uppercase_codes() {
    let published = payload(
        &backend(&["English (US)", "Russian"], &["us", "ru"], Some(1)),
        &BTreeMap::new(),
    );
    assert_eq!(
        published.layouts,
        [
            KeyboardLayout {
                code: "US".to_owned(),
                name: "English (US)".to_owned(),
            },
            KeyboardLayout {
                code: "RU".to_owned(),
                name: "Russian".to_owned(),
            },
        ]
    );
    assert_eq!(published.current, Some(1));
}

#[test]
fn a_label_overrides_the_badge() {
    let mut labels = BTreeMap::new();
    labels.insert("us".to_owned(), "EN".to_owned());
    let published = payload(&backend(&["English (US)"], &["us"], Some(0)), &labels);
    assert_eq!(published.layouts[0].code, "EN");
}

#[test]
fn an_empty_list_has_no_current() {
    let published = payload(&BackendLayouts::default(), &BTreeMap::new());
    assert!(published.layouts.is_empty());
    assert_eq!(published.current, None);
}

#[test]
fn a_current_past_the_end_is_dropped() {
    let published = payload(
        &backend(&["English (US)"], &["us"], Some(4)),
        &BTreeMap::new(),
    );
    assert_eq!(published.current, None);
}

#[test]
fn names_are_capped_by_characters() {
    let long = "я".repeat(NAME_CAP + 8);
    let published = payload(&backend(&[&long], &["ru"], Some(0)), &BTreeMap::new());
    assert_eq!(published.layouts[0].name.chars().count(), NAME_CAP);
}

#[test]
fn a_layout_switch_updates_the_current_index() {
    let mut state = backend(&["English (US)", "Russian"], &["us", "ru"], Some(0));
    assert!(matches!(
        apply_keyboard(&mut state, Change::KeyboardLayoutSwitched(1)),
        Apply::Publish
    ));
    assert_eq!(state.current, Some(1));
}

#[test]
fn a_keyboard_resync_asks_for_a_new_snapshot() {
    let mut state = BackendLayouts::default();
    assert!(matches!(
        apply_keyboard(&mut state, Change::Resync(Resync::Keyboard)),
        Apply::Refetch
    ));
    assert!(matches!(
        apply_keyboard(&mut state, Change::Resync(Resync::Structure)),
        Apply::Ignore
    ));
}

#[test]
fn global_remember_has_no_key() {
    assert_eq!(
        remember_key(Remember::Global, Some(&window(1, "firefox", true))),
        None
    );
}

#[test]
fn window_remember_keys_on_the_focused_id() {
    assert_eq!(
        remember_key(Remember::Window, Some(&window(7, "firefox", true))),
        Some(StoreKey::Window(7))
    );
}

#[test]
fn app_remember_keys_on_the_app_id() {
    assert_eq!(
        remember_key(Remember::App, Some(&window(7, "firefox", true))),
        Some(StoreKey::App("firefox".to_owned()))
    );
    assert_eq!(
        remember_key(
            Remember::App,
            Some(&WindowInfo {
                app_id: None,
                ..window(7, "", true)
            })
        ),
        None
    );
}

#[test]
fn next_and_prev_wrap() {
    assert_eq!(stepped(1, LayoutRef::Next, 2), Some(0));
    assert_eq!(stepped(0, LayoutRef::Prev, 2), Some(1));
}

fn keyboard() -> (Keyboard, Ctx<Keyboard>, impl Sized) {
    use glimpse_dbus::Buses;
    use tokio::sync::{mpsc, watch};
    use tokio_util::sync::CancellationToken;

    use crate::service::{NoConfig, ServiceRuntime, ServiceState};

    let cancel = CancellationToken::new();
    let (_runtime, compositor) = ServiceRuntime::<crate::services::Compositor>::new(
        NoConfig,
        Buses::unavailable("no bus in tests"),
        cancel.clone(),
    );
    let (events, inbox) = mpsc::channel(32);
    let (health, _health_rx) = watch::channel(ServiceState::Starting);
    let (published, _state) = watch::channel(KeyboardLayouts::default());
    let ctx = Ctx::<Keyboard>::new(
        events,
        &cancel,
        published,
        health,
        Buses::unavailable("no bus in tests"),
    );
    let service = Keyboard {
        backend: Backend::Unsupported,
        layouts: ctx.publisher(),
        compositor,
        state: BackendLayouts::default(),
        config: Config {
            remember: Remember::Global,
            labels: BTreeMap::new(),
        },
        store: HashMap::new(),
        focused: None,
        attempt: 0,
        fetch_failed: false,
    };
    (service, ctx, (inbox, cancel.drop_guard()))
}

fn fetch_attempt(service: &Keyboard) -> u64 {
    service
        .subscriptions()
        .iter()
        .find_map(|sub| match sub.key() {
            Watch::Fetch { attempt } => Some(*attempt),
            _ => None,
        })
        .expect("a fetch is always declared")
}

async fn changed(service: &mut Keyboard, ctx: &Ctx<Keyboard>) {
    service
        .handle(
            ctx,
            Input::Event(Event::Changed(Change::KeyboardLayoutSwitched(0))),
        )
        .await;
}

#[tokio::test]
async fn a_failed_first_fetch_is_retried_on_the_next_change() {
    let (mut service, ctx, _guard) = keyboard();
    let before = fetch_attempt(&service);
    service
        .handle(&ctx, Input::Event(Event::FetchFailed("down".to_owned())))
        .await;

    changed(&mut service, &ctx).await;

    assert_ne!(fetch_attempt(&service), before);
}

#[tokio::test]
async fn changes_while_the_first_fetch_is_in_flight_do_not_restart_it() {
    let (mut service, ctx, _guard) = keyboard();
    let before = fetch_attempt(&service);

    for _ in 0..5 {
        changed(&mut service, &ctx).await;
    }

    assert_eq!(fetch_attempt(&service), before);
}

#[tokio::test]
async fn reading_the_layouts_asks_the_compositor_for_nothing_else() {
    use std::sync::{Arc, Mutex};

    use glimpse_compositors::Niri;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;

    let dir = tempfile::tempdir().expect("a temporary directory");
    let socket = dir.path().join("niri.sock");
    let listener = UnixListener::bind(&socket).expect("bind the fake niri socket");
    let requests: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = requests.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let sink = sink.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stream);
                let mut request = String::new();
                if reader.read_line(&mut request).await.is_err() {
                    return;
                }
                sink.lock().expect("lock").push(request.trim().to_owned());
                let reply = r#"{"Ok":{"KeyboardLayouts":{"names":["Polish"],"current_idx":0}}}"#;
                let _ = reader
                    .get_mut()
                    .write_all(format!("{reply}\n").as_bytes())
                    .await;
            });
        }
    });

    let event = fetch_layouts(&Backend::Niri(Niri::at(socket))).await;

    assert!(matches!(event, Event::Snapshot(layouts) if layouts.codes == ["PL"]));
    assert_eq!(*requests.lock().expect("lock"), [r#""KeyboardLayouts""#]);
}
