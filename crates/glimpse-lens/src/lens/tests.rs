use super::*;

#[test]
fn the_value_sits_under_the_lens_and_flips_above_it_near_the_bottom_edge() {
    let area = (1000.0, 800.0);
    let pill = (120.0, 34.0);

    assert_eq!(
        pill_origin((500.0, 300.0), 88.0, pill, area),
        (440.0, 398.0)
    );
    assert_eq!(
        pill_origin((500.0, 780.0), 88.0, pill, area),
        (440.0, 648.0)
    );
    assert_eq!(pill_origin((10.0, 300.0), 88.0, pill, area), (0.0, 398.0));
    assert_eq!(
        pill_origin((995.0, 300.0), 88.0, pill, area),
        (880.0, 398.0)
    );
    assert_eq!(
        pill_origin((500.0, 300.0), 106.0, pill, area),
        (440.0, 416.0)
    );
}

#[test]
fn scrolling_resizes_the_lens_and_stays_within_bounds() {
    assert_eq!(resized(100.0, -1.0), 112.0);
    assert_eq!(resized(100.0, 1.0), 88.0);
    assert_eq!(resized(100.0, 0.0), 100.0);
    assert_eq!(resized(RADIUS_MIN, 1.0), RADIUS_MIN);
    assert_eq!(resized(RADIUS_MAX, -1.0), RADIUS_MAX);
}

#[test]
#[ignore = "needs a display"]
fn widgets() {
    gtk4::init().expect("GTK initializes");
    let monitor = gdk::Display::default()
        .expect("display")
        .monitors()
        .iter::<gdk::Monitor>()
        .flatten()
        .next()
        .expect("monitor");
    for format in [None, Some(ColorFormat::Rgb)] {
        let settings = Settings {
            format,
            radius: 88.0,
            max_zoom: 30,
        };
        let surfaces = Rc::new(vec![surface(
            &monitor,
            Frame {
                connector: String::new(),
                width: 400,
                height: 300,
                pixels: glib::Bytes::from_owned(vec![0; 400 * 300 * 4]),
                alpha: false,
            },
            1,
            settings,
        )]);
        let outcome = Rc::new(Cell::new(None));
        connect(
            &surfaces,
            0,
            Rc::new(Cell::new(1)),
            Rc::new(Cell::new(settings.radius)),
            settings,
            Rc::new({
                let outcome = outcome.clone();
                move |picked| outcome.set(Some(picked))
            }),
        );
        let surface = &surfaces[0];
        surface.lens.allocate(400, 300, -1, None);
        surface.lens.point(200.0, 150.0);
        dress(surface, settings, settings.radius);
        assert_eq!(surface.pill.get_visible(), format.is_some());
        let keys = surface
            .window
            .observe_controllers()
            .iter::<glib::Object>()
            .flatten()
            .find_map(|controller| controller.downcast::<gtk4::EventControllerKey>().ok())
            .expect("key controller");
        for key in [gdk::Key::Return, gdk::Key::KP_Enter, gdk::Key::space] {
            keys.emit_by_name::<bool>("key-pressed", &[&key, &0u32, &gdk::ModifierType::empty()]);
            assert_eq!(outcome.get(), format.map(|_| Outcome::Picked([0; 3])));
        }
        keys.emit_by_name::<bool>(
            "key-pressed",
            &[&gdk::Key::Escape, &0u32, &gdk::ModifierType::empty()],
        );
        assert_eq!(outcome.get(), Some(Outcome::Cancelled));
        surface.window.destroy();
    }
}
