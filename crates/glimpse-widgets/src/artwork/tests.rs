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

#[test]
fn a_thumbnail_keeps_the_aspect_of_an_image_wider_than_the_decode_limit() {
    let (texture, source) = thumbnail(&png(8000, 2000), 320).expect("a thumbnail");

    assert_eq!(source, (8000, 2000));
    let aspect = f64::from(texture.width()) / f64::from(texture.height());
    assert!((aspect - 4.0).abs() < 0.1, "got aspect {aspect}");
}

#[test]
fn a_thumbnail_of_an_extreme_strip_stays_within_the_side_and_never_collapses() {
    let (texture, _) = thumbnail(&png(65535, 48), 320).expect("a thumbnail");

    assert!(texture.width() <= 320 && texture.height() <= 320);
    assert!(texture.width() > 0 && texture.height() > 0);
}

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("glimpse-artwork-{}-{name}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_dir(&self.0);
    }
}

fn jpeg(width: i32, height: i32) -> Vec<u8> {
    let source =
        Pixbuf::new(gtk4::gdk_pixbuf::Colorspace::Rgb, false, 8, width, height).expect("a pixbuf");
    source.fill(0x00ff00ff);
    source
        .save_to_bufferv("jpeg", &[])
        .expect("a jpeg buffer")
        .to_vec()
}

#[test]
fn png_and_jpeg_files_become_square_textures() {
    for (name, bytes) in [("png", png(300, 200)), ("jpeg", jpeg(300, 200))] {
        let file = Scratch::new(name);
        std::fs::write(&file.0, bytes).expect("written");

        let texture = artwork(&file.0, 100, Formats::Strict).expect(name);

        assert_eq!((texture.width(), texture.height()), (100, 100), "{name}");
    }
}

#[test]
fn a_fifo_is_refused_without_waiting_for_a_writer() {
    let file = Scratch::new("fifo");
    let path = std::ffi::CString::new(file.0.to_string_lossy().as_bytes()).expect("a path");
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);

    assert!(artwork(&file.0, 100, Formats::Strict).is_none());
    assert!(matches!(
        cached_artwork(&file.0, 100, Formats::Strict, None),
        Artwork::Rejected(None)
    ));
}

#[test]
fn a_directory_is_refused() {
    let file = Scratch::new("directory");
    std::fs::create_dir(&file.0).expect("a directory");

    assert!(artwork(&file.0, 100, Formats::Strict).is_none());
}

#[test]
fn a_file_over_the_size_cap_is_refused_unread() {
    let file = Scratch::new("oversize");
    let mut bytes = png(8, 8);
    bytes.resize(STRICT_MAX_BYTES as usize + 1, 0);
    std::fs::write(&file.0, bytes).expect("written");

    assert!(matches!(
        cached_artwork(&file.0, 100, Formats::Strict, None),
        Artwork::Rejected(Some(_))
    ));
}

#[test]
fn a_format_outside_the_allow_list_is_refused() {
    let svg: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8"/></svg>"#;
    let gif: &[u8] = b"GIF89a\x01\x00\x01\x00\x00\x00\x00;";
    for (name, bytes) in [("svg", svg), ("gif", gif)] {
        let file = Scratch::new(name);
        std::fs::write(&file.0, bytes).expect("written");

        assert!(artwork(&file.0, 100, Formats::Strict).is_none(), "{name}");
    }
}

#[test]
fn an_image_larger_than_the_decode_limit_is_refused() {
    let file = Scratch::new("huge");
    std::fs::write(&file.0, png(LARGEST + 1, 8)).expect("written");

    assert!(artwork(&file.0, 100, Formats::Strict).is_none());
}

#[test]
fn a_file_rewritten_at_the_same_path_is_decoded_again() {
    let file = Scratch::new("rewritten");
    std::fs::write(&file.0, png(64, 64)).expect("written");
    let Artwork::Decoded(_, stamp) = cached_artwork(&file.0, 100, Formats::Strict, None) else {
        panic!("a first read decodes");
    };

    assert!(matches!(
        cached_artwork(&file.0, 100, Formats::Strict, Some(&stamp)),
        Artwork::Unchanged
    ));

    std::fs::write(&file.0, png(96, 96)).expect("rewritten");
    let Artwork::Decoded(texture, fresh) =
        cached_artwork(&file.0, 100, Formats::Strict, Some(&stamp))
    else {
        panic!("a rewritten file decodes");
    };
    assert_ne!(fresh, stamp);
    assert_eq!(texture.width(), 96);
}

