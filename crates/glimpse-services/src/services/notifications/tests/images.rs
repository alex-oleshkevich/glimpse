use super::*;

#[test]
fn image_hints_remain_content_images() {
    let app_image = record(
        1,
        Incoming {
            app_id: "org.example.Chat".to_owned(),
            icon: Some("org.example.Chat".to_owned()),
            image: Some("file:///tmp/org.example.Chat.png".to_owned()),
            ..incoming("Chat", "Marta")
        },
    );
    assert_eq!(
        app_image.image.as_deref(),
        Some("/tmp/org.example.Chat.png")
    );

    let screenshot = record(
        2,
        Incoming {
            image: Some("/tmp/Screenshot_2026-09-12.png".to_owned()),
            ..incoming("Screenshots", "Screenshot captured")
        },
    );
    assert_eq!(
        screenshot.image.as_deref(),
        Some("/tmp/Screenshot_2026-09-12.png")
    );
}

#[test]
fn only_image_hints_fill_the_image_slot_and_the_app_icon_never_does() {
    let text_only = visuals("/usr/share/pixmaps/chat.png", None, None);
    assert_eq!(
        text_only,
        (Some("/usr/share/pixmaps/chat.png".to_owned()), None)
    );
    let themed = visuals("org.example.Chat", None, None);
    assert_eq!(themed, (Some("org.example.Chat".to_owned()), None));
    let media = visuals(
        "org.example.Chat",
        Some("file:///tmp/photo.jpg"),
        Some("/run/pixels.png".to_owned()),
    );
    assert_eq!(
        media,
        (
            Some("org.example.Chat".to_owned()),
            Some("/run/pixels.png".to_owned())
        )
    );
}

#[test]
fn icons_and_images_resolve_from_names_paths_and_uris() {
    let owned = |value: &str| Some(value.to_owned());
    for (app_icon, image_path, image_data, expected) in [
        (
            "dialog-warning",
            None,
            None,
            (owned("dialog-warning"), None),
        ),
        ("file:///a%20b.png", None, None, (owned("/a b.png"), None)),
        ("/x.png", None, None, (owned("/x.png"), None)),
        (
            "",
            Some("file:///a%20b.png"),
            None,
            (None, owned("/a b.png")),
        ),
        ("app", Some("/y.png"), None, (owned("app"), owned("/y.png"))),
        (
            "",
            Some("dialog-warning"),
            None,
            (owned("dialog-warning"), None),
        ),
        (
            "file:///a.png",
            Some("/b.png"),
            None,
            (owned("/a.png"), owned("/b.png")),
        ),
        (
            "/a.png",
            Some("/b.png"),
            owned("/c.png"),
            (owned("/a.png"), owned("/c.png")),
        ),
        ("http://x/y.png", Some("rel/z.png"), None, (None, None)),
        ("app", Some("audio-volume-high"), None, (owned("app"), None)),
    ] {
        assert_eq!(
            visuals(app_icon, image_path, image_data),
            expected,
            "{app_icon} {image_path:?}"
        );
    }
}

#[test]
fn image_data_encodes_as_a_png_of_the_same_size() {
    let padded = [[255, 0, 0, 255, 0, 255, 0, 255, 9, 9], [0; 10]].concat();
    let pixels = pixels(&image_data((2, 2, 10), true, 8, 4, padded)).expect("valid pixels");
    let bytes = encode(&pixels).expect("encoded");

    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let reader = decoder.read_info().expect("a png");
    let info = reader.info();
    assert_eq!((info.width, info.height), (2, 2));
    assert_eq!(info.color_type, png::ColorType::Rgba);
}

#[test]
fn malformed_image_data_is_ignored() {
    for hints in [
        image_data((2, 2, 8), true, 16, 4, vec![0; 32]),
        image_data((2, 2, 8), true, 8, 3, vec![0; 32]),
        image_data((2, 2, 8), true, 8, 4, vec![0; 15]),
        image_data((2, 2, 4), true, 8, 4, vec![0; 32]),
        image_data((5000, 1, 20000), true, 8, 4, vec![0; 20000]),
        image_data((0, 1, 4), true, 8, 4, vec![0; 4]),
    ] {
        assert!(pixels(&hints).is_none());
    }
}

#[test]
fn only_a_file_directly_in_the_images_directory_is_ever_deleted() {
    let dir = Path::new("/run/user/1000/glimpse/notifications/7");
    assert!(generated(
        "/run/user/1000/glimpse/notifications/7/3-1.png",
        dir
    ));
    assert!(!generated(
        "/run/user/1000/glimpse/notifications/7/../../../../home/u/.ssh/id_ed25519",
        dir
    ));
    assert!(!generated("/run/user/1000/glimpse/notifications/7", dir));
    assert!(!generated("/home/u/picture.png", dir));
}
