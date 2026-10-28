use std::time::Duration;

use glimpse_compositors::client::CompositorState as Mirrored;
use glimpse_compositors::model::Focus;
use glimpse_compositors::{event::Event as BackendEvent, model::WindowId};
use glimpse_dbus::{Buses, testing::PrivateBus};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::Ctx;
use crate::service::{Running, ServiceState};
use crate::services::compositor::tests::{
    Everything, Mirror, Rig, cast, output, serve_on, snapshot_of, until, window as backend_window,
    workspace,
};

fn served(names: &[&str], codes: &[&str], current: Option<u32>) -> Served {
    Served {
        names: names.iter().map(|name| (*name).to_owned()).collect(),
        codes: codes.iter().map(|code| (*code).to_owned()).collect(),
        current,
    }
}

fn codes() -> BTreeMap<String, String> {
    parse_codes(BUILT_IN_CODES).expect("built-in language codes")
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
fn hyprland_layouts_publish_uppercase_codes() {
    let published = payload(
        &served(
            &["English (US)", "German (Switzerland)"],
            &["us", "de_ch"],
            Some(1),
        ),
        &BTreeMap::new(),
        &codes(),
    );
    assert_eq!(
        published.layouts,
        [
            KeyboardLayout {
                code: "US".to_owned(),
                name: "English (US)".to_owned(),
            },
            KeyboardLayout {
                code: "DE_CH".to_owned(),
                name: "German (Switzerland)".to_owned(),
            },
        ]
    );
    assert_eq!(published.current, Some(1));
}

#[test]
fn an_unknown_name_uses_its_first_two_letters() {
    let published = payload(
        &served(&["Unknown layout"], &[], Some(0)),
        &BTreeMap::new(),
        &codes(),
    );
    assert_eq!(published.layouts[0].code, "UN");
    assert_eq!(published.layouts[0].name, "Unknown layout");
}

#[test]
fn a_label_overrides_the_derived_badge_case_insensitively() {
    for key in ["us", "US", "uS", "eNGLISH (us)"] {
        let labels = BTreeMap::from([(key.to_owned(), "custom".to_owned())]);
        let published = payload(
            &served(&["English (US)"], &["us"], Some(0)),
            &labels,
            &codes(),
        );
        assert_eq!(published.layouts[0].code, "custom");
        assert_eq!(published.layouts[0].name, "English (US)");
    }
}

#[test]
fn a_names_only_layout_matches_short_code_labels_case_insensitively() {
    for key in ["EN", "en", "eNGLISH (us)"] {
        let labels = BTreeMap::from([(key.to_owned(), "custom".to_owned())]);
        let published = payload(&served(&["English (US)"], &[], Some(0)), &labels, &codes());
        assert_eq!(published.layouts[0].code, "custom", "label key: {key}");
        assert_eq!(published.layouts[0].name, "English (US)");
    }
}

#[test]
fn a_current_past_the_end_is_dropped() {
    let published = payload(
        &served(&["English (US)"], &["us"], Some(4)),
        &BTreeMap::new(),
        &codes(),
    );
    assert_eq!(published.current, None);
}

#[test]
fn names_are_capped_by_characters() {
    let long = "я".repeat(NAME_CAP + 8);
    let published = payload(
        &served(&[&long], &["ru"], Some(0)),
        &BTreeMap::new(),
        &codes(),
    );
    assert_eq!(published.layouts[0].name.chars().count(), NAME_CAP);
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

fn keyboard_on(mirror: &Mirror, remember: Remember) -> (Running<Keyboard>, KeyboardHandle) {
    let mut document = glimpse_config::Config::default();
    document.keyboard.remember = remember;
    Running::<Keyboard>::spawn(
        &document,
        Buses::unavailable("no bus in tests"),
        Dependencies {
            provider: ProviderHandle(mirror.provider.handle()),
        },
    )
}

fn two_layouts() -> glimpse_compositors::model::Snapshot {
    let mut snapshot = snapshot_of(
        vec![workspace(1, Some(1), "DP-1")],
        vec![backend_window(1, 1), backend_window(2, 1)],
    );
    snapshot.keyboard = served(&["English (US)", "Polish"], &["us", "pl"], Some(0));
    snapshot.focus.window = Some(WindowId(1));
    snapshot.outputs = vec![output("DP-1", true)];
    snapshot
}

#[tokio::test]
async fn names_only_layouts_publish_one_layout_each() {
    let mut snapshot = two_layouts();
    snapshot.keyboard = served(&["English (US)", "German"], &[], Some(0));
    let rig = Rig::start(snapshot).await;
    let (_running, keyboard) = keyboard_on(&rig.mirror, Remember::Global);
    let mut layouts = keyboard.subscribe();

    let published = until(&mut layouts, "the layouts", |layouts| {
        !layouts.layouts.is_empty()
    })
    .await;

    assert_eq!(
        published.layouts,
        [
            KeyboardLayout {
                code: "EN".to_owned(),
                name: "English (US)".to_owned(),
            },
            KeyboardLayout {
                code: "DE".to_owned(),
                name: "German".to_owned(),
            },
        ]
    );
    assert_eq!(published.current, Some(0));
}

#[tokio::test]
async fn stepping_uses_the_provider_with_a_stale_or_missing_current() {
    let mut held = Held::start(Remember::Window).await;
    for current in [Some(1), None] {
        held.follow(layouts_of(1, &["English (US)", "Polish"], current))
            .await;
        for (target, expected) in [(LayoutRef::Next, 1), (LayoutRef::Prev, 0)] {
            assert_eq!(switched(&mut held.service, &held.ctx, target).await, Ok(()));
            let Input::Event(Event::Switched(id, index)) =
                held.events.try_recv().expect("the selected layout")
            else {
                panic!("the switch must remember its native result");
            };
            assert_eq!(
                held.service.operations.get(&id),
                Some(&Some(StoreKey::Window(1)))
            );
            assert_eq!(index, Some(expected));
            held.service
                .handle(&held.ctx, Input::Event(Event::Switched(id, index)))
                .await;
            held.follow(layouts_of(
                1,
                &["English (US)", "Polish"],
                Some(u32::from(expected)),
            ))
            .await;
            held.completed().await;
        }
    }
    assert_eq!(
        held.rig.calls(),
        [
            "step_keyboard_layout true",
            "step_keyboard_layout false",
            "step_keyboard_layout true",
            "step_keyboard_layout false",
        ]
    );
    assert!(held.events.try_recv().is_err());
}

#[tokio::test]
async fn with_no_owner_the_keyboard_publishes_nothing_runs_and_refuses_switches() {
    let bus = PrivateBus::start();
    let provider = glimpse_compositors::client::CompositorProvider::start(bus.connection().await);
    let mut mirrored = provider.handle().subscribe();
    until(&mut mirrored, "the resolved loss", |state| {
        state
            .unavailable
            .as_deref()
            .is_some_and(|reason| reason != glimpse_dbus::PENDING)
    })
    .await;

    let (_running, keyboard) = Running::<Keyboard>::spawn(
        &glimpse_config::Config::default(),
        Buses::unavailable("no bus in tests"),
        Dependencies {
            provider: ProviderHandle(provider.handle()),
        },
    );
    let mut health = keyboard.health();
    until(&mut health, "running health", |health| {
        *health == ServiceState::Running
    })
    .await;
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;

    assert_eq!(*health.borrow(), ServiceState::Running);
    let first = keyboard.switch(LayoutRef::Index { index: 1 }).await;
    let second = keyboard.switch(LayoutRef::Index { index: 1 }).await;

    assert!(
        matches!(first, Err(CommandError::Unsupported(_))),
        "{first:?}"
    );
    assert!(
        matches!(second, Err(CommandError::Unsupported(_))),
        "{second:?}"
    );
    assert_eq!(keyboard.snapshot(), KeyboardLayouts::default());
    assert_eq!(*health.borrow(), ServiceState::Running);
}

#[tokio::test]
async fn a_lost_owner_degrades_the_keyboard_keeps_its_layouts_and_a_new_owner_recovers() {
    let rig = Rig::start(two_layouts()).await;
    let (_running, keyboard) = keyboard_on(&rig.mirror, Remember::Global);
    let mut layouts = keyboard.subscribe();
    let before = until(&mut layouts, "the layouts", |layouts| {
        layouts.current.is_some()
    })
    .await;
    let mut health = keyboard.health();
    until(&mut health, "running health", |health| {
        *health == ServiceState::Running
    })
    .await;

    rig.served.task.abort();
    until(&mut health, "degraded health", |health| {
        matches!(health, ServiceState::Degraded { .. })
    })
    .await;

    assert_eq!(keyboard.snapshot(), before);
    assert!(matches!(
        keyboard.switch(LayoutRef::Index { index: 1 }).await,
        Err(CommandError::Unavailable(_))
    ));

    let mut replacement = two_layouts();
    replacement.keyboard = served(&["Polish"], &["pl"], Some(0));
    let _served = serve_on::<Everything>(&rig.bus, replacement).await;
    until(&mut health, "running health again", |health| {
        *health == ServiceState::Running
    })
    .await;
    let recovered = until(&mut layouts, "the new layouts", |layouts| {
        layouts.layouts.len() == 1
    })
    .await;
    assert_eq!(recovered.layouts[0].code, "PL");
}

#[tokio::test]
async fn focusing_a_window_with_a_remembered_layout_restores_it_once() {
    let mut rig = Rig::start(two_layouts()).await;
    let (_running, keyboard) = keyboard_on(&rig.mirror, Remember::Window);
    let mut layouts = keyboard.subscribe();
    until(&mut layouts, "the layouts", |layouts| {
        layouts.current.is_some()
    })
    .await;

    rig.send(BackendEvent::KeyboardLayoutSwitched(1));
    until(&mut layouts, "layout 1", |layouts| {
        layouts.current == Some(1)
    })
    .await;
    rig.send(BackendEvent::WindowFocusChanged(Some(WindowId(2))));
    rig.send(BackendEvent::KeyboardLayoutSwitched(0));
    until(&mut layouts, "layout 0", |layouts| {
        layouts.current == Some(0)
    })
    .await;

    rig.send(BackendEvent::WindowFocusChanged(Some(WindowId(1))));
    until(&mut rig.served.shared.recorded(), "the restore", |count| {
        *count >= 1
    })
    .await;
    rig.send(BackendEvent::CastsChanged(vec![cast(1, true)]));
    rig.until("the marker cast", |state| {
        state.privacy.as_ref().is_some_and(|privacy| privacy.active)
    })
    .await;

    assert_eq!(rig.calls(), ["switch_keyboard_layout 1"]);
}

fn layouts_of(focus_id: u64, names: &[&str], current: Option<u32>) -> Mirrored {
    Mirrored {
        windows: vec![backend_window(1, 1), backend_window(2, 1)],
        focus: Focus {
            output: Some("DP-1".to_owned()),
            window: Some(WindowId(focus_id)),
        },
        keyboard: served(names, &[], current),
        ..Mirrored::default()
    }
}

struct Held {
    service: Keyboard,
    ctx: Ctx<Keyboard>,
    events: mpsc::Receiver<Input<Keyboard>>,
    rig: Rig,
}

impl Held {
    async fn start(remember: Remember) -> Self {
        let rig = Rig::start(two_layouts()).await;
        let cancel = CancellationToken::new();
        let (events, inbox) = mpsc::channel(8);
        let (published, _state) = watch::channel(KeyboardLayouts::default());
        let (health, _health) = watch::channel(ServiceState::Starting);
        let ctx = Ctx::<Keyboard>::new(
            events,
            &cancel,
            published,
            health,
            Buses::unavailable("no bus in tests"),
        );
        let mut document = glimpse_config::Config::default();
        document.keyboard.remember = remember;
        let service = Keyboard::start(
            &ctx,
            Config::from(&document),
            Dependencies {
                provider: ProviderHandle(rig.mirror.provider.handle()),
            },
        )
        .await;
        Self {
            service,
            ctx,
            events: inbox,
            rig,
        }
    }

    async fn follow(&mut self, state: Mirrored) {
        self.service
            .handle(&self.ctx, Input::Event(Event::Provider(Box::new(state))))
            .await;
    }

    async fn completed(&mut self) {
        let event = tokio::time::timeout(Duration::from_secs(2), self.events.recv())
            .await
            .expect("the layout command finishes")
            .expect("an open event inbox");
        self.service.handle(&self.ctx, event).await;
    }
}

async fn switched(
    service: &mut Keyboard,
    ctx: &Ctx<Keyboard>,
    target: LayoutRef,
) -> Result<(), CommandError> {
    let (reply, result) = oneshot::channel();
    service.switch(ctx, target, reply);
    result.await.expect("a reply")
}

#[tokio::test]
async fn a_remembered_index_past_the_layouts_is_dropped() {
    let mut held = Held::start(Remember::Window).await;
    let wide = ["English (US)", "Polish", "German"];
    held.follow(layouts_of(1, &wide, Some(0))).await;
    held.follow(layouts_of(1, &wide, Some(2))).await;
    held.follow(layouts_of(2, &["English (US)"], Some(0))).await;
    held.follow(layouts_of(1, &["English (US)"], Some(0))).await;

    assert_eq!(held.service.store.get(&StoreKey::Window(1)), None);
    let mut recorded = held.rig.served.shared.recorded();
    let late = tokio::time::timeout(
        Duration::from_millis(500),
        recorded.wait_for(|count| *count >= 1),
    )
    .await;
    assert!(
        late.is_err(),
        "a stored index past the list must send nothing"
    );
}

#[tokio::test]
async fn a_switch_is_remembered_for_the_window_that_had_focus() {
    for (target, call) in [
        (LayoutRef::Index { index: 1 }, "switch_keyboard_layout 1"),
        (LayoutRef::Next, "step_keyboard_layout true"),
        (LayoutRef::Prev, "step_keyboard_layout false"),
    ] {
        let mut held = Held::start(Remember::Window).await;
        let names = ["English (US)", "Polish"];
        held.follow(layouts_of(1, &names, Some(0))).await;

        let result = switched(&mut held.service, &held.ctx, target).await;
        assert_eq!(result, Ok(()));
        held.follow(layouts_of(2, &names, Some(0))).await;
        let switched_for = tokio::time::timeout(Duration::from_secs(2), held.events.recv())
            .await
            .expect("the switch is remembered")
            .expect("the inbox stays open");
        held.service.handle(&held.ctx, switched_for).await;

        assert_eq!(
            held.service.store.get(&StoreKey::Window(1)).copied(),
            Some(1)
        );

        held.follow(layouts_of(2, &names, Some(1))).await;
        assert_eq!(held.service.store.get(&StoreKey::Window(2)), Some(&0));
        held.follow(layouts_of(2, &names, Some(0))).await;
        held.completed().await;
        held.follow(layouts_of(1, &names, Some(0))).await;
        until(
            &mut held.rig.served.shared.recorded(),
            "the restore",
            |count| *count >= 2,
        )
        .await;

        assert_eq!(held.rig.calls(), [call, "switch_keyboard_layout 1"]);
    }
}

#[tokio::test]
async fn a_failed_step_does_not_remember_a_later_external_switch() {
    let mut held = Held::start(Remember::Window).await;
    let names = ["English (US)", "Polish"];
    held.follow(layouts_of(1, &names, Some(1))).await;
    held.follow(layouts_of(1, &names, Some(0))).await;
    let mut provider = held.rig.mirror.provider.handle().subscribe();
    held.rig.served.task.abort();
    until(&mut provider, "the provider loss", |state| {
        state.unavailable.is_some()
    })
    .await;

    for target in [LayoutRef::Next, LayoutRef::Prev] {
        assert!(matches!(
            switched(&mut held.service, &held.ctx, target).await,
            Err(CommandError::Unavailable(_))
        ));
        held.completed().await;
    }
    assert!(held.events.try_recv().is_err());
    held.follow(layouts_of(2, &names, Some(0))).await;
    held.follow(layouts_of(2, &names, Some(1))).await;
    assert_eq!(held.service.store.get(&StoreKey::Window(1)), Some(&0));
    assert_eq!(held.service.store.get(&StoreKey::Window(2)), Some(&1));
}

#[tokio::test]
async fn a_window_keeps_its_initial_layout_after_another_windows_shortcut() {
    let mut held = Held::start(Remember::Window).await;
    let names = ["English (US)", "Polish"];
    held.follow(layouts_of(1, &names, Some(0))).await;
    held.follow(layouts_of(2, &names, Some(0))).await;
    held.follow(layouts_of(2, &names, Some(1))).await;
    held.follow(layouts_of(1, &names, Some(1))).await;

    until(
        &mut held.rig.served.shared.recorded(),
        "window 1 restores its initial layout",
        |count| *count >= 1,
    )
    .await;
    assert_eq!(held.rig.calls(), ["switch_keyboard_layout 0"]);
}

#[tokio::test]
async fn a_coalesced_shortcut_and_focus_do_not_replace_the_incoming_windows_layout() {
    let mut held = Held::start(Remember::Window).await;
    let names = ["English (US)", "Polish"];
    held.service.store.insert(StoreKey::Window(1), 0);
    held.service.store.insert(StoreKey::Window(2), 0);
    held.follow(layouts_of(1, &names, Some(0))).await;
    held.follow(layouts_of(2, &names, Some(1))).await;

    assert_eq!(held.service.store.get(&StoreKey::Window(2)), Some(&0));
}

#[tokio::test]
async fn a_late_restore_does_not_replace_the_newly_focused_windows_layout() {
    let mut held = Held::start(Remember::Window).await;
    let names = ["English (US)", "Polish"];
    held.service.store.insert(StoreKey::Window(1), 1);
    held.service.store.insert(StoreKey::Window(2), 0);
    held.follow(layouts_of(1, &names, Some(1))).await;
    held.follow(layouts_of(2, &names, Some(1))).await;
    until(
        &mut held.rig.served.shared.recorded(),
        "window 2 starts restoring layout 0",
        |count| *count >= 1,
    )
    .await;
    held.follow(layouts_of(1, &names, Some(1))).await;
    held.follow(layouts_of(1, &names, Some(0))).await;

    assert_eq!(held.service.store.get(&StoreKey::Window(1)), Some(&1));
    held.completed().await;
    until(
        &mut held.rig.served.shared.recorded(),
        "window 1 restores layout 1 after the late restore",
        |count| *count >= 2,
    )
    .await;
    assert_eq!(
        held.rig.calls(),
        ["switch_keyboard_layout 0", "switch_keyboard_layout 1"]
    );
}

#[tokio::test]
async fn a_noop_restore_finishes_when_the_shortcut_event_precedes_its_reply() {
    let mut held = Held::start(Remember::Window).await;
    let names = ["English (US)", "Polish"];
    held.service.store.insert(StoreKey::Window(1), 1);
    held.service.store.insert(StoreKey::Window(2), 0);
    held.follow(layouts_of(1, &names, Some(1))).await;
    held.follow(layouts_of(2, &names, Some(1))).await;
    held.follow(layouts_of(2, &names, Some(0))).await;
    held.completed().await;

    assert!(held.service.operations.is_empty());
    assert_eq!(held.service.store.get(&StoreKey::Window(2)), Some(&0));
    assert_eq!(held.rig.calls(), ["switch_keyboard_layout 0"]);
}

#[tokio::test]
async fn a_failed_restore_does_not_suppress_the_next_shortcut() {
    let mut held = Held::start(Remember::Window).await;
    let names = ["English (US)", "Polish"];
    held.follow(layouts_of(1, &names, Some(1))).await;
    held.service.store.insert(StoreKey::Window(2), 0);
    let mut provider = held.rig.mirror.provider.handle().subscribe();
    held.rig.served.task.abort();
    until(&mut provider, "the provider loss", |state| {
        state.unavailable.is_some()
    })
    .await;

    held.follow(layouts_of(2, &names, Some(1))).await;
    held.completed().await;
    assert!(held.service.operations.is_empty());
    held.follow(layouts_of(2, &names, Some(0))).await;
    held.follow(layouts_of(2, &names, Some(1))).await;
    assert_eq!(held.service.store.get(&StoreKey::Window(2)), Some(&1));
}

#[tokio::test]
async fn a_restore_finishes_when_the_native_target_is_superseded_before_observation() {
    let mut held = Held::start(Remember::Window).await;
    let names = ["English (US)", "Polish"];
    held.follow(layouts_of(1, &names, Some(0))).await;
    held.service.store.insert(StoreKey::Window(2), 1);
    held.follow(layouts_of(2, &names, Some(0))).await;
    held.follow(layouts_of(2, &names, Some(0))).await;
    held.completed().await;

    assert!(held.service.operations.is_empty());
    held.follow(layouts_of(1, &names, Some(0))).await;
    held.follow(layouts_of(2, &names, Some(0))).await;
    assert!(held.service.operations.is_empty());
    assert_eq!(held.service.store.get(&StoreKey::Window(2)), Some(&0));
    assert_eq!(held.rig.calls(), ["switch_keyboard_layout 1"]);
}
