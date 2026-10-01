use super::*;

pub(super) fn check() {
    let keyboard = KeyboardPopover::new();
    let layout = |name: &str, code: &str, active: bool| KeyboardLayout {
        name: name.to_owned(),
        code: code.to_owned(),
        active,
    };
    keyboard.set_layouts(&[
        layout("English (US)", "US", true),
        layout("Russian", "RU", false),
    ]);
    keyboard.set_layouts(&[
        layout("English (US)", "US", true),
        layout("Russian", "RU", false),
    ]);
    assert_eq!(
        child_named::<Hero>(&keyboard, "hero").title().as_deref(),
        Some("Keyboard"),
        "the keyboard popover opens under a header like every other"
    );
}
