use std::time::Duration;

use futures_util::StreamExt as _;

use glimpse_dbus::own_name;
use glimpse_dbus::testing::PrivateBus;
use zbus::fdo::{DBusProxy, RequestNameFlags};
use zbus::names::{BusName, WellKnownName};

const NAME: &str = "me.aresa.Glimpse.NameOwnershipTest";

async fn owner_of(dbus: &DBusProxy<'_>) -> String {
    dbus.get_name_owner(BusName::WellKnown(WellKnownName::try_from(NAME).unwrap()))
        .await
        .unwrap()
        .to_string()
}

async fn wait_until_gone(dbus: &DBusProxy<'_>, unique: &str) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let names = dbus.list_names().await.unwrap();
            if !names.iter().any(|name| name.as_str() == unique) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[expect(clippy::disallowed_methods, reason = "the flags under test")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_duplicate_provider_is_refused_and_the_first_owner_keeps_the_name() {
    let bus = PrivateBus::start();
    let client = bus.connection().await;
    let dbus = DBusProxy::new(&client).await.unwrap();

    let first = bus.connection().await;
    own_name(&first, NAME).await.unwrap();
    let held = first.unique_name().unwrap().to_string();
    assert_eq!(owner_of(&dbus).await, held);

    own_name(&first, NAME).await.unwrap();

    let mut changes = dbus
        .receive_name_owner_changed_with_args(&[(0, NAME)])
        .await
        .unwrap();

    let second = bus.connection().await;
    let intruder = second.unique_name().unwrap().to_string();
    assert!(matches!(
        own_name(&second, NAME).await,
        Err(zbus::Error::NameTaken)
    ));
    assert_eq!(owner_of(&dbus).await, held);

    drop(second);
    wait_until_gone(&dbus, &intruder).await;
    assert_eq!(owner_of(&dbus).await, held);

    let older = bus.connection().await;
    let usurper = older.unique_name().unwrap().to_string();
    assert!(matches!(
        older
            .request_name_with_flags(
                NAME,
                RequestNameFlags::AllowReplacement
                    | RequestNameFlags::ReplaceExisting
                    | RequestNameFlags::DoNotQueue,
            )
            .await,
        Err(zbus::Error::NameTaken)
    ));
    assert_eq!(owner_of(&dbus).await, held);

    drop(older);
    wait_until_gone(&dbus, &usurper).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(250), changes.next())
            .await
            .is_err()
    );
}
