use super::*;

fn header(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = PNG_SIGNATURE.to_vec();
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes
}

#[test]
fn invalid_pngs_are_refused_before_decoding() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("image.png");
    for (width, height) in [(20_000, 10), (10, 20_000)] {
        let bytes = header(width, height);
        std::fs::write(&path, &bytes).expect("writes");
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
    assert_eq!(png_size(&header(16384, 1)), Some((16384, 1)));
    assert_eq!(png_size(&header(1, 16384)), Some((1, 16384)));
    assert_eq!(png_size(&header(1, 1)[..20]), None);
    assert_eq!(
        crate::errors::exit(&load(&dir.path().join("absent")).expect_err("missing")),
        crate::errors::Exit::NotFound
    );
}
