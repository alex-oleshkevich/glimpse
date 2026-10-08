use super::*;

fn checkerboard_frame(width: u32, height: u32) -> Frame {
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let at = ((y * width + x) * 4) as usize;
            let value = if (x + y) % 2 == 0 { 255 } else { 0 };
            pixels[at..at + 3].copy_from_slice(&[value, value, value]);
            pixels[at + 3] = 255;
        }
    }
    Frame {
        connector: "DP-2".to_owned(),
        width,
        height,
        pixels: glib::Bytes::from_owned(pixels),
        alpha: false,
        layout: glimpse_wayland::capture::Layout::Rgbx,
    }
}

#[test]
fn a_pixelation_stroke_has_flat_blocks_and_a_round_brush_mask() {
    let frame = checkerboard_frame(64, 64);
    let points = [
        Point {
            x: BufferPx(20),
            y: BufferPx(20),
        },
        Point {
            x: BufferPx(44),
            y: BufferPx(20),
        },
    ];
    let (bounds, tile) = pixelate_stroke(&frame, &points, 12);
    let sample =
        |x: i32, y: i32| pixel_at(&tile, (x - bounds.x.0) as usize, (y - bounds.y.0) as usize);
    assert_eq!(sample(20, 20), sample(21, 20));
    assert!((120..=135).contains(&sample(20, 20)[0]));
    assert_eq!(sample(20, 20)[3], 255);
    assert_eq!(sample(20, 27)[3], 0);
    assert_eq!(sample(44, 20)[3], 255);
}

fn pixel_at(tile: &Tile, x: usize, y: usize) -> [u8; 4] {
    let at = (y * tile.width as usize + x) * 4;
    [
        tile.rgba[at],
        tile.rgba[at + 1],
        tile.rgba[at + 2],
        tile.rgba[at + 3],
    ]
}

#[test]
fn a_blur_leaves_no_trace_of_detail_finer_than_its_block() {
    let frame = checkerboard_frame(100, 100);
    let rect = Rect {
        x: BufferPx(10),
        y: BufferPx(10),
        width: BufferPx(64),
        height: BufferPx(24),
    };
    let tile = redact(&frame, rect);

    assert_eq!((tile.width, tile.height), (64, 24));
    for y in 0..tile.height as usize {
        for x in 0..tile.width as usize {
            let [r, g, b, a] = pixel_at(&tile, x, y);
            assert!((120..=135).contains(&r), "({x},{y}) kept detail: {r}");
            assert_eq!((r, g, a), (b, b, 255));
        }
    }
}

#[test]
fn a_blur_is_a_smooth_gradient_not_flat_blocks() {
    let mut frame = checkerboard_frame(64, 16);
    let mut pixels = frame.pixels.to_vec();
    for y in 0..16 {
        for x in 0..64 {
            let at = ((y * 64 + x) * 4) as usize;
            let value = if x < 32 { 0 } else { 255 };
            pixels[at..at + 3].copy_from_slice(&[value, value, value]);
        }
    }
    frame.pixels = glib::Bytes::from_owned(pixels);
    let rect = Rect {
        x: BufferPx(0),
        y: BufferPx(0),
        width: BufferPx(64),
        height: BufferPx(16),
    };
    let tile = redact(&frame, rect);

    let row: Vec<u8> = (0..64).map(|x| pixel_at(&tile, x, 8)[0]).collect();
    assert!(row.windows(2).all(|pair| pair[0] <= pair[1]), "{row:?}");
    let steps = row.windows(2).filter(|pair| pair[0] != pair[1]).count();
    assert!(steps > 16, "the edge is spread over many pixels: {row:?}");
}

#[test]
fn the_tile_is_clamped_to_the_frame() {
    let frame = checkerboard_frame(20, 20);
    let rect = Rect {
        x: BufferPx(15),
        y: BufferPx(15),
        width: BufferPx(30),
        height: BufferPx(30),
    };
    let tile = redact(&frame, rect);

    assert_eq!(tile.width, 5);
    assert_eq!(tile.height, 5);
}

#[test]
fn a_rect_near_i32_max_does_not_overflow() {
    let frame = checkerboard_frame(10, 10);
    let rect = Rect {
        x: BufferPx(i32::MAX - 5),
        y: BufferPx(0),
        width: BufferPx(20),
        height: BufferPx(5),
    };

    let tile = redact(&frame, rect);

    assert_eq!(tile.width, 0);
    assert_eq!(tile.height, 5);
}
