use super::*;

fn frame() -> Frame {
    Frame {
        connector: "DP-2".to_owned(),
        width: 400,
        height: 300,
        pixels: glib::Bytes::from_owned(vec![0u8; 400 * 300 * 4]),
        alpha: false,
    }
}

fn number_at(id: MarkId, x: i32, y: i32) -> (MarkId, Mark) {
    (
        id,
        Mark::Number {
            at: Point {
                x: BufferPx(x),
                y: BufferPx(y),
            },
            style: Style {
                color: PaletteColor::Red,
                stroke: Stroke::Medium,
            },
        },
    )
}

fn label(mark: &Mark) -> Rect<BufferPx> {
    let (Mark::Number { at, .. } | Mark::Text { at, .. }) = mark else {
        panic!("only a label is measured");
    };
    Rect {
        x: BufferPx(at.x.0 - 10),
        y: BufferPx(at.y.0 - 10),
        width: BufferPx(20),
        height: BufferPx(20),
    }
}

fn outlined(marks: Vec<(MarkId, Mark)>) -> Document {
    let area = Rect {
        x: BufferPx(100),
        y: BufferPx(100),
        width: BufferPx(100),
        height: BufferPx(60),
    };
    let style = Style {
        color: PaletteColor::Red,
        stroke: Stroke::Thin,
    };
    let mut all = vec![
        (
            1,
            Mark::Rect {
                rect: area,
                style,
                filled: false,
            },
        ),
        (
            2,
            Mark::Oval {
                rect: Rect {
                    x: BufferPx(250),
                    ..area
                },
                style,
            },
        ),
    ];
    all.extend(marks);
    Document {
        source: Source::File {
            path: PathBuf::from("/tmp/x.png"),
            frame: frame(),
        },
        crop: None,
        marks: all,
        selected: None,
    }
}

fn point(x: i32, y: i32) -> Point<BufferPx> {
    Point {
        x: BufferPx(x),
        y: BufferPx(y),
    }
}

#[test]
fn a_click_inside_a_shape_hits_nothing_and_so_a_number_there_stamps() {
    let doc = outlined(Vec::new());

    assert_eq!(doc.hit(point(150, 130), label), None);
    assert_eq!(doc.hit(point(300, 130), label), None);
}

#[test]
fn a_filled_rect_is_hit_anywhere_inside_and_nowhere_outside() {
    let mut doc = outlined(Vec::new());
    if let Some((_, Mark::Rect { filled, .. })) = doc.marks.first_mut() {
        *filled = true;
    }

    assert_eq!(doc.hit(point(150, 130), label), Some(1));
    assert_eq!(doc.hit(point(100, 100), label), Some(1));
    assert_eq!(
        doc.hit(point(94, 130), label),
        None,
        "no stroke band outside"
    );
}

#[test]
fn a_click_on_a_shape_stroke_hits_it_within_six_pixels() {
    let doc = outlined(Vec::new());

    assert_eq!(doc.hit(point(100, 130), label), Some(1));
    assert_eq!(doc.hit(point(105, 130), label), Some(1));
    assert_eq!(doc.hit(point(94, 130), label), Some(1));
    assert_eq!(doc.hit(point(92, 130), label), None);
    assert_eq!(doc.hit(point(107, 130), label), None);
    assert_eq!(doc.hit(point(250, 130), label), Some(2));
    assert_eq!(doc.hit(point(300, 104), label), Some(2));
    assert_eq!(doc.hit(point(300, 90), label), None);
}

#[test]
fn a_number_over_a_shape_stroke_wins() {
    let doc = outlined(vec![number_at(3, 100, 130)]);

    assert_eq!(doc.hit(point(100, 130), label), Some(3));
    assert_eq!(doc.hit(point(108, 136), label), Some(3));
}

