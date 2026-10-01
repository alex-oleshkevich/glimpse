use glimpse_wayland::capture::Frame;

use super::*;
use crate::doc::document::{MarkId, Source};
use crate::doc::units::Scale;

fn frame() -> Frame {
    Frame {
        connector: "DP-2".to_owned(),
        width: 400,
        height: 300,
        pixels: glib::Bytes::from_owned(vec![0u8; 400 * 300 * 4]),
        alpha: false,
    }
}

fn sample_marks() -> Vec<(MarkId, Mark)> {
    vec![
        (
            0,
            Mark::Arrow {
                from: Point {
                    x: BufferPx(10),
                    y: BufferPx(10),
                },
                to: Point {
                    x: BufferPx(90),
                    y: BufferPx(70),
                },
                style: Style {
                    color: PaletteColor::Red,
                    stroke: Stroke::Medium,
                },
            },
        ),
        (
            1,
            Mark::Rect {
                rect: Rect {
                    x: BufferPx(20),
                    y: BufferPx(20),
                    width: BufferPx(50),
                    height: BufferPx(40),
                },
                style: Style {
                    color: PaletteColor::Blue,
                    stroke: Stroke::Thin,
                },
                filled: false,
            },
        ),
        (
            2,
            Mark::Oval {
                rect: Rect {
                    x: BufferPx(30),
                    y: BufferPx(30),
                    width: BufferPx(60),
                    height: BufferPx(45),
                },
                style: Style {
                    color: PaletteColor::Green,
                    stroke: Stroke::Thick,
                },
            },
        ),
        (
            3,
            Mark::Number {
                at: Point {
                    x: BufferPx(15),
                    y: BufferPx(15),
                },
                style: Style {
                    color: PaletteColor::Yellow,
                    stroke: Stroke::Thick,
                },
            },
        ),
        (
            4,
            Mark::Text {
                at: Point {
                    x: BufferPx(40),
                    y: BufferPx(50),
                },
                text: "hello".to_owned(),
                style: Style {
                    color: PaletteColor::White,
                    stroke: Stroke::Medium,
                },
            },
        ),
        (
            5,
            Mark::Blur {
                rect: Rect {
                    x: BufferPx(5),
                    y: BufferPx(5),
                    width: BufferPx(20),
                    height: BufferPx(15),
                },
            },
        ),
        (
            6,
            Mark::Line {
                from: Point {
                    x: BufferPx(10),
                    y: BufferPx(20),
                },
                to: Point {
                    x: BufferPx(60),
                    y: BufferPx(80),
                },
                style: Style {
                    color: PaletteColor::Blue,
                    stroke: Stroke::Thin,
                },
            },
        ),
        (
            7,
            Mark::Path {
                points: vec![
                    Point {
                        x: BufferPx(1),
                        y: BufferPx(2),
                    },
                    Point {
                        x: BufferPx(3),
                        y: BufferPx(4),
                    },
                ],
                style: Style {
                    color: PaletteColor::Green,
                    stroke: Stroke::Medium,
                },
                highlighter: false,
            },
        ),
        (
            8,
            Mark::Path {
                points: vec![
                    Point {
                        x: BufferPx(4),
                        y: BufferPx(5),
                    },
                    Point {
                        x: BufferPx(6),
                        y: BufferPx(7),
                    },
                ],
                style: Style {
                    color: PaletteColor::Yellow,
                    stroke: Stroke::Thick,
                },
                highlighter: true,
            },
        ),
    ]
}

fn sample_document() -> Document {
    Document {
        source: Source::Output {
            name: "DP-2".to_owned(),
            scale: Scale(1.25),
            frame: frame(),
        },
        crop: Some(Rect {
            x: BufferPx(0),
            y: BufferPx(0),
            width: BufferPx(200),
            height: BufferPx(200),
        }),
        marks: sample_marks(),
        selected: None,
    }
}

#[test]
fn a_round_trip_through_json_keeps_every_mark_of_every_kind() {
    let doc = sample_document();
    let (out, dropped) = annotations(&doc);
    assert_eq!(dropped, 0);

    let json = serde_json::to_string(&out).expect("serializes");
    let parsed: Vec<AnnotationV1> = serde_json::from_str(&json).expect("parses");
    let round_tripped = marks(&parsed).expect("within the limits");

    let originals: Vec<Mark> = doc.marks.into_iter().map(|(_, mark)| mark).collect();
    assert_eq!(round_tripped, originals);
}

#[test]
fn a_filled_rect_round_trips_and_a_hollow_one_omits_the_flag() {
    let filled: Vec<AnnotationV1> = serde_json::from_str(
        r#"[{"kind":"rect","x":1,"y":2,"width":3,"height":4,"color":"red","stroke":2,"filled":true},
                {"kind":"rect","x":1,"y":2,"width":3,"height":4,"color":"red","stroke":2}]"#,
    )
    .expect("parses");
    let parsed = marks(&filled).expect("within the limits");
    assert!(matches!(parsed[0], Mark::Rect { filled: true, .. }));
    assert!(matches!(parsed[1], Mark::Rect { filled: false, .. }));

    let written = serde_json::to_value(&filled).expect("serializes");
    assert_eq!(written[0]["filled"], true);
    assert!(written[1].get("filled").is_none(), "{written}");
}

