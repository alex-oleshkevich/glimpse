use super::*;
use gtk4::subclass::prelude::*;

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
    assert_eq!(resized(MIN_RADIUS as f32, 1.0), MIN_RADIUS as f32);
    assert_eq!(resized(MAX_RADIUS as f32, -1.0), MAX_RADIUS as f32);
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
            zoom: 3,
        };
        let surfaces = Rc::new(vec![surface(
            &monitor,
            Frame {
                connector: String::new(),
                width: 400,
                height: 300,
                pixels: glib::Bytes::from_owned([63, 86, 224, 255].repeat(400 * 300)),
                alpha: false,
                layout: Layout::Bgrx,
            },
            settings,
        )]);
        let outcome = Rc::new(Cell::new(None));
        connect(
            &surfaces,
            0,
            Rc::new(Cell::new(settings.zoom)),
            Rc::new(Cell::new(settings.radius)),
            settings,
            Rc::new({
                let outcome = outcome.clone();
                move |picked| outcome.set(Some(picked))
            }),
        );
        let surface = &surfaces[0];
        let texture = surface
            .lens
            .imp()
            .texture
            .borrow()
            .clone()
            .expect("texture");
        let mut downloader = gdk::TextureDownloader::new(&texture);
        downloader.set_format(gdk::MemoryFormat::R8g8b8a8);
        let (pixels, _) = downloader.download_bytes();
        assert_eq!(&pixels.as_ref()[..4], &[224, 86, 63, 255]);
        assert_eq!(surface.frame.pixel(0, 0), [224, 86, 63]);
        assert_eq!(surface.lens.imp().zoom.get(), settings.zoom);
        assert_eq!(surface.lens.imp().radius.get(), settings.radius);
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
            assert_eq!(
                outcome.get(),
                format.map(|_| Outcome::Picked([224, 86, 63]))
            );
        }
        keys.emit_by_name::<bool>(
            "key-pressed",
            &[&gdk::Key::Escape, &0u32, &gdk::ModifierType::empty()],
        );
        assert_eq!(outcome.get(), Some(Outcome::Cancelled));
        surface.window.destroy();
    }
}
