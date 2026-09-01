mod hero;
mod indicator;
mod indicator_group;
mod panel;
mod popover_shell;
mod theme;

pub use hero::Hero;
pub use indicator::{Indicator, IndicatorSpec};
pub use indicator_group::IndicatorGroup;
pub use panel::Panel;
pub use popover_shell::PopoverShell;
pub use theme::Styles;

#[cfg(test)]
use hero::TEXT_MAX_CHARS;
#[cfg(test)]
use indicator::{LABEL_MAX_CHARS, TOOLTIP_MAX_CHARS};

pub(crate) fn clear_children(container: &gtk4::Box) {
    use gtk4::prelude::*;

    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

pub fn register_resources() -> Result<(), glib::Error> {
    gio::resources_register_include!("glimpse-panel.gresource")
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    fn spec(label: &str) -> IndicatorSpec {
        IndicatorSpec {
            label: Some(label.to_owned()),
            ..Default::default()
        }
    }

    fn child_at(group: &IndicatorGroup, index: usize) -> Indicator {
        let mut child = group.first_child();
        for _ in 0..index {
            child = child.and_then(|widget| widget.next_sibling());
        }
        child
            .and_downcast::<Indicator>()
            .unwrap_or_else(|| panic!("no indicator at {index}"))
    }

    fn label_widget(indicator: &Indicator) -> gtk4::Label {
        indicator
            .first_child()
            .and_then(|icon| icon.next_sibling())
            .and_downcast::<gtk4::Label>()
            .expect("label child")
    }

    fn label_of(indicator: &Indicator) -> String {
        label_widget(indicator).text().to_string()
    }

    fn labels(group: &IndicatorGroup) -> Vec<String> {
        let mut out = Vec::new();
        let mut child = group.first_child();
        while let Some(widget) = child {
            let indicator = widget.clone().downcast::<Indicator>().expect("indicator");
            out.push(label_of(&indicator));
            child = widget.next_sibling();
        }
        out
    }

    #[test]
    #[ignore = "needs a display"]
    fn widgets() {
        if gtk4::init().is_err() {
            return;
        }
        register_resources().expect("resources");

        let group = IndicatorGroup::new();
        assert!(!group.is_visible(), "an untouched group starts hidden");

        group.set_orientation(gtk4::Orientation::Vertical);
        assert_eq!(
            group
                .layout_manager()
                .and_downcast::<gtk4::BoxLayout>()
                .expect("box layout")
                .orientation(),
            gtk4::Orientation::Vertical,
            "a group follows the bar it sits in"
        );
        group.set_orientation(gtk4::Orientation::Horizontal);

        group.set_items(&[spec("a"), spec("b"), spec("c")]);
        assert_eq!(labels(&group), ["a", "b", "c"]);
        assert!(group.is_visible());

        let first = child_at(&group, 0);
        group.set_items(&[spec("c"), spec("a")]);
        assert_eq!(labels(&group), ["c", "a"]);
        assert_eq!(
            child_at(&group, 0),
            first,
            "a position reuses its widget rather than rebuilding it"
        );

        let pressed = Rc::new(RefCell::new(Vec::new()));
        group.connect_pressed({
            let pressed = Rc::clone(&pressed);
            move |_, button| pressed.borrow_mut().push(button)
        });
        let scrolled = Rc::new(RefCell::new(Vec::new()));
        group.connect_scrolled({
            let scrolled = Rc::clone(&scrolled);
            move |_, dx, dy| scrolled.borrow_mut().push((dx, dy))
        });

        group.emit_by_name::<()>("pressed", &[&3u32]);
        group.emit_by_name::<()>("scrolled", &[&1.0f64, &-2.0f64]);
        assert_eq!(
            *pressed.borrow(),
            [3u32],
            "the whole group reports the press, exactly once"
        );
        assert_eq!(*scrolled.borrow(), [(1.0f64, -2.0f64)]);

        assert_eq!(
            child_at(&group, 0).observe_controllers().n_items(),
            0,
            "a chip owns no input controller; the group is the one clickable thing"
        );
        assert_eq!(
            group.observe_controllers().n_items(),
            3,
            "the group owns the click, scroll and key controllers"
        );

        group.set_items(&[spec("a"), spec("b")]);
        assert_eq!(
            *group.imp().accessible_name.borrow(),
            "a b",
            "the interactive element is named after everything it shows"
        );
        let hostile = "п".repeat(LABEL_MAX_CHARS * 2);
        group.set_items(&[IndicatorSpec {
            label: Some(hostile),
            ..Default::default()
        }]);
        assert_eq!(
            group.imp().accessible_name.borrow().chars().count(),
            LABEL_MAX_CHARS,
            "an unbounded label is capped before it reaches the accessible name"
        );

        group.set_items(&[]);
        assert!(group.first_child().is_none());
        assert!(!group.is_visible(), "an empty group hides itself");
        assert!(
            group.imp().accessible_name.borrow().is_empty(),
            "an empty group carries no stale name"
        );

        let long = "ы".repeat(LABEL_MAX_CHARS * 2);
        let indicator = Indicator::new();
        indicator.set_label(Some(&long));
        assert_eq!(label_of(&indicator).chars().count(), LABEL_MAX_CHARS);
        let label = label_widget(&indicator);
        assert!(label.is_visible());
        indicator.set_label(None);
        assert!(!label.is_visible(), "an emptied label reserves no space");

        let image = indicator
            .first_child()
            .and_downcast::<gtk4::Image>()
            .expect("icon child");
        assert!(
            !image.is_visible(),
            "an indicator with no icon reserves no icon space"
        );

        let changes = Rc::new(Cell::new(0u32));
        image.connect_gicon_notify({
            let changes = Rc::clone(&changes);
            move |_| changes.set(changes.get() + 1)
        });

        indicator.set_icon(Some(&gio::ThemedIcon::new("audio-volume-high").upcast()));
        assert_eq!(changes.get(), 1);
        indicator.set_icon(Some(&gio::ThemedIcon::new("audio-volume-high").upcast()));
        assert_eq!(changes.get(), 1, "an equal icon is not reapplied");
        indicator.set_icon(Some(&gio::ThemedIcon::new("audio-volume-low").upcast()));
        assert_eq!(changes.get(), 2);
        assert!(image.is_visible());

        indicator.set_icon(None);
        assert!(!image.is_visible());

        indicator.apply(&IndicatorSpec {
            tooltip: Some("п".repeat(TOOLTIP_MAX_CHARS * 2)),
            ..Default::default()
        });
        assert_eq!(
            indicator.tooltip_text().unwrap_or_default().chars().count(),
            TOOLTIP_MAX_CHARS,
            "an unbounded tooltip from another application is capped"
        );

        let hero = Hero::new();
        let title = child_named::<gtk4::Label>(&hero, "hero__title");
        let subtitle = child_named::<gtk4::Label>(&hero, "hero__subtitle");
        assert!(!title.is_visible() && !subtitle.is_visible());

        hero.set_title(Some("Wi-Fi"));
        assert!(title.is_visible());
        hero.set_subtitle(Some("Connected"));
        assert_eq!(subtitle.text(), "Connected");

        hero.set_title(Some("ё".repeat(TEXT_MAX_CHARS * 2)));
        assert_eq!(
            title.text().chars().count(),
            TEXT_MAX_CHARS,
            "an unbounded title is capped without slicing a multi-byte character"
        );

        let hero_icon = child_named::<gtk4::Image>(&hero, "hero__icon");
        let icon_changes = Rc::new(Cell::new(0u32));
        hero_icon.connect_gicon_notify({
            let icon_changes = Rc::clone(&icon_changes);
            move |_| icon_changes.set(icon_changes.get() + 1)
        });
        hero.set_icon(Some(&gio::ThemedIcon::new("network-wireless").upcast()));
        hero.set_icon(Some(&gio::ThemedIcon::new("network-wireless").upcast()));
        assert_eq!(icon_changes.get(), 1, "an equal icon is not reapplied");

        let switch = gtk4::Switch::new();
        hero.set_slot(&switch);
        assert_eq!(
            switch.parent().and_then(|slot| slot.parent()),
            Some(hero.clone().upcast())
        );
        hero.clear_slot();
        assert!(
            switch.parent().is_none(),
            "a cleared slot unparents its child"
        );

        let shell = PopoverShell::new();
        let hero_box = child_named::<gtk4::Box>(&shell, "popover-shell__hero");
        let footer_box = child_named::<gtk4::Box>(&shell, "popover-shell__footer");
        let rules: Vec<gtk4::Separator> = children_of(&shell);
        assert_eq!(
            rules.len(),
            2,
            "one hairline above the footer, one below the hero"
        );
        assert!(
            !hero_box.is_visible() && !rules[0].is_visible(),
            "an absent hero leaves neither space nor a stray hairline"
        );
        assert!(!footer_box.is_visible() && !rules[1].is_visible());

        shell.set_hero(&hero);
        assert!(hero_box.is_visible() && rules[0].is_visible());
        shell.clear_hero();
        assert!(
            !hero_box.is_visible() && !rules[0].is_visible(),
            "the hairline goes back with the section it belongs to"
        );
        assert!(
            hero.parent().is_none(),
            "a cleared hero unparents its widget"
        );

        let plain = gtk4::Label::new(Some("a hero the shell has never heard of"));
        shell.set_hero(&plain);
        assert!(
            hero_box.is_visible(),
            "any widget is a hero; the shell does not require its own type"
        );

        let content = gtk4::Label::new(Some("body"));
        shell.set_content(&content);
        let replacement = gtk4::Label::new(Some("body again"));
        shell.set_content(&replacement);
        assert!(
            content.parent().is_none(),
            "content is one child, so a second setter unparents the first"
        );

        let button = gtk4::Button::new();
        shell.append_to_footer(&button);
        assert!(footer_box.is_visible() && rules[1].is_visible());
        shell.clear_footer();
        assert!(!footer_box.is_visible() && !rules[1].is_visible());
        assert!(button.parent().is_none());
    }

    fn children_of<T: IsA<gtk4::Widget>>(parent: &impl IsA<gtk4::Widget>) -> Vec<T> {
        let mut found = Vec::new();
        let mut child = parent.as_ref().first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(widget) = widget.downcast::<T>() {
                found.push(widget);
            }
        }
        found
    }

    fn child_named<T: IsA<gtk4::Widget>>(parent: &impl IsA<gtk4::Widget>, class: &str) -> T {
        fn find(widget: &gtk4::Widget, class: &str) -> Option<gtk4::Widget> {
            if widget.has_css_class(class) {
                return Some(widget.clone());
            }
            let mut child = widget.first_child();
            while let Some(candidate) = child {
                if let Some(found) = find(&candidate, class) {
                    return Some(found);
                }
                child = candidate.next_sibling();
            }
            None
        }

        find(parent.as_ref(), class)
            .and_downcast::<T>()
            .unwrap_or_else(|| panic!("no {class} below the widget"))
    }
}
