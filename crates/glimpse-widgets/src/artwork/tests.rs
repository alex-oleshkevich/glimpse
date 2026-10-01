use super::*;

fn png(width: i32, height: i32) -> Vec<u8> {
    let source =
        Pixbuf::new(gtk4::gdk_pixbuf::Colorspace::Rgb, false, 8, width, height).expect("a pixbuf");
    source.fill(0x00ff00ff);
    source
        .save_to_bufferv("png", &[])
        .expect("a png buffer")
        .to_vec()
}

/// The clipboard caps an entry's *bytes*, which says nothing about its pixel count: a small
/// compressed image can decode to an enormous bitmap. The decode has to be bounded too.
#[test]
fn a_large_image_is_scaled_during_decode_rather_than_after() {
    let (texture, source) = thumbnail(&png(4000, 3000), 48).expect("a thumbnail");
    assert_eq!(
        source,
        (4000, 3000),
        "the size reported is the image's, not the decode's"
    );

    assert!(texture.width() <= 64, "got {}", texture.width());
    assert!(texture.height() <= 64, "got {}", texture.height());
}

#[test]
fn a_small_image_is_not_enlarged() {
    let (texture, _) = thumbnail(&png(16, 16), 48).expect("a thumbnail");

    assert_eq!((texture.width(), texture.height()), (16, 16));
}

/// Another application chose these bytes. A picture that will not decode is not an error path:
/// the entry stays restorable and the row falls back to its icon.
#[test]
fn bytes_that_are_not_an_image_yield_nothing_rather_than_panicking() {
    assert!(thumbnail(b"definitely not a png", 48).is_none());
    assert!(thumbnail(&[], 48).is_none());
}

#[test]
fn a_square_source_larger_than_the_slot_is_scaled_and_not_cropped() {
    let cover = cover(1000, 1000, 100).expect("a cover");

    assert_eq!((cover.width, cover.height), (100, 100));
    assert_eq!((cover.x, cover.y, cover.side), (0, 0, 100));
}

/// A video thumbnail is the common case, and the whole reason this exists: letterboxed inside
/// a square slot it leaves the corners showing the popover behind it.
#[test]
fn a_wide_source_is_scaled_by_its_shorter_side_and_cropped_in_the_middle() {
    let cover = cover(1920, 1080, 100).expect("a cover");

    assert_eq!(
        cover.height, 100,
        "the shorter side is what reaches the slot"
    );
    assert_eq!(cover.width, 178);
    assert_eq!(cover.side, 100);
    assert_eq!(cover.x, 39, "the crop is centred, not taken from the left");
    assert_eq!(cover.y, 0);
    assert_eq!(
        cover.width - (cover.x + cover.side),
        cover.x,
        "the margin dropped on the left and on the right must be the same"
    );
}

#[test]
fn a_tall_source_crops_vertically_instead() {
    let cover = cover(1080, 1920, 100).expect("a cover");

    assert_eq!(cover.width, 100);
    assert_eq!(cover.side, 100);
    assert_eq!(cover.x, 0);
    assert_eq!(cover.y, 39);
}

/// Enlarging costs memory and buys nothing: `Gtk.Image` scales whatever it is given up to the
/// slot anyway, and it looks no better for having been enlarged twice.
#[test]
fn a_source_smaller_than_the_slot_is_left_at_its_own_size() {
    let cover = cover(64, 64, 192).expect("a cover");

    assert_eq!((cover.width, cover.height), (64, 64));
    assert_eq!(cover.side, 64);
}

#[test]
fn a_small_wide_source_is_still_cropped_square_without_being_enlarged() {
    let cover = cover(160, 90, 192).expect("a cover");

    assert_eq!((cover.width, cover.height), (160, 90));
    assert_eq!(cover.side, 90);
    assert_eq!(cover.x, 35);
}

#[test]
fn a_source_with_no_area_yields_nothing_rather_than_a_division() {
    assert_eq!(cover(0, 100, 64), None);
    assert_eq!(cover(100, 0, 64), None);
    assert_eq!(cover(-1, 100, 64), None);
    assert_eq!(cover(100, 100, 0), None);
}

#[test]
fn the_crop_never_reaches_outside_the_scaled_image() {
    for (width, height) in [(1920, 1080), (1080, 1920), (3000, 2999), (7, 4000), (1, 1)] {
        let cover = cover(width, height, 100).expect("a cover");
        assert!(cover.x >= 0 && cover.y >= 0);
        assert!(cover.x + cover.side <= cover.width, "{width}x{height}");
        assert!(cover.y + cover.side <= cover.height, "{width}x{height}");
    }
}
