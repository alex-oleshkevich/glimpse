use super::*;

pub(super) fn check() {
    let bar = RangeBar::new();
    bar.set_scale(7.0, 26.0);
    bar.set_range(12.0, 18.0);
    assert_eq!(bar.range(), (12.0, 18.0));
    assert_eq!(bar.scale(), (7.0, 26.0));
    bar.set_range(20.0, 5.0);
    assert_eq!(
        bar.range(),
        (20.0, 20.0),
        "a high below its low is clamped rather than drawn backwards"
    );
    assert_eq!(
        bar.now(),
        None,
        "a bar marks no reading until it is given one"
    );
    bar.set_now(Some(16.0));
    assert_eq!(bar.now(), Some(16.0));
}