#[test]
fn pixelation_strokes_round_trip_and_reject_bad_width() {
    let input = r#"[{"kind":"pixelate","points":[[10,20],[30,20]],"width":40}]"#;
    let parsed: Vec<AnnotationV1> = serde_json::from_str(input).expect("parses");
    let parsed_marks = marks(&parsed).expect("valid stroke");
    assert!(matches!(parsed_marks[0], Mark::Pixelate { width: 40, .. }));
    let output = to_annotation(&parsed_marks[0], None);
    assert_eq!(
        serde_json::to_value(output).expect("serializes"),
        serde_json::to_value(&parsed[0]).expect("serializes")
    );
    let bad: Vec<AnnotationV1> =
        serde_json::from_str(r#"[{"kind":"pixelate","points":[[10,20]],"width":0}]"#)
            .expect("parses");
    assert!(marks(&bad).is_err());
}

#[test]
fn over_max_marks_errs_naming_the_limit() {
    let too_many: Vec<AnnotationV1> = (0..257)
        .map(|_| AnnotationV1::Blur {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        })
        .collect();
    let error = marks(&too_many).expect_err("over the limit");
    assert!(error.contains("256"));
}

#[test]
fn an_over_length_text_errs_naming_the_index() {
    let long_text = "a".repeat(MAX_TEXT_CHARS + 1);
    let annotations = vec![
        AnnotationV1::Blur {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        },
        AnnotationV1::Text {
            x: 0,
            y: 0,
            text: long_text,
            color: PaletteColor::Red,
            stroke: 2,
        },
    ];
    let error = marks(&annotations).expect_err("over the character limit");
    assert!(error.contains("1024"));
    assert!(error.contains("annotation 1"));
}

#[test]
fn malformed_and_overlong_paths_are_rejected() {
    let too_many = format!(
        "[{{\"kind\":\"freehand\",\"points\":[{}],\"color\":\"red\",\"stroke\":2}}]",
        vec!["[1,2]"; MAX_PATH_POINTS + 1].join(",")
    );
    assert!(serde_json::from_str::<Vec<AnnotationV1>>(&too_many).is_err());
    let malformed = r#"[{"kind":"highlighter","points":[[1,2],[3]],"color":"red","stroke":2}]"#;
    assert!(serde_json::from_str::<Vec<AnnotationV1>>(malformed).is_err());
    let short: Vec<AnnotationV1> =
        serde_json::from_str(r#"[{"kind":"freehand","points":[[1,2]],"color":"red","stroke":2}]"#)
            .expect("valid JSON");
    assert!(marks(&short).is_err());
}

#[test]
fn a_mark_wholly_outside_the_crop_is_not_exported_and_counted_as_dropped() {
    let mut doc = sample_document();
    doc.marks.push((
        6,
        Mark::Blur {
            rect: Rect {
                x: BufferPx(500),
                y: BufferPx(500),
                width: BufferPx(10),
                height: BufferPx(10),
            },
        },
    ));

    let (out, dropped) = annotations(&doc);
    assert_eq!(dropped, 1);
    assert_eq!(out.len(), doc.marks.len() - 1);
}

#[test]
fn a_highlighter_stroke_touching_the_crop_is_kept_in_json() {
    let mut doc = sample_document();
    doc.crop = Some(Rect {
        x: BufferPx(0),
        y: BufferPx(0),
        width: BufferPx(100),
        height: BufferPx(100),
    });
    doc.marks = vec![(
        0,
        Mark::Path {
            points: vec![
                Point {
                    x: BufferPx(105),
                    y: BufferPx(10),
                },
                Point {
                    x: BufferPx(105),
                    y: BufferPx(50),
                },
            ],
            style: Style {
                color: PaletteColor::Yellow,
                stroke: Stroke::Medium,
            },
            highlighter: true,
        },
    )];
    let (out, dropped) = annotations(&doc);
    assert_eq!(dropped, 0);
    assert_eq!(out.len(), 1);
}

#[test]
fn a_mark_exactly_on_the_crop_edge_is_kept() {
    let mut doc = sample_document();
    doc.marks = vec![
        (
            10,
            Mark::Number {
                at: Point {
                    x: BufferPx(0),
                    y: BufferPx(0),
                },
                style: Style {
                    color: PaletteColor::Red,
                    stroke: Stroke::Medium,
                },
            },
        ),
        (
            11,
            Mark::Arrow {
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
            },
        ),
    ];

    let (out, dropped) = annotations(&doc);
    assert_eq!(dropped, 0);
    assert_eq!(out.len(), 2);
}
