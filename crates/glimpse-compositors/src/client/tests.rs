use std::collections::HashMap;
use std::time::Duration;

use glimpse_dbus::testing::PrivateBus;
use zbus::zvariant::Value;

use super::*;
use crate::contract::OBJECT_PATH;

struct Squatter {
    name: String,
    capabilities: Vec<String>,
    title: String,
}

fn owned(value: impl Into<Value<'static>>) -> OwnedValue {
    OwnedValue::try_from(value.into()).expect("an owned value")
}

#[zbus::interface(name = "me.aresa.Glimpse.Compositor1")]
impl Squatter {
    #[zbus(property)]
    fn name(&self) -> String {
        self.name.clone()
    }

    #[zbus(property)]
    fn capabilities(&self) -> Vec<String> {
        self.capabilities.clone()
    }

    #[zbus(property)]
    fn windows(&self) -> Vec<Item> {
        vec![Item::from([
            (key::ID.to_owned(), owned(1_u64)),
            (key::TITLE.to_owned(), owned(self.title.clone())),
        ])]
    }
}

async fn squat(
    bus: &PrivateBus,
    name: &str,
    capabilities: &[&str],
    title: &str,
) -> zbus::Connection {
    let owner = bus.connection().await;
    let squatter = Squatter {
        name: name.to_owned(),
        capabilities: capabilities.iter().map(|it| (*it).to_owned()).collect(),
        title: title.to_owned(),
    };
    owner
        .object_server()
        .at(OBJECT_PATH, squatter)
        .await
        .expect("an exported object");
    glimpse_dbus::own_name(&owner, BUS_NAME)
        .await
        .expect("the bus name");
    owner
}

async fn invalidate(connection: &zbus::Connection, property: &str) {
    connection
        .emit_signal(
            None::<&str>,
            OBJECT_PATH,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(
                "me.aresa.Glimpse.Compositor1",
                HashMap::<String, OwnedValue>::new(),
                vec![property],
            ),
        )
        .await
        .expect("an invalidation");
}

async fn followed(state: &mut watch::Receiver<CompositorState>, name: &str) -> CompositorState {
    tokio::time::timeout(
        Duration::from_secs(10),
        state.wait_for(|state| state.name == name),
    )
    .await
    .expect("the provider in time")
    .expect("a live follower")
    .clone()
}

#[tokio::test]
async fn a_hostile_title_is_capped_and_cleaned_by_the_client() {
    let bus = PrivateBus::start();
    let hostile = format!(
        "\u{202e}\u{2066}\u{7}\n{}",
        "x\u{202e}\u{2066}\u{1b}".repeat(1 << 18)
    );
    let _owner = squat(&bus, "squatter", &[], &hostile).await;
    let provider = CompositorProvider::start(bus.connection().await);
    let handle = Handle(provider.handle());

    let state = followed(&mut handle.subscribe(), "squatter").await;

    let title = state.windows[0].title.clone().expect("a title");
    assert_eq!(title, "x".repeat(512));
}

#[tokio::test]
async fn a_new_owner_brings_its_own_name_and_capabilities() {
    let bus = PrivateBus::start();
    let first = squat(&bus, "first", &["output-power"], "").await;
    let provider = CompositorProvider::start(bus.connection().await);
    let handle = Handle(provider.handle());
    let mut state = handle.subscribe();

    let seen = followed(&mut state, "first").await;
    assert_eq!(seen.capabilities, [Capability::OutputPower]);
    assert!(handle.supports(Capability::OutputPower));

    first.close().await.expect("the first owner leaves");
    let _second = squat(&bus, "second", &["window-geometry", "frobnicate"], "").await;

    let seen = followed(&mut state, "second").await;
    assert_eq!(seen.capabilities, [Capability::WindowGeometry]);
    assert!(!handle.supports(Capability::OutputPower));
}

#[tokio::test]
async fn an_invalidated_window_list_is_read_again() {
    let bus = PrivateBus::start();
    let owner = squat(&bus, "squatter", &[], "kept").await;
    let provider = CompositorProvider::start(bus.connection().await);
    let handle = Handle(provider.handle());
    let mut state = handle.subscribe();
    let seen = followed(&mut state, "squatter").await;
    assert_eq!(seen.windows[0].title.as_deref(), Some("kept"));

    let interface = owner
        .object_server()
        .interface::<_, Squatter>(OBJECT_PATH)
        .await
        .expect("an interface");
    interface.get_mut().await.title = "after".to_owned();
    invalidate(&owner, "Windows").await;

    let seen = tokio::time::timeout(
        Duration::from_secs(2),
        state.wait_for(|state| {
            state
                .windows
                .first()
                .and_then(|window| window.title.as_deref())
                == Some("after")
        }),
    )
    .await
    .expect("the invalidated list is read again")
    .expect("a live follower");
    assert_eq!(seen.windows.len(), 1);
}

