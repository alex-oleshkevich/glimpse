use chrono::Utc;

use super::*;

fn titled(id: u64, title: &str) -> Window {
    let mut window = window(id, 4);
    window.title = Some(title.to_owned());
    window
}

async fn seeded(harness: &mut Harness, windows: Vec<Window>) {
    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Snapshot(Box::new(snapshot_of(
                vec![workspace(4, Some(1), "DP-2")],
                windows,
            )))),
        )
        .await;
    harness.state.borrow_and_update();
}

fn published_title(harness: &Harness, id: u64) -> Option<String> {
    harness
        .state
        .borrow()
        .windows
        .as_ref()
        .and_then(|windows| windows.windows.iter().find(|window| window.id == id))
        .and_then(|window| window.title.clone())
}

fn held(service: &Compositor) -> Option<chrono::DateTime<Utc>> {
    service.titles_at
}

#[tokio::test]
async fn a_spinning_title_is_held_and_published_once_when_the_deadline_arrives() {
    let mut harness = harness(Backend::Unsupported);
    seeded(&mut harness, vec![titled(1, "a terminal")]).await;

    let mut spinning = titled(1, "a terminal");
    for frame in ["⠋", "⠙", "⠹"] {
        spinning.title = Some(format!("{frame} a terminal"));
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Changed(Change::WindowOpenedOrChanged(
                    spinning.clone(),
                ))),
            )
            .await;
    }

    let due = held(&harness.service).expect("the first title starts one hold");
    assert!(
        (due - (Utc::now() + TITLE_HOLD)).num_milliseconds().abs() < 50,
        "the hold is one second, not restarted by the frames that follow"
    );
    assert!(
        !harness.state.has_changed().unwrap_or(true),
        "a title-only change must not wake the bar"
    );
    assert_eq!(published_title(&harness, 1).as_deref(), Some("a terminal"));

    harness
        .service
        .handle(&harness.ctx, Input::Event(Event::TitlesDue(due)))
        .await;

    assert_eq!(
        published_title(&harness, 1).as_deref(),
        Some("⠹ a terminal")
    );
    assert_eq!(held(&harness.service), None);
}

#[tokio::test]
async fn a_stale_title_deadline_does_not_cancel_the_hold_that_replaced_it() {
    let mut harness = harness(Backend::Unsupported);
    seeded(&mut harness, vec![titled(1, "a terminal")]).await;

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Changed(Change::WindowOpenedOrChanged(titled(
                1,
                "⠋ a terminal",
            )))),
        )
        .await;
    let first = held(&harness.service).expect("first hold");

    let mut focused = titled(1, "⠋ a terminal");
    focused.is_focused = true;
    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Changed(Change::WindowOpenedOrChanged(focused))),
        )
        .await;
    assert_eq!(
        held(&harness.service),
        None,
        "a visible change publishes immediately and drops the hold"
    );
    assert_eq!(
        published_title(&harness, 1).as_deref(),
        Some("⠋ a terminal")
    );

    let mut spinning = titled(1, "⠙ a terminal");
    spinning.is_focused = true;
    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Changed(Change::WindowOpenedOrChanged(spinning))),
        )
        .await;
    let second = held(&harness.service).expect("the next title starts a new hold");
    harness.state.borrow_and_update();

    harness
        .service
        .handle(&harness.ctx, Input::Event(Event::TitlesDue(first)))
        .await;

    assert_eq!(held(&harness.service), Some(second));
    assert!(
        !harness.state.has_changed().unwrap_or(true),
        "a deadline from the hold a focus change replaced must not publish early"
    );
    assert_eq!(
        published_title(&harness, 1).as_deref(),
        Some("⠋ a terminal")
    );
}

#[tokio::test]
async fn a_repeated_window_event_and_a_geometry_only_change_publish_nothing() {
    let mut harness = harness(Backend::Unsupported);
    let mut open = titled(1, "a terminal");
    open.size = Some((800, 600));
    seeded(&mut harness, vec![open.clone()]).await;

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Changed(Change::WindowOpenedOrChanged(open.clone()))),
        )
        .await;
    open.size = Some((1000, 700));
    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Changed(Change::WindowOpenedOrChanged(open))),
        )
        .await;

    assert_eq!(held(&harness.service), None);
    assert!(
        !harness.state.has_changed().unwrap_or(true),
        "size and rect are not published, so they must not wake the bar"
    );
}

#[tokio::test]
async fn a_window_that_opens_still_publishes_immediately() {
    let mut harness = harness(Backend::Unsupported);
    seeded(&mut harness, Vec::new()).await;

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Changed(Change::WindowOpenedOrChanged(titled(
                9,
                "a terminal",
            )))),
        )
        .await;

    assert_eq!(held(&harness.service), None);
    assert_eq!(published_title(&harness, 9).as_deref(), Some("a terminal"));
}