#[test]
fn an_unchanged_stamp_answers_without_reading_the_bytes() {
    let file = Scratch::new("unread");
    std::fs::write(&file.0, png(64, 64)).expect("written");
    let Artwork::Decoded(_, stamp) = cached_artwork(&file.0, 100, Formats::Strict, None) else {
        panic!("a first read decodes");
    };
    let modified = std::fs::metadata(&file.0)
        .and_then(|metadata| metadata.modified())
        .expect("a modification time");
    let len = stamp.len as usize;

    std::fs::write(&file.0, vec![0u8; len]).expect("overwritten with garbage of the same size");
    std::fs::File::options()
        .write(true)
        .open(&file.0)
        .and_then(|file| file.set_modified(modified))
        .expect("the time restored");

    assert!(
        matches!(
            cached_artwork(&file.0, 100, Formats::Strict, Some(&stamp)),
            Artwork::Unchanged
        ),
        "the stamp matches, so the invalid bytes are never looked at"
    );
    assert!(
        matches!(
            cached_artwork(&file.0, 100, Formats::Strict, None),
            Artwork::Rejected(_)
        ),
        "while a caller with no stamp does read them"
    );
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

fn claimed_png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = png(8, 8);
    bytes[16..20].copy_from_slice(&width.to_be_bytes());
    bytes[20..24].copy_from_slice(&height.to_be_bytes());
    let crc = crc32(&bytes[12..29]);
    bytes[29..33].copy_from_slice(&crc.to_be_bytes());
    bytes
}

#[test]
fn a_header_claiming_a_huge_image_is_refused_quickly() {
    let file = Scratch::new("claimed");
    std::fs::write(&file.0, claimed_png(30000, 30000)).expect("written");
    let started = std::time::Instant::now();

    assert!(artwork(&file.0, 128, Formats::Strict).is_none());
    assert!(artwork(&file.0, 128, Formats::Any).is_none());
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}

#[test]
fn the_strict_bound_is_lower_than_the_loose_one() {
    let file = Scratch::new("middling");
    std::fs::write(&file.0, png(STRICT_LARGEST + 1, 8)).expect("written");

    assert!(artwork(&file.0, 100, Formats::Strict).is_none());
    assert!(artwork(&file.0, 100, Formats::Any).is_some());
}

#[test]
fn the_loose_policy_keeps_the_formats_it_always_read() {
    let file = Scratch::new("loose");
    let bmp = Pixbuf::new(gtk4::gdk_pixbuf::Colorspace::Rgb, false, 8, 40, 40)
        .expect("a pixbuf")
        .save_to_bufferv("bmp", &[])
        .expect("a bmp buffer");
    std::fs::write(&file.0, bmp).expect("written");

    assert!(artwork(&file.0, 100, Formats::Strict).is_none());
    assert!(artwork(&file.0, 100, Formats::Any).is_some());
}

#[test]
fn each_policy_has_its_own_byte_cap() {
    let file = Scratch::new("padded");
    let mut bytes = png(32, 32);
    bytes.resize(STRICT_MAX_BYTES as usize + 1, 0);
    std::fs::write(&file.0, &bytes).expect("written");

    assert!(artwork(&file.0, 100, Formats::Strict).is_none());
    assert!(artwork(&file.0, 100, Formats::Any).is_some());

    bytes.resize(ANY_MAX_BYTES as usize + 1, 0);
    std::fs::write(&file.0, &bytes).expect("written");
    assert!(artwork(&file.0, 100, Formats::Any).is_none());
}

#[test]
fn the_loose_dimension_bound_is_exact() {
    let file = Scratch::new("edge");
    std::fs::write(&file.0, png(LARGEST, 8)).expect("written");
    assert!(artwork(&file.0, 100, Formats::Any).is_some());

    std::fs::write(&file.0, png(LARGEST + 1, 8)).expect("written");
    assert!(artwork(&file.0, 100, Formats::Any).is_none());
}
