use std::time::Duration;

use glimpse_dbus::testing::tray::{FakeItem, Shape};
use zbus::Connection;

#[tokio::main(flavor = "current_thread")]
async fn main() -> zbus::Result<()> {
    let ayatana = FakeItem::start(Connection::session().await?, Shape::Ayatana, "fake-ayatana")
        .await
        .inspect_err(|error| eprintln!("the Ayatana fake did not start: {error}"))?;
    let pixmap = FakeItem::start(Connection::session().await?, Shape::Pixmap, "fake-pixmap")
        .await
        .inspect_err(|error| eprintln!("the pixmap fake did not start: {error}"))?;

    for item in [&ayatana, &pixmap] {
        // A well-known name is what lets a host's sweep find this item after the host restarts, and
        // re-registering on the announcement is what a Qt or libayatana client does. Without both, a
        // panel restart loses the item until the application itself restarts — which is the residue
        // case the README documents, and is no way to run a dev helper.
        match item.claim_well_known_name().await {
            Ok(name) => println!("holding {name}"),
            Err(error) => println!("no well-known name: {error}"),
        }
        item.re_register_on_host_announcement().await?;
        match item.register().await {
            Ok(()) => println!("registered {}", item.key()),
            Err(error) => println!("no watcher took {}: {error}", item.key()),
        }
    }

    println!("cycling status every 4s; Ctrl-C to stop");
    let mut states = ["Active", "NeedsAttention", "Passive"].into_iter().cycle();
    loop {
        tokio::time::sleep(Duration::from_secs(4)).await;
        let next = states.next().unwrap_or("Active");
        ayatana.set_status(next).await?;
        ayatana
            .set_label(if next == "NeedsAttention" { "3" } else { "" })
            .await?;
        pixmap.set_status(next).await?;
        println!("status = {next}");
    }
}
