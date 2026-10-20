use super::*;
use crate::{MAX_RADIUS, MIN_RADIUS};

#[test]
fn unconstrained_or_anchorless_points_pass_through_unchanged() {
    assert_eq!(constrain(None, (10, 20), true), (10, 20));
    assert_eq!(constrain(Some((5, 5)), (10, 20), false), (10, 20));
}

#[test]
fn constrained_snaps_to_whichever_axis_moved_further() {
    assert_eq!(constrain(Some((100, 100)), (140, 110), true), (140, 100));
    assert_eq!(constrain(Some((100, 100)), (110, 140), true), (100, 140));
}

#[test]
fn a_tie_locks_horizontal() {
    assert_eq!(constrain(Some((100, 100)), (140, 140), true), (140, 100));
}

#[test]
fn resizing_steps_in_both_directions_and_clamps_to_the_documented_range() {
    let grown = resized(100.0, -1.0);
    assert!(grown > 100.0);
    let shrunk = resized(100.0, 1.0);
    assert!(shrunk < 100.0);
    assert_eq!(resized(MIN_RADIUS as f32, 1.0), MIN_RADIUS as f32);
    assert_eq!(resized(MAX_RADIUS as f32, -1.0), MAX_RADIUS as f32);
    assert_eq!(resized(100.0, 0.0), 100.0);
}

#[test]
fn distance_is_straight_and_the_angle_is_measured_from_horizontal_without_a_sign() {
    let segment = Segment {
        from: (412, 268),
        to: (626, 364),
    };
    assert_eq!(segment.delta(), (214, 96));
    assert!((segment.distance() - 234.546).abs() < 0.001);
    assert!((segment.angle() - 24.1609).abs() < 0.001);

    let back = Segment {
        from: segment.to,
        to: segment.from,
    };
    assert_eq!(back.delta(), (-214, -96));
    assert_eq!(back.distance(), segment.distance());
    assert_eq!(back.angle(), segment.angle());

    let flat = Segment {
        from: (10, 5),
        to: (0, 5),
    };
    assert_eq!(flat.angle(), 0.0);
    let upright = Segment {
        from: (5, 10),
        to: (5, 0),
    };
    assert_eq!(upright.angle(), 90.0);
    let still = Segment {
        from: (5, 5),
        to: (5, 5),
    };
    assert_eq!((still.distance(), still.angle()), (0.0, 0.0));
}

#[test]
fn the_first_point_anchors_and_each_later_one_confirms_a_segment_and_chains_on() {
    let mut chain = Chain::default();
    chain.place(0, (1, 1));
    assert!(chain.confirmed().is_empty());
    assert_eq!(chain.total(), None);

    chain.place(0, (4, 5));
    chain.place(0, (4, 15));

    assert_eq!(
        chain.confirmed(),
        vec![
            Segment {
                from: (1, 1),
                to: (4, 5)
            },
            Segment {
                from: (4, 5),
                to: (4, 15)
            },
        ]
    );
    assert_eq!(chain.anchor_on(0), Some((4, 15)));
    assert_eq!(chain.total(), Some((15.0, 2)));
}

#[test]
fn a_second_press_on_the_anchor_pixel_confirms_no_segment() {
    let mut chain = Chain::default();
    chain.place(0, (100, 100));
    chain.place(0, (300, 100));
    chain.place(0, (300, 100));
    assert_eq!(chain.confirmed().len(), 1);
    assert_eq!(
        crate::measure::last_distance(&chain.confirmed()).as_deref(),
        Some("200")
    );
}

#[test]
fn leaving_an_output_drops_its_anchor_but_keeps_what_was_confirmed() {
    let mut chain = Chain::default();
    chain.place(0, (0, 0));
    chain.place(0, (3, 4));
    chain.left(1);
    assert_eq!(chain.anchor_on(0), Some((3, 4)));

    chain.left(0);
    assert_eq!(chain.anchor_on(0), None);
    chain.place(1, (9, 9));
    chain.place(1, (9, 19));

    assert_eq!(chain.on(0).len(), 1);
    assert_eq!(chain.on(1).len(), 1);
    assert_eq!(chain.confirmed().len(), 2);
}

#[test]
fn a_point_on_another_output_never_joins_the_anchor_across_outputs() {
    let mut chain = Chain::default();
    chain.place(0, (5, 5));
    chain.place(1, (6, 6));

    assert!(chain.confirmed().is_empty());
    assert_eq!(chain.anchor_on(1), Some((6, 6)));
}

