use super::*;

fn raw(width: u32, height: u32, transform: wl_output::Transform, y_invert: bool) -> Raw {
    Raw {
        width,
        height,
        stride: width * 4,
        layout: Layout::Rgbx,
        y_invert,
        transform,
    }
}

fn numbered(width: u32, height: u32) -> Vec<u8> {
    (0..width * height)
        .flat_map(|index| [index as u8, 0, 0, 255])
        .collect()
}

fn reds(frame: &Frame) -> Vec<u8> {
    frame.pixels.chunks(4).map(|pixel| pixel[0]).collect()
}

#[test]
fn a_bgrx_buffer_reads_as_red_green_blue() {
    let bytes = [0x3F, 0x56, 0xE0, 0xFF];
    let raw = Raw {
        layout: Layout::Bgrx,
        ..raw(1, 1, wl_output::Transform::Normal, false)
    };
    let frame = Frame::upright("DP-1".to_owned(), raw, &bytes);

    assert_eq!(frame.pixel(0, 0), [0xE0, 0x56, 0x3F]);
}

#[test]
fn a_normal_buffer_is_read_row_by_row() {
    let frame = Frame::upright(
        String::new(),
        raw(3, 2, wl_output::Transform::Normal, false),
        &numbered(3, 2),
    );

    assert_eq!((frame.width, frame.height), (3, 2));
    assert_eq!(reds(&frame), [0, 1, 2, 3, 4, 5]);
}

#[test]
fn an_inverted_buffer_is_read_bottom_row_first() {
    let frame = Frame::upright(
        String::new(),
        raw(3, 2, wl_output::Transform::Normal, true),
        &numbered(3, 2),
    );

    assert_eq!(reds(&frame), [3, 4, 5, 0, 1, 2]);
}

#[test]
fn a_quarter_turn_swaps_the_sides() {
    let frame = Frame::upright(
        String::new(),
        raw(3, 2, wl_output::Transform::_90, false),
        &numbered(3, 2),
    );

    assert_eq!((frame.width, frame.height), (2, 3));
    assert_eq!(reds(&frame), [3, 0, 4, 1, 5, 2]);
}

#[test]
fn a_half_turn_reverses_the_buffer() {
    let frame = Frame::upright(
        String::new(),
        raw(3, 2, wl_output::Transform::_180, false),
        &numbered(3, 2),
    );

    assert_eq!(reds(&frame), [5, 4, 3, 2, 1, 0]);
}

fn grim(raw: Raw, x: usize, y: usize) -> (usize, usize) {
    use wl_output::Transform as T;
    let (w, h) = (raw.width as f64, raw.height as f64);
    let (angle, flipped) = match raw.transform {
        T::Normal => (0.0, false),
        T::_90 => (std::f64::consts::FRAC_PI_2, false),
        T::_180 => (std::f64::consts::PI, false),
        T::_270 => (3.0 * std::f64::consts::FRAC_PI_2, false),
        T::Flipped => (0.0, true),
        T::Flipped90 => (std::f64::consts::FRAC_PI_2, true),
        T::Flipped180 => (std::f64::consts::PI, true),
        _ => (3.0 * std::f64::consts::FRAC_PI_2, true),
    };
    let (cos, sin) = (angle.cos().round(), angle.sin().round());
    let turned = sin != 0.0;
    let (lw, lh) = if turned { (h, w) } else { (w, h) };
    for v in 0..raw.height as usize {
        for u in 0..raw.width as usize {
            let a = u as f64 + 0.5 - w / 2.0;
            let mut b = v as f64 + 0.5 - h / 2.0;
            if raw.y_invert {
                b = -b;
            }
            let (mut rx, ry) = (cos * a - sin * b, sin * a + cos * b);
            if flipped {
                rx = -rx;
            }
            let (lx, ly) = (rx + lw / 2.0 - 0.5, ry + lh / 2.0 - 0.5);
            if lx.round() as usize == x && ly.round() as usize == y {
                return (u, v);
            }
        }
    }
    unreachable!("every logical pixel has a source")
}

#[test]
fn every_transform_matches_the_mapping_grim_uses() {
    use wl_output::Transform as T;
    let transforms = [
        T::Normal,
        T::_90,
        T::_180,
        T::_270,
        T::Flipped,
        T::Flipped90,
        T::Flipped180,
        T::Flipped270,
    ];
    for transform in transforms {
        for y_invert in [false, true] {
            let raw = raw(3, 2, transform, y_invert);
            let frame = Frame::upright(String::new(), raw, &numbered(3, 2));
            for y in 0..frame.height as usize {
                for x in 0..frame.width as usize {
                    let (u, v) = grim(raw, x, y);
                    assert_eq!(
                        frame.pixel(x as u32, y as u32)[0] as usize,
                        v * 3 + u,
                        "{transform:?} y_invert={y_invert} at ({x}, {y})"
                    );
                }
            }
        }
    }
}

#[test]
fn a_pixel_past_the_edge_reads_the_edge() {
    let frame = Frame::upright(
        String::new(),
        raw(3, 2, wl_output::Transform::Normal, false),
        &numbered(3, 2),
    );

    assert_eq!(frame.pixel(99, 99), [5, 0, 0]);
}

const EXT: [(&str, u32); 2] = [
    ("ext_image_copy_capture_manager_v1", 1),
    ("ext_output_image_capture_source_manager_v1", 1),
];
const TOPLEVELS: [(&str, u32); 2] = [
    ("ext_foreign_toplevel_image_capture_source_manager_v1", 1),
    ("ext_foreign_toplevel_list_v1", 1),
];
const WLR: (&str, u32) = ("zwlr_screencopy_manager_v1", 3);

#[test]
fn ext_is_chosen_whenever_it_is_offered() {
    let mut globals = vec![WLR, ("wl_output", 4)];
    globals.extend(EXT);
    globals.extend(TOPLEVELS);

    let chosen = choose(&globals).unwrap();

    assert_eq!(
        chosen,
        Capabilities {
            outputs: Backend::ExtImageCopy,
            toplevels: true
        }
    );
}

#[test]
fn screencopy_alone_captures_outputs_and_no_windows() {
    let chosen = choose(&[("wl_output", 4), WLR]).unwrap();

    assert_eq!(
        chosen,
        Capabilities {
            outputs: Backend::WlrScreencopy,
            toplevels: false
        }
    );
}

#[test]
fn neither_protocol_is_unsupported() {
    assert!(matches!(
        choose(&[("wl_output", 4), ("wl_shm", 1)]),
        Err(CaptureError::Unsupported)
    ));
    assert!(matches!(choose(&[]), Err(CaptureError::Unsupported)));
}

#[test]
fn ext_without_a_toplevel_source_captures_no_windows() {
    let chosen = choose(&EXT).unwrap();

    assert_eq!(
        chosen,
        Capabilities {
            outputs: Backend::ExtImageCopy,
            toplevels: false
        }
    );
    let mut without_list = EXT.to_vec();
    without_list.push(TOPLEVELS[0]);
    assert!(!choose(&without_list).unwrap().toplevels);
}

#[test]
fn a_copy_manager_without_an_output_source_falls_back_to_screencopy() {
    let chosen = choose(&[EXT[0], WLR]).unwrap();

    assert_eq!(chosen.outputs, Backend::WlrScreencopy);
}
