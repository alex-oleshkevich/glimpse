use gtk4::prelude::*;
use gtk4::{gdk, graphene, gsk};

use super::{Rect, SQUARE, Tip, arrow, outline, rounded};

const CARD: Rect = Rect {
    x: 10,
    y: 20,
    width: 200,
    height: 100,
};

fn area(strips: &[Rect]) -> i32 {
    strips.iter().map(|strip| strip.width * strip.height).sum()
}

fn covers(strips: &[Rect], x: i32, y: i32) -> bool {
    strips
        .iter()
        .any(|s| (s.x..s.x + s.width).contains(&x) && (s.y..s.y + s.height).contains(&y))
}

#[test]
fn square_corners_are_one_rectangle() {
    assert_eq!(rounded(CARD, SQUARE), [CARD]);
}

#[test]
fn a_rounded_corner_leaves_its_outer_pixel_clear_and_its_edges_whole() {
    let strips = rounded(CARD, [(12.0, 12.0); 4]);

    for (x, y) in [(10, 20), (209, 20), (10, 119), (209, 119)] {
        assert!(
            !covers(&strips, x, y),
            "the corner pixel at {x},{y} is blurred"
        );
    }
    for (x, y) in [(110, 20), (110, 119), (10, 70), (209, 70)] {
        assert!(
            covers(&strips, x, y),
            "the edge pixel at {x},{y} is not blurred"
        );
    }
}

#[test]
fn a_rounded_region_is_symmetric_and_runs_merge() {
    let strips = rounded(CARD, [(12.0, 12.0); 4]);
    let mirrored: Vec<Rect> = strips
        .iter()
        .rev()
        .map(|s| Rect {
            y: CARD.y + CARD.height - (s.y - CARD.y) - s.height,
            ..*s
        })
        .collect();

    assert_eq!(strips, mirrored);
    assert!(strips.len() <= 25, "{} strips", strips.len());
    assert!(
        strips
            .iter()
            .any(|s| s.width == CARD.width && s.height >= CARD.height - 24)
    );
}

#[test]
fn the_rounded_area_is_the_rectangle_less_four_corners() {
    let radius = 12.0_f32;
    let corners = 4.0 * (radius * radius - std::f32::consts::PI * radius * radius / 4.0);
    let expected = (CARD.width * CARD.height) as f32 - corners;

    let actual = area(&rounded(CARD, [(radius, radius); 4])) as f32;

    assert!(
        (actual - expected).abs() < 20.0,
        "{actual} against {expected}"
    );
}

#[test]
fn a_radius_larger_than_the_rectangle_is_clamped_to_a_pill() {
    let strips = rounded(CARD, [(500.0, 500.0); 4]);

    assert!(covers(&strips, 110, 70));
    assert!(!covers(&strips, 10, 20));
    assert!(strips.iter().all(|s| s.width > 0 && s.height > 0));
}

#[test]
fn corners_are_independent() {
    let strips = rounded(CARD, [(12.0, 12.0), (0.0, 0.0), (0.0, 0.0), (0.0, 0.0)]);

    assert!(!covers(&strips, 10, 20));
    assert!(covers(&strips, 209, 20));
    assert!(covers(&strips, 10, 119));
    assert!(covers(&strips, 209, 119));
}

#[test]
fn the_outline_is_the_rounded_clip_matching_the_widget_through_any_wrapping() {
    let rounded_clip = |width: f32, height: f32, radius: f32| {
        let bounds = graphene::Rect::new(0.0, 0.0, width, height);
        let corner = graphene::Size::new(radius, radius);
        let fill = gsk::ColorNode::new(&gdk::RGBA::RED, &bounds);
        gsk::RoundedClipNode::new(
            &fill,
            &gsk::RoundedRect::new(bounds, corner, corner, corner, corner),
        )
    };
    let button = rounded_clip(20.0, 20.0, 4.0);
    let card = gsk::OpacityNode::new(rounded_clip(200.0, 100.0, 14.0), 0.5);
    let tree = gsk::ContainerNode::new(&[button.upcast(), card.upcast()]);

    assert_eq!(
        outline(tree.upcast_ref(), 200.0, 100.0),
        Some([(14.0, 14.0); 4])
    );

    let flat = gsk::ColorNode::new(
        &gdk::RGBA::RED,
        &graphene::Rect::new(0.0, 0.0, 200.0, 100.0),
    );
    assert_eq!(outline(flat.upcast_ref(), 200.0, 100.0), None);
}

#[test]
fn an_arrow_narrows_toward_its_tip_on_every_side() {
    let flat = Rect {
        x: 0,
        y: 0,
        width: 20,
        height: 10,
    };
    let tall = Rect {
        x: 0,
        y: 0,
        width: 10,
        height: 20,
    };

    let up = arrow(flat, Tip::Up);
    assert!(covers(&up, 10, 0) && !covers(&up, 0, 0) && covers(&up, 0, 9));

    let down = arrow(flat, Tip::Down);
    assert!(covers(&down, 10, 9) && !covers(&down, 0, 9) && covers(&down, 0, 0));

    let left = arrow(tall, Tip::Left);
    assert!(covers(&left, 0, 10) && !covers(&left, 0, 0) && covers(&left, 9, 0));

    let right = arrow(tall, Tip::Right);
    assert!(covers(&right, 9, 10) && !covers(&right, 9, 0) && covers(&right, 0, 0));
}

#[test]
fn an_arrow_is_about_half_its_box() {
    let flat = Rect {
        x: 0,
        y: 0,
        width: 20,
        height: 10,
    };
    for tip in [Tip::Up, Tip::Down] {
        let covered = area(&arrow(flat, tip));
        assert!((90..=110).contains(&covered), "{tip:?} covers {covered}");
    }
}
