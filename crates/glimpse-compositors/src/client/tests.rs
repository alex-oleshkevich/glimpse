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
