use super::*;

#[test]
fn each_edge_has_the_expected_anchors_and_vertical_motion() {
    for (edge, expected, class) in [
        (
            NotificationEdge::TopLeft,
            &[Edge::Top, Edge::Left][..],
            "notification-popup__entry--top",
        ),
        (
            NotificationEdge::TopCenter,
            &[Edge::Top][..],
            "notification-popup__entry--top",
        ),
        (
            NotificationEdge::TopRight,
            &[Edge::Top, Edge::Right][..],
            "notification-popup__entry--top",
        ),
        (
            NotificationEdge::BottomLeft,
            &[Edge::Bottom, Edge::Left][..],
            "notification-popup__entry--bottom",
        ),
        (
            NotificationEdge::BottomCenter,
            &[Edge::Bottom][..],
            "notification-popup__entry--bottom",
        ),
        (
            NotificationEdge::BottomRight,
            &[Edge::Bottom, Edge::Right][..],
            "notification-popup__entry--bottom",
        ),
    ] {
        assert_eq!(anchors(edge), expected);
        assert_eq!(animation_class(edge), class);
    }
}

#[test]
fn height_overflow_hides_only_the_oldest_and_keeps_one_card() {
    assert_eq!(height_overflow(&[3, 2, 1], 901, 900), Some(1));
    assert_eq!(height_overflow(&[3, 2, 1], 900, 900), None);
    assert_eq!(height_overflow(&[1], 901, 900), None);
}

#[test]
fn input_region_encloses_fractional_card_bounds() {
    let rectangle = enclosing_rectangle(gtk4::graphene::Rect::new(0.75, 1.25, 100.75, 50.5));
    assert_eq!(
        (
            rectangle.x(),
            rectangle.y(),
            rectangle.width(),
            rectangle.height(),
        ),
        (0, 1, 102, 51)
    );
}

#[test]
fn appearing_and_visible_cards_accept_input() {
    assert!(EntryState::Entering.accepts_input());
    assert!(EntryState::Visible.accepts_input());
    assert!(!EntryState::Leaving.accepts_input());
}

#[test]
fn an_unrepresentable_deadline_expires_safely() {
    let now = Instant::now();
    assert_eq!(deadline(now, Duration::MAX), now);
}

#[test]
fn countdowns_pause_independently_and_replacements_preserve_hover() {
    let start = Instant::now();
    let mut first = Countdown::running(Duration::from_secs(4), start);
    let mut second = Countdown::running(Duration::from_secs(4), start);

    first.pause(start + Duration::from_secs(1));
    second.pause(start + Duration::from_secs(2));
    assert_eq!(first.remaining, Duration::from_secs(3));
    assert_eq!(second.remaining, Duration::from_secs(2));

    assert!(!first.restart(Duration::from_secs(8), start));
    assert_eq!(
        first.resume(start + Duration::from_secs(5)),
        Some(Duration::from_secs(8))
    );
    assert!(first.restart(Duration::from_secs(6), start));
    assert_eq!(first.deadline, Some(start + Duration::from_secs(6)));
    assert!(!first.expired(start + Duration::from_secs(4)));
    assert!(first.expired(start + Duration::from_secs(6)));
    assert_eq!(second.deadline, None);
    assert!(!second.expired(start + Duration::from_secs(20)));
}

#[test]
fn a_popup_lives_as_long_as_its_sender_asked() {
    let record = NotificationRecord {
        id: 1,
        app_id: "app".to_owned(),
        app_name: "App".to_owned(),
        app_pid: None,
        summary: "Summary".to_owned(),
        body: None,
        icon: None,
        image: None,
        urgency: NotificationUrgency::Normal,
        actions: Vec::new(),
        progress: None,
        created: chrono::Utc::now(),
        unread: true,
        resident: false,
        expire_timeout: -1,
    };
    let delay = Duration::from_secs(4);
    assert_eq!(lifetime(&record, delay), Some(delay));
    assert_eq!(
        lifetime(
            &NotificationRecord {
                expire_timeout: 1500,
                ..record.clone()
            },
            delay
        ),
        Some(Duration::from_millis(1500))
    );
    assert_eq!(
        lifetime(
            &NotificationRecord {
                resident: true,
                ..record.clone()
            },
            delay
        ),
        Some(delay)
    );
    for sticky in [
        NotificationRecord {
            expire_timeout: 0,
            ..record.clone()
        },
        NotificationRecord {
            urgency: NotificationUrgency::Critical,
            expire_timeout: 1500,
            ..record
        },
    ] {
        assert_eq!(lifetime(&sticky, delay), None);
    }
}
