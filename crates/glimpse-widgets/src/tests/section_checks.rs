use super::*;

pub(super) fn check() {
    let section = Section::new();
    let header = child_named::<gtk4::Box>(&section, "section__header");
    let section_count = child_named::<gtk4::Label>(&section, "section__count");
    let section_content = child_named::<gtk4::Box>(&section, "section__content");
    let section_placeholder = child_named::<gtk4::Box>(&section, "section__placeholder");

    assert!(
        !header.get_visible(),
        "an untitled section spends no height on an empty header"
    );
    section.set_title(Some("Today"));
    section.set_count(Some("3"));
    assert!(header.get_visible() && section_count.get_visible());
    assert!(section_content.get_visible() && !section_placeholder.get_visible());

    section.set_empty(true);
    assert!(
        !section_content.get_visible() && section_placeholder.get_visible(),
        "an empty section swaps its content for the placeholder rather than stacking both"
    );
    assert!(
        !section_count.get_visible(),
        "a count of nothing beside an empty state says the same thing twice"
    );
    assert_eq!(
        section.count().as_deref(),
        Some("3"),
        "the count is hidden, not forgotten, so restoring content restores it"
    );
    section.set_empty(false);
    assert!(section_count.get_visible());
    assert_eq!(
        *section.imp().accessible_name.borrow(),
        "Today 3",
        "the count is information, not decoration, so it reaches a screen reader even though \
             the label that draws it is marked presentational"
    );
    section.set_empty(true);
    assert_eq!(
        *section.imp().accessible_name.borrow(),
        "Today",
        "and goes away with it, rather than announcing three of nothing"
    );
    section.set_empty(false);

    let first_body = gtk4::Label::new(Some("body"));
    section.set_content(Some(&first_body));
    section.set_content(Some(&gtk4::Label::new(Some("body again"))));
    assert!(
        first_body.parent().is_none(),
        "content is one child, so a second setter unparents the first"
    );
}