#[test]
fn hit_finds_the_topmost_mark_at_a_point() {
    let doc = Document {
        source: Source::File {
            path: PathBuf::from("/tmp/x.png"),
            frame: frame(),
        },
        crop: None,
        marks: vec![number_at(1, 10, 0), number_at(2, 10, 0)],
        selected: None,
    };

    assert_eq!(
        doc.hit(
            Point {
                x: BufferPx(10),
                y: BufferPx(0)
            },
            label
        ),
        Some(2)
    );
}

#[test]
fn exported_rect_is_the_crop_when_set_and_the_frame_bounds_otherwise() {
    let crop = Rect {
        x: BufferPx(0),
        y: BufferPx(0),
        width: BufferPx(10),
        height: BufferPx(10),
    };
    let mut doc = Document {
        source: Source::File {
            path: PathBuf::from("/tmp/x.png"),
            frame: frame(),
        },
        crop: Some(crop),
        marks: Vec::new(),
        selected: None,
    };
    assert_eq!(doc.exported_rect(), crop);

    doc.crop = None;
    assert_eq!(
        doc.exported_rect(),
        Rect {
            x: BufferPx(0),
            y: BufferPx(0),
            width: BufferPx(400),
            height: BufferPx(300),
        }
    );
}

#[test]
fn a_zero_extent_mark_exactly_on_the_crop_edge_is_not_wholly_outside() {
    let crop = Rect {
        x: BufferPx(0),
        y: BufferPx(0),
        width: BufferPx(100),
        height: BufferPx(100),
    };
    let number_on_the_corner = Mark::Number {
        at: Point {
            x: BufferPx(0),
            y: BufferPx(0),
        },
        style: Style {
            color: PaletteColor::Red,
            stroke: Stroke::Medium,
        },
    };
    assert!(!wholly_outside(&number_on_the_corner, crop, 1.0));

    let horizontal_arrow_on_the_top_edge = Mark::Arrow {
        from: Point {
            x: BufferPx(10),
            y: BufferPx(0),
        },
        to: Point {
            x: BufferPx(50),
            y: BufferPx(0),
        },
        style: Style {
            color: PaletteColor::Red,
            stroke: Stroke::Medium,
        },
    };
    assert!(!wholly_outside(
        &horizontal_arrow_on_the_top_edge,
        crop,
        1.0
    ));
}

#[test]
fn line_and_path_hit_only_near_painted_segments() {
    let style = Style {
        color: PaletteColor::Red,
        stroke: Stroke::Thin,
    };
    let mut doc = outlined(Vec::new());
    doc.marks = vec![(
        3,
        Mark::Line {
            from: point(20, 20),
            to: point(80, 80),
            style,
        },
    )];
    assert_eq!(doc.hit(point(50, 50), label), Some(3));
    assert_eq!(doc.hit(point(20, 80), label), None);

    doc.marks[0].1 = Mark::Path {
        points: vec![point(20, 20), point(80, 20), point(80, 80)],
        style,
        highlighter: false,
    };
    assert_eq!(doc.hit(point(50, 22), label), Some(3));
    assert_eq!(doc.hit(point(50, 50), label), None);
}

#[test]
fn hit_does_not_overflow_near_i32_max() {
    let doc = Document {
        source: Source::File {
            path: PathBuf::from("/tmp/x.png"),
            frame: frame(),
        },
        crop: None,
        marks: vec![(
            0,
            Mark::Arrow {
                from: Point {
                    x: BufferPx(i32::MAX - 5),
                    y: BufferPx(i32::MAX - 5),
                },
                to: Point {
                    x: BufferPx(i32::MAX),
                    y: BufferPx(i32::MAX),
                },
                style: Style {
                    color: PaletteColor::Red,
                    stroke: Stroke::Thick,
                },
            },
        )],
        selected: None,
    };

    assert_eq!(
        doc.hit(
            Point {
                x: BufferPx(i32::MAX - 2),
                y: BufferPx(i32::MAX - 2),
            },
            label
        ),
        Some(0)
    );
}