async fn keyboard_change(owner: &zbus::Connection, name: &str, item: Item) {
    owner
        .emit_signal(
            None::<&str>,
            OBJECT_PATH,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(
                "me.aresa.Glimpse.Compositor1",
                HashMap::from([(name.to_owned(), owned(item))]),
                Vec::<String>::new(),
            ),
        )
        .await
        .expect("a keyboard or focus change");
}

#[tokio::test]
async fn keyboard_updates_keep_shortcuts_and_focus_in_wire_order() {
    let bus = PrivateBus::start();
    let owner = squat(&bus, "keyboard", &[], "").await;
    let provider = CompositorProvider::start(bus.connection().await);
    let handle = Handle(provider.handle());
    let (_barrier, mut updates) = handle.keyboard_updates();
    tokio::time::timeout(Duration::from_secs(2), updates.next())
        .await
        .expect("the initial keyboard snapshot")
        .expect("an open stream");

    for (name, field, value) in [
        ("Focus", key::WINDOW, owned(1_u64)),
        ("Keyboard", key::CURRENT, owned(0_u32)),
        ("Keyboard", key::CURRENT, owned(1_u32)),
        ("Focus", key::WINDOW, owned(2_u64)),
        ("Keyboard", key::CURRENT, owned(0_u32)),
    ] {
        keyboard_change(&owner, name, Item::from([(field.to_owned(), value)])).await;
    }

    let mut observed = Vec::new();
    for _ in 0..5 {
        let update = tokio::time::timeout(Duration::from_secs(2), updates.next())
            .await
            .expect("every ordered keyboard update")
            .expect("an open stream");
        let KeyboardUpdate::State(state) = update else {
            panic!("an ordered state update");
        };
        observed.push((state.focus.window, state.keyboard.current));
    }
    assert_eq!(
        observed,
        [
            (Some(WindowId(1)), None),
            (Some(WindowId(1)), Some(0)),
            (Some(WindowId(1)), Some(1)),
            (Some(WindowId(2)), Some(1)),
            (Some(WindowId(2)), Some(0)),
        ]
    );
}

#[tokio::test]
async fn a_keyboard_barrier_follows_every_preceding_property_payload() {
    let bus = PrivateBus::start();
    let owner = squat(&bus, "keyboard", &[], "").await;
    let provider = CompositorProvider::start(bus.connection().await);
    let handle = Handle(provider.handle());
    let (barrier, mut updates) = handle.keyboard_updates();
    tokio::time::timeout(Duration::from_secs(2), updates.next())
        .await
        .expect("the initial keyboard snapshot")
        .expect("an open stream");

    for current in [1_u32, 0, 1, 0] {
        keyboard_change(
            &owner,
            "Keyboard",
            Item::from([(key::CURRENT.to_owned(), owned(current))]),
        )
        .await;
    }
    barrier.request(7).await.expect("a completion barrier");
    let mut observed = Vec::new();
    loop {
        let update = tokio::time::timeout(Duration::from_secs(2), updates.next())
            .await
            .expect("the ordered completion")
            .expect("an open stream");
        match update {
            KeyboardUpdate::State(state) => observed.push(state.keyboard.current),
            KeyboardUpdate::Barrier(id) => {
                assert_eq!(id, 7);
                break;
            }
        }
    }
    assert_eq!(observed, [Some(1), Some(0), Some(1), Some(0), Some(0)]);

    barrier
        .request(8)
        .await
        .expect("a no-op completion barrier");
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), updates.next())
            .await
            .expect("the no-op readback"),
        Some(KeyboardUpdate::State(_))
    ));
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), updates.next())
            .await
            .expect("completion without a new signal"),
        Some(KeyboardUpdate::Barrier(8))
    ));
}

#[tokio::test]
async fn a_delayed_keyboard_consumer_cannot_stall_the_shared_bus() {
    let bus = PrivateBus::start();
    let owner = squat(&bus, "keyboard", &[], "").await;
    let provider = CompositorProvider::start(bus.connection().await);
    let handle = Handle(provider.handle());
    let (_barrier, mut updates) = handle.keyboard_updates();
    tokio::time::timeout(Duration::from_secs(2), updates.next())
        .await
        .expect("the initial keyboard snapshot")
        .expect("an open stream");

    for current in 0..256_u32 {
        keyboard_change(
            &owner,
            "Keyboard",
            Item::from([(key::CURRENT.to_owned(), owned(current))]),
        )
        .await;
    }
    let proxy = handle.0.proxy().await.expect("the provider proxy");
    tokio::time::timeout(
        Duration::from_secs(2),
        proxy.inner().connection().call_method(
            Some(BUS_NAME),
            OBJECT_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "GetAll",
            &("me.aresa.Glimpse.Compositor1",),
        ),
    )
    .await
    .expect("the shared connection still receives method replies")
    .expect("a property snapshot");
    let update = tokio::time::timeout(Duration::from_secs(2), updates.next())
        .await
        .expect("the lost chronology is detected")
        .expect("an open stream");
    assert!(matches!(update, KeyboardUpdate::State(state) if state.unavailable.is_some()));
}
