use super::*;

pub(super) fn check() {
    let exec = PopoverShell::new();
    assert!(!exec.imp().scroller.get_visible());
    let body = exec.body();
    body.append(&gtk4::Label::new(Some("body 1")));
    body.set_visible(true);
    assert!(
        exec.imp().scroller.get_visible(),
        "a body filled through body() shows its scroller"
    );

    exec.set_title(Some("ж".repeat(10_000)));
    assert!(exec.imp().header.get_visible());
    let title = exec.title().unwrap_or_default();
    assert!((1..=crate::TEXT_MAX_CHARS).contains(&title.chars().count()));

    let late = PopoverShell::new();
    assert!(!late.imp().body.get_visible());
    late.body().append(&gtk4::Label::new(Some("late")));
    assert!(
        late.imp().body.get_visible(),
        "a child appended to body() after construction shows the body"
    );
}