#[test]
fn reset_discards_every_confirmed_segment_and_the_pending_anchor() {
    let mut chain = Chain::default();
    chain.place(0, (0, 0));
    chain.place(0, (3, 4));
    chain.place(0, (10, 10));

    chain.reset();

    assert!(chain.confirmed().is_empty());
    assert_eq!(chain.anchor_on(0), None);
    assert_eq!(chain.total(), None);
}

#[test]
fn the_readout_shows_the_position_idle_and_the_segment_once_anchored() {
    assert_eq!(
        readout(None, (412, 268), None),
        [Some("412, 268".to_owned()), None, None]
    );
    assert_eq!(
        readout(Some((412, 268)), (626, 364), None),
        [
            Some("Δx 214  Δy 96".to_owned()),
            Some("234.5px · 24.2°".to_owned()),
            None,
        ]
    );
    assert_eq!(
        readout(Some((412, 268)), (626, 364), Some((12.34, 1))),
        [
            Some("Δx 214  Δy 96".to_owned()),
            Some("segment 234.5px · 24.2°".to_owned()),
            Some("total 12.3px, 1 segment".to_owned()),
        ]
    );
    assert_eq!(
        readout(None, (1, 2), Some((195.3, 2)))[2],
        Some("total 195.3px, 2 segments".to_owned())
    );
}

fn one_lens_surface() -> Surfaces {
    gtk4::init().expect("GTK initializes");
    let monitor = gdk::Display::default()
        .expect("display")
        .monitors()
        .iter::<gdk::Monitor>()
        .flatten()
        .next()
        .expect("monitor");
    let surfaces = Rc::new(vec![surface(
        &monitor,
        Frame {
            connector: String::new(),
            width: 400,
            height: 300,
            pixels: glib::Bytes::from_owned(vec![0; 400 * 300 * 4]),
            alpha: false,
            layout: glimpse_wayland::capture::Layout::Rgbx,
        },
        Settings {
            radius: 106.0,
            zoom: 1,
        },
    )]);
    surfaces[0].lens.allocate(400, 300, -1, None);
    surfaces
}

#[test]
#[ignore = "needs a display"]
fn shift_scroll_keeps_the_axis_lock_on_the_readout() {
    let surfaces = one_lens_surface();
    let chain: Shared = Rc::new(RefCell::new(Chain {
        segments: Vec::new(),
        anchor: Some((0, (100, 100))),
    }));
    surfaces[0].lens.point(300.0, 120.0);
    let (px, _) = surfaces[0].lens.pixel();
    let radius = Cell::new(106.0);

    resize(&surfaces, 0, &chain, &radius, -1.0, true);

    assert_eq!(
        surfaces[0].lines[0].text().as_str(),
        format!("Δx {}  Δy 0", i64::from(px) - 100)
    );
}

#[test]
#[ignore = "needs a display"]
fn shift_scroll_without_an_anchor_reads_the_pointer() {
    let surfaces = one_lens_surface();
    let chain: Shared = Rc::new(RefCell::new(Chain::default()));
    surfaces[0].lens.point(300.0, 120.0);
    let (px, py) = surfaces[0].lens.pixel();
    let radius = Cell::new(106.0);

    resize(&surfaces, 0, &chain, &radius, -1.0, true);

    assert_eq!(surfaces[0].lines[0].text().as_str(), format!("{px}, {py}"));
}

