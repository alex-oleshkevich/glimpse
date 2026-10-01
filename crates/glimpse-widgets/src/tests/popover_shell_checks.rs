use super::*;

pub(super) fn check() {
    let bare = PopoverShell::new();
    let section = gtk4::Label::new(Some("Other players"));
    bare.set_hero(&gtk4::Label::new(Some("Now playing")));
    bare.set_content(&section);
    bare.append_to_footer(&Row::new());
    let rules = |shell: &PopoverShell| {
        (
            shell.imp().hero_rule.get_visible(),
            shell.imp().footer_rule.get_visible(),
        )
    };
    assert_eq!(rules(&bare), (true, true));
    section.set_visible(false);
    assert_eq!(
        rules(&bare),
        (true, false),
        "an empty content slot leaves one rule between hero and footer, not two"
    );
    assert!(!bare.imp().content_box.get_visible());
    section.set_visible(true);
    assert_eq!(rules(&bare), (true, true));
}
