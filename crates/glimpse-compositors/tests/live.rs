//! Run against the compositor this session is actually under: `just test-compositor`.
//!
//! Ignored by default because they need a real socket. The headless suite covers the parsing and
//! the command shapes; what only a live compositor can prove is that the requests we send are ones
//! it still accepts.

use glimpse_compositors::{Compositor, Event, LayoutTarget, detect_compositor};

use futures_util::StreamExt;

#[tokio::test]
#[ignore = "needs a running niri or Hyprland"]
async fn a_live_snapshot_describes_the_session() {
    let compositor = detect_compositor();
    assert!(
        !matches!(compositor, Compositor::Unsupported),
        "no supported compositor detected; \
         NIRI_SOCKET and HYPRLAND_INSTANCE_SIGNATURE are both unset"
    );

    let snapshot = compositor.snapshot().await.expect("a snapshot");

    assert!(
        snapshot.outputs.iter().any(|output| output.enabled),
        "a session with no enabled output: {:?}",
        snapshot.outputs
    );
    assert!(!snapshot.workspaces.is_empty(), "no workspaces");
    assert_eq!(
        snapshot.keyboard.names.len(),
        snapshot.keyboard.codes.len(),
        "names and codes must stay parallel: {:?}",
        snapshot.keyboard
    );
    assert!(
        snapshot
            .keyboard
            .current
            .is_none_or(|current| current < snapshot.keyboard.names.len()),
        "the current layout is out of range: {:?}",
        snapshot.keyboard
    );
}

/// The round trip the headless tests cannot make: a command we send comes back as an event the
/// compositor chose to emit. Restores the layout it started on.
#[tokio::test]
#[ignore = "needs a running niri or Hyprland"]
async fn switching_the_layout_is_reported_back_on_the_event_stream() {
    let compositor = detect_compositor();
    assert!(!matches!(compositor, Compositor::Unsupported));

    let before = compositor.snapshot().await.expect("a snapshot").keyboard;
    if before.names.len() < 2 {
        eprintln!("skipped: this session has fewer than two keyboard layouts configured");
        return;
    }

    let mut events = compositor.events().await.expect("subscribed");
    compositor
        .switch_keyboard_layout(LayoutTarget::Next)
        .await
        .expect("switched");

    let switched = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(event) = events.next().await {
            if let Event::KeyboardLayoutSwitched { idx, .. } = event {
                return Some(idx);
            }
        }
        None
    })
    .await
    .expect("the compositor reported the switch within five seconds");

    assert!(switched.is_some_and(|idx| idx < before.names.len()));

    if let Some(original) = before.current {
        let original = u8::try_from(original).expect("a layout index fits a u8");
        compositor
            .switch_keyboard_layout(LayoutTarget::Index(original))
            .await
            .expect("restored");
    }
}
