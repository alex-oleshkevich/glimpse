use std::cell::Cell;

use super::active;

#[test]
fn blur_needs_every_condition() {
    for enabled in [false, true] {
        for gtk_ok in [false, true] {
            for compositor in [false, true] {
                assert_eq!(
                    active(enabled, gtk_ok, || compositor),
                    enabled && gtk_ok && compositor,
                    "enabled={enabled} gtk_ok={gtk_ok} compositor={compositor}"
                );
            }
        }
    }
}

#[test]
fn the_compositor_is_asked_only_when_everything_else_allows_blur() {
    for (enabled, gtk_ok) in [(false, false), (false, true), (true, false)] {
        let asked = Cell::new(false);
        active(enabled, gtk_ok, || {
            asked.set(true);
            true
        });
        assert!(!asked.get(), "enabled={enabled} gtk_ok={gtk_ok}");
    }

    let asked = Cell::new(false);
    active(true, true, || {
        asked.set(true);
        true
    });
    assert!(asked.get());
}