#[test]
#[ignore = "needs a display"]
fn widgets() {
    use gtk4::{graphene, gsk, subclass::prelude::*};

    gtk4::init().expect("GTK initializes");
    let monitor = gdk::Display::default()
        .expect("display")
        .monitors()
        .iter::<gdk::Monitor>()
        .flatten()
        .next()
        .expect("monitor");
    let surfaces = Rc::new(vec![surface(
        &monitor,
        Frame {
            connector: String::new(),
            width: 400,
            height: 300,
            pixels: glib::Bytes::from_owned(vec![0; 400 * 300 * 4]),
            alpha: false,
            layout: glimpse_wayland::capture::Layout::Rgbx,
        },
        Settings {
            radius: 106.0,
            zoom: 1,
        },
    )]);
    let segment = Segment {
        from: (190, 145),
        to: (195, 145),
    };
    let chain = Rc::new(RefCell::new(Chain {
        segments: vec![(0, segment)],
        anchor: None,
    }));
    connect(
        &surfaces,
        0,
        chain.clone(),
        Rc::new(Cell::new(1)),
        Rc::new(Cell::new(106.0)),
        Rc::new(|_| {}),
    );
    let surface = &surfaces[0];
    surface.lens.allocate(400, 300, -1, None);
    surface.marks.allocate(400, 300, -1, None);
    let controllers = surface.lens.observe_controllers();
    let motion = controllers
        .iter::<glib::Object>()
        .flatten()
        .find_map(|controller| controller.downcast::<gtk4::EventControllerMotion>().ok())
        .expect("motion controller");
    let scroll = controllers
        .iter::<glib::Object>()
        .flatten()
        .find_map(|controller| controller.downcast::<gtk4::EventControllerScroll>().ok())
        .expect("scroll controller");
    let keys = surface
        .window
        .observe_controllers()
        .iter::<glib::Object>()
        .flatten()
        .find_map(|controller| controller.downcast::<gtk4::EventControllerKey>().ok())
        .expect("key controller");
    let alpha = |marks: &Marks, x: usize, y: usize| {
        let renderer = gsk::CairoRenderer::new();
        renderer.realize(None).expect("renderer realizes");
        let snapshot = gtk4::Snapshot::new();
        marks.imp().snapshot(&snapshot);
        let node = snapshot.to_node().expect("marks render");
        let texture =
            renderer.render_texture(&node, Some(&graphene::Rect::new(0.0, 0.0, 400.0, 300.0)));
        let mut pixels = vec![0; 400 * 300 * 4];
        texture.download(&mut pixels, 400 * 4);
        renderer.unrealize();
        pixels[(y * 400 + x) * 4 + 3]
    };
    motion.emit_by_name::<()>("motion", &[&200.5, &150.5]);
    for _ in 0..4 {
        assert!(scroll.emit_by_name::<bool>("scroll", &[&0.0, &-1.0]));
    }
    assert!(alpha(&surface.marks, 160, 125) > 200);
    assert_eq!(alpha(&surface.marks, 193, 145), 0);
    motion.emit_by_name::<()>("motion", &[&210.5, &150.5]);
    assert!(alpha(&surface.marks, 120, 125) > 200);
    for _ in 0..3 {
        assert!(scroll.emit_by_name::<bool>("scroll", &[&0.0, &1.0]));
    }
    assert!(alpha(&surface.marks, 175, 140) > 200);
    assert!(scroll.emit_by_name::<bool>("scroll", &[&0.0, &1.0]));
    assert!(alpha(&surface.marks, 193, 145) > 200);
    for _ in 0..4 {
        assert!(scroll.emit_by_name::<bool>("scroll", &[&0.0, &-1.0]));
    }
    motion.emit_by_name::<()>("motion", &[&200.5, &150.5]);
    for _ in 0..5 {
        assert!(keys.emit_by_name::<bool>(
            "key-pressed",
            &[&gdk::Key::Right, &0u32, &gdk::ModifierType::empty()]
        ));
    }
    assert!(alpha(&surface.marks, 140, 125) > 200);
    motion.emit_by_name::<()>("leave", &[]);
    assert!(alpha(&surface.marks, 193, 145) > 200);
    chain.borrow_mut().segments.clear();
    chain.borrow_mut().anchor = Some((0, (190, 140)));
    motion.emit_by_name::<()>("motion", &[&200.5, &150.5]);
    assert!(keys.emit_by_name::<bool>(
        "key-pressed",
        &[&gdk::Key::Right, &0u32, &gdk::ModifierType::SHIFT_MASK]
    ));
    assert!(alpha(&surface.marks, 175, 100) > 200);
    surface.window.destroy();

    let scaled = Marks::new((500, 375), 106.0);
    scaled.allocate(400, 300, -1, None);
    scaled.set_zoom(5);
    scaled.set(
        vec![Segment {
            from: (238, 182),
            to: (243, 182),
        }],
        None,
        None,
        Some((200.5, 150.5)),
        (250, 187),
    );
    assert!(alpha(&scaled, 160, 130) > 200);

    let zoomed = super::surface(
        &monitor,
        Frame {
            connector: String::new(),
            width: 400,
            height: 300,
            pixels: glib::Bytes::from_owned(vec![0; 400 * 300 * 4]),
            alpha: false,
            layout: glimpse_wayland::capture::Layout::Rgbx,
        },
        Settings {
            radius: 12.0,
            zoom: 175,
        },
    );
    assert_eq!(zoomed.lens.imp().zoom.get(), 175);
    assert_eq!(zoomed.lens.imp().radius.get(), 12.0);
    assert_eq!(zoomed.marks.imp().zoom.get(), 175);
    zoomed.window.destroy();
}
