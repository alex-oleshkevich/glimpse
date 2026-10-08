use glimpse_config::{PaletteColor, Stroke};
use glimpse_wayland::capture::Frame;

use super::*;
use crate::doc::document::{Source, Style};
use crate::doc::units::Scale;

fn frame() -> Frame {
    Frame {
        connector: "DP-2".to_owned(),
        width: 400,
        height: 300,
        pixels: glib::Bytes::from_owned(vec![0u8; 400 * 300 * 4]),
        alpha: false,
        layout: glimpse_wayland::capture::Layout::Rgbx,
    }
}

fn number_mark(x: i32) -> Mark {
    Mark::Number {
        at: Point {
            x: BufferPx(x),
            y: BufferPx(0),
        },
        style: Style {
            color: PaletteColor::Red,
            stroke: Stroke::Medium,
        },
    }
}

fn document_with_three_numbers() -> (Document, History) {
    let doc = Document {
        source: Source::Output {
            name: "DP-2".to_owned(),
            scale: Scale(1.25),
            frame: frame(),
        },
        crop: None,
        marks: vec![
            (0, number_mark(0)),
            (1, number_mark(10)),
            (2, number_mark(20)),
        ],
        selected: None,
    };
    (doc, History::new(3))
}

#[test]
fn removing_the_second_number_renumbers_the_third_and_undo_restores_all_three() {
    let (mut doc, mut history) = document_with_three_numbers();
    assert_eq!(doc.number_of(0), Some(1));
    assert_eq!(doc.number_of(1), Some(2));
    assert_eq!(doc.number_of(2), Some(3));

    history.apply(&mut doc, Op::Remove(1));
    assert_eq!(doc.number_of(0), Some(1));
    assert_eq!(doc.number_of(1), None);
    assert_eq!(doc.number_of(2), Some(2));

    assert!(history.undo(&mut doc));
    assert_eq!(doc.number_of(0), Some(1));
    assert_eq!(doc.number_of(1), Some(2));
    assert_eq!(doc.number_of(2), Some(3));
}

#[test]
fn undo_on_an_empty_history_returns_false() {
    let (mut doc, mut history) = document_with_three_numbers();
    assert!(!history.can_undo());
    assert!(!history.undo(&mut doc));
}

#[test]
fn redo_after_a_new_apply_returns_false() {
    let (mut doc, mut history) = document_with_three_numbers();
    history.apply(&mut doc, Op::Remove(1));
    assert!(history.can_undo());
    assert!(history.undo(&mut doc));
    assert!(history.can_redo());
    history.apply(&mut doc, Op::Remove(0));
    assert!(!history.can_redo());
    assert!(!history.redo(&mut doc));
}

#[test]
fn base_marks_survive_undoing_everything() {
    let (mut doc, mut history) = document_with_three_numbers();
    history.apply(&mut doc, Op::Add(number_mark(30)));
    history.apply(&mut doc, Op::Remove(0));

    while history.undo(&mut doc) {}

    assert_eq!(doc.marks.len(), 3);
    assert_eq!(doc.number_of(0), Some(1));
    assert_eq!(doc.number_of(1), Some(2));
    assert_eq!(doc.number_of(2), Some(3));
}

#[test]
fn moving_a_mark_translates_it_and_undo_restores_the_prior_position() {
    let (mut doc, mut history) = document_with_three_numbers();
    history.apply(
        &mut doc,
        Op::Move(
            0,
            Point {
                x: BufferPx(100),
                y: BufferPx(50),
            },
        ),
    );
    let Mark::Number { at, .. } = &doc.marks[0].1 else {
        unreachable!("still a number mark")
    };
    assert_eq!(
        *at,
        Point {
            x: BufferPx(100),
            y: BufferPx(50)
        }
    );

    assert!(history.undo(&mut doc));
    let Mark::Number { at, .. } = &doc.marks[0].1 else {
        unreachable!("still a number mark")
    };
    assert_eq!(
        *at,
        Point {
            x: BufferPx(0),
            y: BufferPx(0)
        }
    );
}
