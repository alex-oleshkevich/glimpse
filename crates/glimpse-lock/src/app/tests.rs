use super::{focused_output, reports};
use glimpse_services::{CompositorOutputs, CompositorState, OutputInfo};
use std::cell::RefCell;
use tokio::sync::watch;

fn output(connector: &str, focused: bool) -> OutputInfo {
    OutputInfo {
        connector: connector.to_owned(),
        label: None,
        built_in: false,
        focused,
        make: None,
        model: None,
        current_mode: None,
        logical: None,
        enabled: true,
    }
}

#[test]
fn the_focused_output_comes_from_the_compositor_outputs() {
    assert_eq!(
        focused_output(&CompositorState::default()),
        None,
        "no outputs reported yet says nothing, rather than clearing focus"
    );
    let state = CompositorState {
        outputs: Some(CompositorOutputs {
            outputs: vec![output("eDP-1", false), output("DP-2", true)],
        }),
        ..Default::default()
    };
    assert_eq!(focused_output(&state), Some(Some("DP-2".to_owned())));
    let unfocused = CompositorState {
        outputs: Some(CompositorOutputs {
            outputs: vec![output("eDP-1", false)],
        }),
        ..Default::default()
    };
    assert_eq!(focused_output(&unfocused), Some(None));
}

#[tokio::test]
async fn reports_skip_the_placeholder_and_keep_a_value_published_before_subscribing() {
    let (publisher, held) = watch::channel(0);
    let seen = RefCell::new(Vec::new());
    let early = held.clone();
    publisher.send_replace(1);
    let late = held.clone();
    drop(held);
    drop(publisher);
    reports(early, |value| seen.borrow_mut().push(value)).await;
    reports(late, |value| seen.borrow_mut().push(value)).await;
    assert_eq!(
        seen.into_inner(),
        vec![1, 1],
        "a receiver cloned from the never-read original sees a publish from either side"
    );

    let (publisher, held) = watch::channel(0);
    let seen = RefCell::new(Vec::new());
    drop(publisher);
    reports(held, |value| seen.borrow_mut().push(value)).await;
    assert!(
        seen.into_inner().is_empty(),
        "nothing published means nothing reported, so the network slot stays hidden"
    );
}
