mod hero;
mod indicator;
mod indicator_group;
mod panel;
mod popover_shell;
mod row;
mod theme;

pub use hero::Hero;
pub use indicator::{Indicator, IndicatorSpec};
pub use indicator_group::IndicatorGroup;
pub use panel::Panel;
pub use popover_shell::PopoverShell;
pub use row::Row;
pub use theme::Styles;

#[cfg(test)]
use indicator::{LABEL_MAX_CHARS, TOOLTIP_MAX_CHARS};

pub(crate) const TEXT_MAX_CHARS: usize = 128;

pub(crate) fn clear_children(container: &gtk4::Box) {
    use gtk4::prelude::*;

    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

pub(crate) fn set_text(label: &gtk4::Label, value: Option<&str>) {
    use gtk4::prelude::*;

    let text = indicator::truncate(value.unwrap_or_default(), TEXT_MAX_CHARS);
    if label.text().as_str() == text {
        return;
    }
    label.set_text(&text);
    label.set_visible(!text.is_empty());
}

pub(crate) fn set_css_class(widget: &impl gtk4::prelude::IsA<gtk4::Widget>, name: &str, on: bool) {
    use gtk4::prelude::*;

    match on {
        true => widget.add_css_class(name),
        false => widget.remove_css_class(name),
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

        let row = Row::new();
        let check = child_named::<gtk4::Image>(&row, "row__check");
        let row_title = child_named::<gtk4::Label>(&row, "row__title");
        let row_subtitle = child_named::<gtk4::Label>(&row, "row__subtitle");

        assert!(
            !check.is_visible(),
            "a row that cannot be selected spends no width on the column"
        );
        row.set_selectable(true);
        assert!(
            check.is_visible() && check.icon_name().is_none(),
            "a selectable row reserves the column before anything is selected, so a later \
             selection does not shift the label"
        );
        row.set_selected(true);
        assert_eq!(check.icon_name().as_deref(), Some("object-select-symbolic"));
        assert!(row.has_css_class("row--on"));
        row.set_selected(false);
        assert!(check.icon_name().is_none() && !row.has_css_class("row--on"));

        assert!(!row_subtitle.is_visible());
        row.set_title(Some("Tenda_4A21F0"));
        row.set_subtitle(Some("WPA2 · 5 GHz"));
        assert!(row_subtitle.is_visible());
        assert!(
            row.has_css_class("row--two"),
            "a subtitle is what makes a row two lines; nothing else has to be told"
        );
        row.set_subtitle(None::<&str>);
        assert!(!row.has_css_class("row--two"));

        row.set_title(Some("ё".repeat(TEXT_MAX_CHARS * 2)));
        assert_eq!(
            row_title.text().chars().count(),
            TEXT_MAX_CHARS,
            "an SSID is another application's string: capped without slicing a character"
        );

        let signal = gtk4::Image::from_icon_name("network-wireless-symbolic");
        let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
        row.set_lead(&signal);
        row.set_trail(&chevron);
        assert_eq!(
            signal.parent().and_then(|slot| slot.parent()),
            chevron.parent().and_then(|slot| slot.parent()),
            "both slots hang off the same row"
        );
        row.clear_trail();
        assert!(chevron.parent().is_none());
        assert!(
            signal.parent().is_some(),
            "clearing one slot leaves the other alone"
        );

        row.set_title(Some("W".repeat(40)));
        let wide = row.measure(gtk4::Orientation::Horizontal, -1).1;
        row.set_title(Some("W".repeat(120)));
        assert_eq!(
            row.measure(gtk4::Orientation::Horizontal, -1).1,
            wide,
            "past the cap a longer title asks for no more width, so an SSID cannot widen the \
             popover it sits in. `ellipsize` alone does not do this — it lowers the minimum \
             width and leaves the natural width at the full string."
        );

        assert!(row.can_target() && row.activatable());
        row.set_activatable(false);
        assert!(
            !row.can_target() && !row.can_focus(),
            "a row that does nothing takes neither the pointer nor the focus, so it cannot \
             light up under a hover that leads nowhere"
        );
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
