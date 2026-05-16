//! Subscribe to shell events and dispatch an action.
//!
//! Run against a live Glimpse session:
//!   cargo run --example ipc

use glimpse_sdk::ipc;

#[tokio::main]
async fn main() -> glimpse_sdk::AppletResult<()> {
    // Cheap: resolves the socket path, no connection yet.
    let sub = ipc("shell")?;

    // One-shot connection; awaits the ack. Errors if the server replies
    // ok=false.
    let ack = sub
        .dispatch("open_uri", [("uri", "https://example.com")])
        .await?;
    println!("dispatch ack: {ack:?}");

    // Long-lived connection; yields events until the socket closes.
    let mut events = sub.listen("audio.*").await?;
    while let Some(ev) = events.next().await {
        let ev = ev?;
        println!("{} ts={} {:?}", ev.name, ev.ts, ev.fields);
    }
    Ok(())
}
