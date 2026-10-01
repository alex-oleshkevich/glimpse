use super::*;

fn rgbs(palette: &Palette) -> Vec<[u8; 3]> {
    palette.colors().iter().map(|color| color.rgb).collect()
}

#[test]
fn the_newest_pick_comes_first() {
    let mut palette = Palette::new(8);
    palette.push([1, 1, 1]);
    palette.push([2, 2, 2]);

    assert_eq!(rgbs(&palette), [[2, 2, 2], [1, 1, 1]]);
}

#[test]
fn picking_a_color_again_moves_it_to_the_front_and_keeps_its_id() {
    let mut palette = Palette::new(8);
    let first = palette.push([1, 1, 1]);
    palette.push([2, 2, 2]);
    let again = palette.push([1, 1, 1]);

    assert_eq!(rgbs(&palette), [[1, 1, 1], [2, 2, 2]]);
    assert_eq!(again.id, first.id);
}

#[test]
fn a_pick_beyond_the_limit_drops_the_oldest() {
    let mut palette = Palette::new(2);
    palette.push([1, 1, 1]);
    palette.push([2, 2, 2]);
    palette.push([3, 3, 3]);

    assert_eq!(rgbs(&palette), [[3, 3, 3], [2, 2, 2]]);
}

#[test]
fn a_dropped_color_is_not_found_and_its_id_is_not_reused() {
    let mut palette = Palette::new(1);
    let dropped = palette.push([1, 1, 1]);
    let kept = palette.push([2, 2, 2]);

    assert_eq!(palette.get(dropped.id), None);
    assert_eq!(palette.get(kept.id), Some(kept));
    assert_ne!(kept.id, dropped.id);
}

#[test]
fn the_limit_is_clamped() {
    assert_eq!(Palette::new(0).limit, 1);
    assert_eq!(Palette::new(1000).limit, MAX_LIMIT);
}
