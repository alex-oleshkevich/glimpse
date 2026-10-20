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
#[ignore = "needs a display"]
fn late_artwork_rebuilds_the_input_region_after_the_card_settles() {
    adw::init().expect("display");
    glimpse_widgets::register_resources().expect("resources");

    let scratch = glib::mkdtemp(std::env::temp_dir().join("glimpse-notification-image-XXXXXX"))
        .expect("a scratch directory");
    let fifo = scratch.join("fifo.png");
    std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs");

    let card = NotificationCard::new();
    let (sender, receiver) = relm4::channel();
    let _ = App::fill(
        &card,
        &NotificationRecord {
            image: Some(fifo.to_string_lossy().into_owned()),
            ..record()
        },
        &sender,
    );

    let started = std::time::Instant::now();
    glib::MainContext::default().block_on(glib::timeout_future(Duration::from_millis(100)));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the main loop kept running while a FIFO image path loaded"
    );
    assert!(
        picture(&card).is_none_or(|picture| picture.paintable().is_none()),
        "a FIFO never yields an image"
    );

    let png = scratch.join("ok.png");
    gdk::MemoryTexture::new(
        2,
        2,
        gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from_static(&[0u8; 16]),
        8,
    )
    .save_to_png(&png)
    .expect("a fixture png is written");
    let second = NotificationCard::new();
    let frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    frame.append(&second);
    let stack = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    stack.append(&frame);
    let root = gtk4::Window::new();
    root.set_child(Some(&stack));
    let config = Config::default();
    let app = App {
        actions: ComponentActions::install(&gio::SimpleActionGroup::new(), "test", Ok(())),
        root,
        stack,
        state: PopupState::new(config.notifications.clone()),
        config,
        services: None,
        services_start: relm4::spawn(std::future::pending()),
        rows: HashMap::from([(
            1,
            Entry {
                frame: frame.clone(),
                card: second.clone(),
                timer: Timer {
                    countdown: None,
                    task: None,
                    hovered: false,
                },
                animation: animation(&frame),
                state: EntryState::Visible,
                image: None,
            },
        )]),
        surface_edge: NotificationEdge::TopRight,
        theme_watch: relm4::spawn(std::future::pending()),
        styles: Styles::install(color_scheme(glimpse_config::ColorScheme::Desktop)),
        input_region_pending: Rc::new(Cell::new(false)),
        input_region_cards: Rc::new(RefCell::new(Vec::new())),
    };
    let _ = App::fill(
        &second,
        &NotificationRecord {
            image: Some(png.to_string_lossy().into_owned()),
            ..record()
        },
        &sender,
    );
    app.root.present();
    app.update_input_region();
    glib::MainContext::default().block_on(async {
        glib::timeout_future(Duration::from_millis(100)).await;
        assert!(!app.input_region_pending.get());
        let height = second.height();
        let Some(Input::ImageLoaded(id, texture)) = receiver.recv().await else {
            panic!("a regular png arrives after the async load");
        };
        app.image_loaded(id, &texture, sender.clone());
        assert!(
            app.input_region_pending.get(),
            "late artwork schedules a new input region"
        );
        let Some(Input::Allocated) = receiver.recv().await else {
            panic!("height enforcement runs after allocation");
        };
        assert!(!app.input_region_pending.get());
        assert!(
            second.height() > height,
            "artwork enlarges the settled card"
        );
        assert!(picture(&second).is_some_and(|picture| picture.paintable().is_some()));
        let bounds = app.input_region_cards.borrow()[0]
            .compute_bounds(app.root.upcast_ref::<gtk4::Widget>())
            .expect("card bounds");
        assert_eq!(enclosing_rectangle(bounds).height(), second.height());
    });
    app.root.close();
    app.services_start.abort();
    app.theme_watch.abort();
    let _ = std::fs::remove_dir_all(&scratch);
}

fn picture(card: &NotificationCard) -> Option<gtk4::Picture> {
    let mut pending = vec![card.clone().upcast::<gtk4::Widget>()];
    while let Some(widget) = pending.pop() {
        if let Some(picture) = widget.downcast_ref::<gtk4::Picture>() {
            return Some(picture.clone());
        }
        let mut child = widget.first_child();
        while let Some(next) = child {
            child = next.next_sibling();
            pending.push(next);
        }
    }
    None
}

fn record() -> NotificationRecord {
    NotificationRecord {
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
    }
}

#[test]
fn a_popup_lives_as_long_as_its_sender_asked() {
    let record = record();
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
