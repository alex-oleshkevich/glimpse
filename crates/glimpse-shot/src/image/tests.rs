use super::*;
use gtk4::gdk_pixbuf::Colorspace;

fn write(path: &Path, format: &str, width: i32, height: i32) {
    Pixbuf::new(Colorspace::Rgb, true, 8, width, height)
        .expect("a pixbuf")
        .savev(path, format, &[])
        .expect("saves");
}

#[test]
fn invalid_images_are_refused_before_decoding() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("image.png");
    for (width, height) in [(20_000, 10), (10, 20_000)] {
        write(&path, "png", width, height);
        let bytes = std::fs::read(&path).expect("source");
        let refused = format!("{:#}", load(&path).expect_err("too large"));
        assert!(
            refused.contains(&format!("{width} × {height}")),
            "{refused}"
        );
        assert_eq!(std::fs::read(&path).expect("source"), bytes);
    }
    for bytes in [
        b"not an image, but long enough to hold a header".as_slice(),
        b"short",
    ] {
        std::fs::write(&path, bytes).expect("writes");
        assert!(load(&path).is_err());
    }
    assert_eq!(
        crate::errors::exit(&load(&dir.path().join("absent")).expect_err("missing")),
        crate::errors::Exit::NotFound
    );
}

#[test]
fn a_jpeg_loads() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("image.jpg");
    write(&path, "jpeg", 3, 2);
    let Source::File { frame, .. } = load(&path).expect("loads").source else {
        panic!("not a file source");
    };
    assert_eq!((frame.width, frame.height), (3, 2));
}
