mod artwork;
mod calendar;
mod calendar_popover;
mod choice_list;
mod dots;
pub mod drawer;
mod event_list;
mod fact_list;
mod forecast;
mod hero;
mod indicator;
mod indicator_group;
mod keyboard_popover;
mod mpris_popover;
mod next_event_popover;
mod notice;
mod notification_card;
mod notification_header;
mod notification_image_body;
mod notification_list;
mod notification_stack;
mod notification_text_body;
mod notifications_popover;
mod now_playing;
mod pager;
mod panel;
mod placeholder;
mod player_list;
mod popover_shell;
mod range_bar;
mod readout;
mod reconcile;
pub(crate) mod row;
mod scrubber;
mod section;
mod split_row;
mod theme;
mod tooltip_card;
mod transport;
mod tray_strip;
mod weather_popover;
mod workspace_list;
mod workspace_section;
mod workspaces_popover;
mod world_clock;

pub use artwork::artwork;
pub use calendar::{Calendar, Ymd};
pub use calendar_popover::CalendarPopover;
pub use choice_list::{Choice, ChoiceList};
pub use event_list::{Event, EventList, EventRow};
pub use fact_list::{Fact, FactList};
pub use forecast::{Day, ForecastDay, ForecastHour, ForecastList, ForecastStrip, Hour};
pub use hero::Hero;
pub use indicator::{Indicator, IndicatorSpec};
pub use indicator_group::IndicatorGroup;
pub use keyboard_popover::{KeyboardPopover, Layout as KeyboardLayout};
pub use mpris_popover::MprisPopover;
pub use next_event_popover::NextEventPopover;
pub use notice::{Notice, Severity};
pub use notification_card::{Action, NotificationCard, Urgency};
pub use notification_header::NotificationHeader;
pub use notification_image_body::{NotificationImageBody, notification_image};
pub use notification_list::{Body, Notification, NotificationList};
pub use notification_stack::NotificationStack;
pub use notification_text_body::NotificationTextBody;
pub use notifications_popover::{Group, NotificationsPopover};
pub use now_playing::NowPlaying;
pub use pager::{Focus, Pager, PagerItem, Shape, Slot};
pub use panel::Panel;
pub use placeholder::Placeholder;
pub use player_list::{Player, PlayerList, PlayerRow};
pub use popover_shell::PopoverShell;
pub use range_bar::RangeBar;
pub use readout::Readout;
pub use row::Row;
pub use scrubber::{Scrubber, clock};
pub use section::Section;
pub use split_row::SplitRow;
pub use theme::Styles;
pub use tooltip_card::TooltipCard;
pub use transport::{Repeat, Transport, TransportAction};
pub use tray_strip::{Edge, TrayChip, TrayStrip};
pub use weather_popover::{Advisory, Page as WeatherPage, WeatherPopover, alert_page, day_page};
pub use workspace_list::{Window as WorkspaceWindow, Workspace, WorkspaceList};
pub use workspace_section::WorkspaceSection;
pub use workspaces_popover::WorkspacesPopover;
pub use world_clock::{ClockRow, WorldClock, Zone};

#[cfg(test)]
use indicator::{LABEL_MAX_CHARS, TOOLTIP_MAX_CHARS};

pub(crate) const TEXT_MAX_CHARS: usize = 128;

pub(crate) fn clear_children(container: &gtk4::Box) {
    use gtk4::prelude::*;

    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

pub(crate) use indicator::truncate;

pub(crate) fn fill_slot(slot: &gtk4::Box, widget: &impl gtk4::prelude::IsA<gtk4::Widget>) {
    use gtk4::prelude::*;

    if slot.first_child().as_ref() == Some(widget.as_ref()) {
        return;
    }
    clear_children(slot);
    slot.append(widget);
}

pub(crate) fn set_footer_row(row: &crate::Row, label: Option<&str>) {
    use gtk4::prelude::*;

    row.set_visible(label.is_some());
    row.set_title(label);
    row.set_activatable(label.is_some());
}

pub(crate) fn set_text(label: &gtk4::Label, value: Option<&str>) {
    use gtk4::prelude::*;

    let text = truncate(value.unwrap_or_default(), TEXT_MAX_CHARS);
    if label.text().as_str() == text {
        return;
    }
    label.set_text(&text);
    label.set_visible(!text.is_empty());
}

pub(crate) fn set_play_pause(button: &gtk4::Button, playing: bool) {
    use gettextrs::gettext;
    use gtk4::prelude::*;

    let (icon, tooltip) = match playing {
        true => ("media-playback-pause-symbolic", gettext("Pause")),
        false => ("media-playback-start-symbolic", gettext("Play")),
    };
    button.set_icon_name(icon);
    button.set_tooltip_text(Some(tooltip.as_str()));
}

pub(crate) fn set_css_class(widget: &impl gtk4::prelude::IsA<gtk4::Widget>, name: &str, on: bool) {
    use gtk4::prelude::*;

    match on {
        true => widget.add_css_class(name),
        false => widget.remove_css_class(name),
    }
}

pub(crate) fn icons_equal(current: Option<&gio::Icon>, next: Option<&gio::Icon>) -> bool {
    use gtk4::prelude::*;

    match (current, next) {
        (None, None) => true,
        (Some(current), Some(next)) => current.equal(Some(next)),
        _ => false,
    }
}

pub(crate) fn none_if_empty(text: &str) -> Option<&str> {
    (!text.is_empty()).then_some(text)
}

pub fn register_resources() -> Result<(), glib::Error> {
    gio::resources_register_include!("glimpse-widgets.gresource")
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::gdk;
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    #[test]
    fn an_absent_field_is_not_an_empty_one() {
        assert_eq!(none_if_empty(""), None);
        assert_eq!(none_if_empty("Marta Kaz"), Some("Marta Kaz"));
    }

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
        child_named::<gtk4::Label>(indicator, "indicator__label")
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

    fn secondary_click(widget: &impl IsA<gtk4::Widget>) {
        let controllers = widget.observe_controllers();
        let click = (0..controllers.n_items())
            .filter_map(|index| controllers.item(index))
            .filter_map(|controller| controller.downcast::<gtk4::GestureClick>().ok())
            .find(|gesture| gesture.button() == gdk::BUTTON_SECONDARY)
            .expect("a secondary-click gesture");
        click.emit_by_name::<()>("released", &[&1i32, &0.0f64, &0.0f64]);
    }

    #[test]
    #[ignore = "needs a display"]
    fn widgets() {
        if gtk4::init().is_err() {
            return;
        }
        register_resources().expect("resources");
        let _styles = Styles::install(adw::ColorScheme::Default);

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

        let image = child_named::<gtk4::Image>(&indicator, "indicator__icon");
        assert!(
            !image.is_visible(),
            "an indicator with no icon reserves no icon space"
        );

        let dot = child_named::<crate::dots::Dots>(&indicator, "indicator__dot");
        assert!(
            !dot.is_visible(),
            "an indicator with no calendar behind it reserves no dot space"
        );
        indicator.set_dot(Some(gtk4::gdk::RGBA::new(1.0, 0.0, 0.0, 1.0)));
        assert!(dot.is_visible());
        indicator.set_dot(None);
        assert!(!dot.is_visible());

        let attention_dot = child_named::<gtk4::Box>(&indicator, "indicator__attention-dot");
        let badge = child_named::<gtk4::Label>(&indicator, "indicator__badge");
        assert_eq!(attention_dot.halign(), gtk4::Align::Center);
        assert_eq!(attention_dot.valign(), gtk4::Align::Center);
        indicator.apply(&IndicatorSpec {
            attention: true,
            ..Default::default()
        });
        assert!(attention_dot.is_visible() && !badge.is_visible());
        indicator.apply(&IndicatorSpec::default());
        indicator.apply(&IndicatorSpec {
            badge: Some("3".to_owned()),
            attention: true,
            ..Default::default()
        });
        assert!(!attention_dot.is_visible() && badge.is_visible());
        assert!(
            indicator.has_css_class("indicator--attention"),
            "a counter hides the dot but must not cancel attention, which the class carries"
        );
        indicator.apply(&IndicatorSpec::default());
        assert!(!attention_dot.is_visible() && !badge.is_visible());
        assert!(!indicator.has_css_class("indicator--attention"));

        let overlay = child_named::<gtk4::Image>(&indicator, "indicator__overlay");
        assert!(
            !overlay.get_visible(),
            "an indicator with no overlay reserves no corner"
        );
        indicator.apply(&IndicatorSpec {
            icon: Some(gio::ThemedIcon::new("folder-symbolic").upcast()),
            overlay: Some(gio::ThemedIcon::new("emblem-synchronizing-symbolic").upcast()),
            ..Default::default()
        });
        assert!(overlay.get_visible(), "an overlay shows over the base icon");
        indicator.set_overlay(None);
        assert!(!overlay.get_visible());

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

        let row_icon = child_named::<gtk4::Image>(&row, "row__icon");
        let row_value = child_named::<gtk4::Label>(&row, "row__value");
        assert!(
            !row_icon.get_visible() && !row_value.get_visible(),
            "a row with neither reserves space for neither"
        );
        row.set_lead_icon(Some("network-wireless-symbolic"));
        row.set_value(Some("WPA3"));
        assert!(row_icon.get_visible() && row_value.get_visible());

        let icon_changes = Rc::new(Cell::new(0u32));
        row_icon.connect_icon_name_notify({
            let icon_changes = Rc::clone(&icon_changes);
            move |_| icon_changes.set(icon_changes.get() + 1)
        });
        row.set_lead_icon(Some("network-wireless-symbolic"));
        assert_eq!(
            icon_changes.get(),
            0,
            "an equal icon name is not reapplied, so a list re-rendering itself restyles nothing"
        );

        row.set_value(Some("ё".repeat(TEXT_MAX_CHARS * 2)));
        assert_eq!(
            row_value.text().chars().count(),
            TEXT_MAX_CHARS,
            "a value is another application's string too"
        );
        row.set_value(None::<&str>);
        assert!(
            !row_value.get_visible(),
            "a cleared value gives its width back rather than leaving a gap before the chevron"
        );
        row.set_lead_icon(None::<&str>);

        let signal = gtk4::Image::from_icon_name("network-wireless-symbolic");
        let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
        let reparented = Rc::new(Cell::new(0u32));
        signal.connect_parent_notify({
            let reparented = Rc::clone(&reparented);
            move |_| reparented.set(reparented.get() + 1)
        });
        row.set_lead(&signal);
        row.set_lead(&signal);
        assert_eq!(
            reparented.get(),
            1,
            "filling a slot with the widget already in it does not unparent and reparent it, \
             which is what a list re-rendering the same rows would otherwise do every update"
        );
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

        let calendar = Calendar::new();
        calendar.set_today(Ymd::new(2026, 9, 23));
        calendar.show_month(2026, 9);
        let today_button = child_named::<gtk4::Button>(&calendar, "calendar__today");
        assert!(
            !today_button.is_visible(),
            "Today is meaningless while today's month is shown and nothing else is selected"
        );

        calendar.select(Ymd::new(2026, 9, 17));
        assert!(
            today_button.is_visible(),
            "the way back has to be offered whenever the selection is not today, even on today's \
             own month"
        );
        today_button.emit_clicked();
        assert_eq!(calendar.selected(), Some(Ymd::new(2026, 9, 23)));
        assert!(
            !today_button.is_visible(),
            "taking the way back leaves nothing to go back to"
        );

        calendar.step(1);
        assert_eq!(calendar.shown(), (2026, 10));
        assert!(today_button.is_visible());
        calendar.step(-4);
        assert_eq!(
            calendar.shown(),
            (2026, 6),
            "stepping crosses months, not weeks"
        );

        let chosen = Rc::new(RefCell::new(Vec::new()));
        calendar.connect_day_selected({
            let chosen = Rc::clone(&chosen);
            move |_, date| chosen.borrow_mut().push(date)
        });
        calendar.select(Ymd::new(2026, 6, 4));
        assert_eq!(calendar.selected(), Some(Ymd::new(2026, 6, 4)));
        assert_eq!(
            chosen.borrow().as_slice(),
            &[Ymd::new(2026, 6, 4)],
            "selecting a day reports it once, with the day it was given"
        );

        let scope_button = child_named::<gtk4::Button>(&calendar, "calendar__scope");
        let title = child_named::<gtk4::Label>(&calendar, "calendar__title");
        scope_button.emit_clicked();
        assert_eq!(
            title.text().as_str(),
            "2026",
            "the title is the zoom control: clicking it widens the scope to the year"
        );
        let months = all_named(&calendar, "calendar__month");
        assert_eq!(months.len(), 12);
        assert!(
            months[5].has_css_class("calendar__cell--selected"),
            "the year view marks the month it was opened from, so widening the scope does not \
             lose where you were"
        );
        assert!(
            !months[0].has_css_class("calendar__cell--selected"),
            "and marks only that one"
        );

        scope_button.emit_clicked();
        assert_ne!(title.text().as_str(), "2026");

        let repeats = Rc::new(Cell::new(0));
        calendar.connect_day_selected({
            let repeats = Rc::clone(&repeats);
            move |calendar, date| {
                repeats.set(repeats.get() + 1);
                calendar.select(date);
            }
        });
        calendar.select(Ymd::new(2026, 6, 11));
        assert_eq!(
            repeats.get(),
            1,
            "selecting the day that is already selected reports nothing, so a handler that \
             reselects in response does not drive the signal round for ever"
        );

        calendar.clear_selection();
        assert_eq!(calendar.selected(), None);

        let red = gtk4::gdk::RGBA::new(1.0, 0.0, 0.0, 1.0);
        calendar.set_events(&[(Ymd::new(2026, 6, 4), vec![red; 5])]);
        assert_eq!(
            calendar.events(Ymd::new(2026, 6, 4)).len(),
            3,
            "three dots is a cap, not a count: a fourth event adds nothing and shifts nothing"
        );

        let notice = Notice::new();
        let notice_icon = child_named::<gtk4::Image>(&notice, "notice__icon");
        let notice_chevron = child_named::<gtk4::Image>(&notice, "notice__chevron");
        assert!(
            !notice.can_target() && !notice_chevron.get_visible(),
            "a notice that only states something takes no click and promises none"
        );
        notice.set_activatable(true);
        assert!(
            notice.can_target() && notice_chevron.get_visible(),
            "and one that leads somewhere shows the chevron that says so"
        );

        notice.set_title(Some("Thunderstorm warning until 21:00"));
        notice.set_icon_name(Some("dialog-warning-symbolic"));
        assert_eq!(notice.severity(), Severity::Info);
        assert!(!notice.has_css_class("notice--warning"));
        notice.set_severity(Severity::Warning);
        assert!(notice.has_css_class("notice--warning") && !notice.has_css_class("notice--error"));
        notice.set_severity(Severity::Error);
        assert!(
            notice.has_css_class("notice--error") && !notice.has_css_class("notice--warning"),
            "severity is one state, not a set of flags that can disagree"
        );
        notice.set_severity(Severity::Info);
        assert!(!notice.has_css_class("notice--error"));

        let notice_changes = Rc::new(Cell::new(0u32));
        notice_icon.connect_icon_name_notify({
            let notice_changes = Rc::clone(&notice_changes);
            move |_| notice_changes.set(notice_changes.get() + 1)
        });
        notice.set_icon_name(Some("dialog-warning-symbolic"));
        assert_eq!(notice_changes.get(), 0, "an equal icon is not reapplied");

        let item = NotificationCard::new();
        let item_summary = child_named::<gtk4::Label>(&item, "notification__summary");
        let item_body = child_named::<gtk4::Label>(&item, "notification__body");
        let item_icon = child_named::<gtk4::Image>(&item, "notification__app-icon");
        let item_actions = child_named::<gtk4::Box>(&item, "notification__actions");
        let item_close = child_named::<gtk4::Button>(&item, "notification__close");
        assert!(
            !item_summary.get_visible()
                && !item_body.get_visible()
                && !item_icon.get_visible()
                && !item_actions.get_visible(),
            "a notification with no content reserves no content slot"
        );
        assert!(
            item_close.get_visible(),
            "every notification exposes its close control without waiting for unread state"
        );
        item.set_activatable(false);
        assert!(
            !item.is_focusable() && !item.has_css_class("notification--activatable"),
            "a card with no default action neither takes a click nor advertises one"
        );
        item.set_activatable(true);
        assert!(item.is_focusable() && item.has_css_class("notification--activatable"));

        let content = child_named::<gtk4::Box>(&item, "notification__content");
        let header = child_named::<NotificationHeader>(&item, "notification__header");
        let text = child_named::<NotificationTextBody>(&item, "notification__text-body");
        let image = child_named::<NotificationImageBody>(&item, "notification__image-body");
        let body_group = content.parent().expect("body group");
        assert!(
            text.parent().as_ref() == Some(content.upcast_ref())
                && image.parent().as_ref() == Some(&body_group)
                && body_group.parent() == header.parent(),
            "the text and optional image share a body row below the full-width header"
        );

        {
            let shot = NotificationCard::new();
            shot.set_summary(Some("Screenshot captured"));
            shot.set_body(Some("Saved to Pictures"));
            let shot_picture = child_named::<gtk4::Picture>(&shot, "notification__image");

            shot.set_image(Some(&texture(64, 64)));
            assert!(
                child_named::<gtk4::Label>(&shot, "notification__summary").get_visible()
                    && child_named::<gtk4::Label>(&shot, "notification__body").get_visible()
                    && shot_picture.get_visible(),
                "an image notification keeps its title and body beside the thumbnail"
            );
            let bounded = shot_picture.paintable().expect("an image");
            assert_eq!(
                (bounded.intrinsic_width(), bounded.intrinsic_height()),
                (64, 64),
                "the thumbnail occupies the former 64px media slot"
            );

            let same = texture(64, 64);
            shot.set_image(Some(&same));
            let first = shot_picture.paintable().expect("an image");
            shot.set_image(Some(&same));
            assert!(
                shot_picture.paintable().expect("an image").eq(&first),
                "bound builds a new texture every time it resamples, so an unchanged image must \
                 be compared on the source rather than on the result"
            );

            shot.set_image(Some(&texture(8000, 6000)));
            assert!(
                !shot.imp().image.get_visible()
                    && child_named::<gtk4::Label>(&shot, "notification__summary").get_visible(),
                "download copies the whole image, and the image is somebody else's: past the \
                 ceiling the thumbnail is dropped without hiding the notification title"
            );

            shot.set_image(None);
            assert!(
                !shot.imp().image.get_visible(),
                "a notification without an image leaves no thumbnail gap"
            );
        }

        let item_icon_changes = Rc::new(Cell::new(0u32));
        item.set_icon_name(Some("dialog-information-symbolic"));
        item_icon.connect_icon_name_notify({
            let item_icon_changes = Rc::clone(&item_icon_changes);
            move |_| item_icon_changes.set(item_icon_changes.get() + 1)
        });
        item.set_icon_name(Some("dialog-information-symbolic"));
        assert_eq!(item_icon_changes.get(), 0, "an equal icon is not reapplied");

        item.set_summary(Some("é".repeat(TEXT_MAX_CHARS * 2).as_str()));
        assert_eq!(
            item_summary.text().chars().count(),
            TEXT_MAX_CHARS,
            "a summary is another application's string, capped by character so it cannot be cut \
             mid-codepoint"
        );

        item.set_body_markup(Some("<b>Alice</b>\nHey there"));
        assert_eq!(
            *item_body.text(),
            *"Alice\nHey there",
            "markup pango accepts is rendered, not shown as tags"
        );

        item.set_body_markup(Some("AT&T"));
        assert_eq!(
            *item_body.text(),
            *"AT&T",
            "markup pango refuses still reads; without the gate GtkLabel renders an empty label \
             and the body disappears"
        );
        assert!(item_body.get_visible());

        item.set_body_markup(Some(
            r#"<b>Alice</b>&nbsp;<a href="https://x">said hello</a>"#,
        ));
        assert_eq!(
            *item_body.text(),
            *"Alice\u{a0}said hello",
            "a refused body reads as its text, not as its tags: handing the markup itself to \
             set_text shows the reader tag soup and looks like a broken application"
        );

        assert_eq!(
            crate::notification_text_body::plain("&whoops; &#9733; &#x2605; &amp;"),
            "&whoops; \u{2605} \u{2605} &",
            "an unrecognised reference is left as written rather than silently swallowed"
        );

        item.set_body_markup(Some("<b>bold</b>"));
        item.set_body(Some("bold"));
        assert!(
            !item_body.uses_markup(),
            "a body switching from markup to the same plain string reads as unchanged by text \
             alone, and short-circuiting there would leave the bold still applied"
        );

        item.set_body(Some(
            "a".repeat(crate::notification_text_body::BODY_MAX_CHARS * 2)
                .as_str(),
        ));
        assert_eq!(
            item_body.text().chars().count(),
            crate::notification_text_body::BODY_MAX_CHARS
        );

        assert_eq!(item.urgency(), Urgency::Normal);
        item.set_urgency(Urgency::Critical);
        assert_eq!(item.urgency(), Urgency::Critical);
        assert!(
            !item.has_css_class("notification--critical"),
            "urgency is behaviour, and a class no rule in the sheet ever matched was not appearance \
             either"
        );

        let spoken = NotificationCard::new();
        spoken.set_app_name(Some("Telegram"));
        spoken.set_summary(Some("Marta Kaz"));
        spoken.set_body(Some("Are we still on for 14:00?"));
        spoken.set_when(Some("2m"));
        assert_eq!(
            *spoken.imp().accessible_name.borrow(),
            "Telegram. Marta Kaz. Are we still on for 14:00?. 2m",
            "every leaf is presentation, so the whole card reaches a screen reader through the one \
             label the activatable child carries — and the sender and the age are part of it"
        );

        spoken.set_unread(true);
        assert_eq!(
            *spoken.imp().accessible_name.borrow(),
            "Unread. Telegram. Marta Kaz. Are we still on for 14:00?. 2m",
            "unread remains available to assistive technology without a visual adornment"
        );

        assert_eq!(
            *spoken.imp().dismiss_name.borrow(),
            "Dismiss Marta Kaz",
            "twenty cards otherwise give twenty tab stops that each read Dismiss and name nothing"
        );
        spoken.set_summary(None::<&str>);
        assert_eq!(
            *spoken.imp().dismiss_name.borrow(),
            "Dismiss",
            "with no summary to name there is nothing to interpolate"
        );

        let action = |key: &str| Action {
            key: key.to_owned(),
            label: key.to_owned(),
        };
        item.set_actions(&[
            action("reply"),
            action("mute"),
            action("open"),
            action("archive"),
            action("delete"),
        ]);
        let buttons = all_named(&item, "notification__action");
        assert_eq!(
            buttons.len(),
            3,
            "three is where the GNOME HIG and KDE both stop; a longer list grows the card sideways"
        );
        assert!(
            buttons.iter().all(|button| button.has_css_class("flat")),
            "every notification action uses the ghost-button treatment"
        );
        assert!(
            buttons
                .iter()
                .all(|button| !button.has_css_class("notification__action--primary")),
            "notification actions do not invent a visual priority the sender did not declare"
        );
        assert!(item_actions.get_visible());

        let fired = Rc::new(RefCell::new(Vec::<String>::new()));
        item.connect_activated({
            let fired = Rc::clone(&fired);
            move |_| fired.borrow_mut().push("activated".to_owned())
        });
        item.connect_dismissed({
            let fired = Rc::clone(&fired);
            move |_| fired.borrow_mut().push("dismissed".to_owned())
        });
        item.connect_action_invoked({
            let fired = Rc::clone(&fired);
            move |_, key| fired.borrow_mut().push(key)
        });

        item.emit_by_name::<()>("activated", &[]);
        buttons[1]
            .clone()
            .downcast::<gtk4::Button>()
            .expect("an action is a button")
            .emit_clicked();
        child_named::<gtk4::Button>(&item, "notification__close").emit_clicked();
        assert_eq!(
            *fired.borrow(),
            vec![
                "activated".to_owned(),
                "mute".to_owned(),
                "dismissed".to_owned()
            ],
            "a key is read back when the button fires, not captured when it was built"
        );

        let relabelled = [
            Action {
                key: "reply".to_owned(),
                label: "Reply now".to_owned(),
            },
            action("mute"),
            action("open"),
        ];
        item.set_actions(&relabelled);
        assert_eq!(
            all_named(&item, "notification__action")[0]
                .downcast_ref::<gtk4::Button>()
                .and_then(|button| button.label())
                .map(|label| label.to_string()),
            Some("Reply now".to_owned()),
            "a sender that relabels an action without changing its key must still redraw it"
        );

        item.set_actions(&[]);
        assert!(
            all_named(&item, "notification__action").is_empty() && !item_actions.get_visible(),
            "a notification with no actions leaves no strip behind"
        );

        let list = NotificationList::new();
        let note = |key: &str, summary: &str| Notification {
            key: key.to_owned(),
            summary: summary.to_owned(),
            when: "now".to_owned(),
            ..Notification::default()
        };
        let rows_of = |list: &NotificationList| children_of::<NotificationCard>(list);

        assert!(rows_of(&list).is_empty() && !list.get_visible());

        list.set_notifications(&[note("a", "First"), note("b", "Second")]);
        let rows = rows_of(&list);
        assert_eq!(rows.len(), 2);
        assert!(list.get_visible());
        assert_eq!(rows[0].summary().as_deref(), Some("First"));

        let first = rows[0].clone();
        list.set_notifications(&[note("b", "Second"), note("a", "First again")]);
        let reordered = rows_of(&list);
        assert_eq!(
            reordered.len(),
            2,
            "a reorder moves rows, it does not build more of them"
        );
        assert!(
            reordered[1] == first,
            "a notification that moves keeps its own widget: by_key matches on the key, so the \
             row carrying `a` follows it rather than staying where it was"
        );
        assert_eq!(reordered[1].summary().as_deref(), Some("First again"));
        assert_eq!(reordered[0].summary().as_deref(), Some("Second"));

        let fired = Rc::new(RefCell::new(Vec::<String>::new()));
        list.connect_activated({
            let fired = Rc::clone(&fired);
            move |_, key| fired.borrow_mut().push(format!("activated {key}"))
        });
        list.connect_dismissed({
            let fired = Rc::clone(&fired);
            move |_, key| fired.borrow_mut().push(format!("dismissed {key}"))
        });
        list.connect_action_invoked({
            let fired = Rc::clone(&fired);
            move |_, key, action| fired.borrow_mut().push(format!("{key}/{action}"))
        });

        reordered[1].emit_by_name::<()>("activated", &[]);
        child_named::<gtk4::Button>(&reordered[0], "notification__close").emit_clicked();
        assert_eq!(
            *fired.borrow(),
            vec!["activated a".to_owned(), "dismissed b".to_owned()],
            "the key a row reports is its own, whatever position it has ended up in"
        );

        let mut image_note = note("image", "Screenshot captured");
        image_note.image = Some(texture(64, 64));
        list.set_notifications(&[image_note]);
        let image_row = children_of::<NotificationCard>(&list)
            .into_iter()
            .next()
            .expect("an image notification builds a card");
        assert!(
            child_named::<NotificationImageBody>(&image_row, "notification__image-body")
                .get_visible(),
            "an image notification reveals the shared card's thumbnail slot"
        );
        image_row.emit_by_name::<()>("activated", &[]);
        child_named::<gtk4::Button>(&image_row, "notification__close").emit_clicked();
        assert_eq!(
            &fired.borrow()[2..],
            ["activated image", "dismissed image"],
            "an image notification forwards the card events"
        );

        list.set_notifications(&[note("image", "Now text")]);
        assert!(
            children_of::<NotificationCard>(&list)[0] == image_row
                && !child_named::<NotificationImageBody>(&image_row, "notification__image-body")
                    .get_visible(),
            "removing an image reuses the same card and collapses its thumbnail slot"
        );

        list.set_notifications(&[]);
        assert!(
            rows_of(&list).is_empty() && !list.get_visible(),
            "an emptied list unparents its rows and takes no space"
        );

        list.set_notifications(&[note("z", "Only")]);
        let only = rows_of(&list)[0].clone();
        only.set_summary(Some("Touched by hand"));
        list.set_notifications(&[note("z", "Only")]);
        assert_eq!(
            only.summary().as_deref(),
            Some("Touched by hand"),
            "an unchanged slice is not re-applied: the stored copy exists to answer that, and \
             writing it back over an untouched row is the work this avoids"
        );

        let stack = NotificationStack::new();
        let stack_rows = |stack: &NotificationStack| children_of::<NotificationCard>(stack);
        let strips = |stack: &NotificationStack| stack.imp().strips.borrow().len();
        let chip = stack
            .imp()
            .chip
            .get()
            .expect("the chip is built once")
            .clone();
        let chip_text = || {
            stack
                .imp()
                .chip_label
                .get()
                .expect("the chip is built once")
                .text()
                .to_string()
        };

        assert!(
            stack_rows(&stack).is_empty() && !stack.get_visible() && stack.is_collapsed(),
            "a stack lands on screen collapsed and takes no room until it is filled"
        );

        let three = [note("a", "First"), note("b", "Second"), note("c", "Third")];
        stack.set_items(&three);
        assert!(stack.get_visible());
        assert_eq!(stack_rows(&stack).len(), 3);
        stack.set_collapsed(true);
        assert!(!stack.is_collapsed());
        assert_eq!(strips(&stack), 0);
        assert_eq!(
            stack_rows(&stack)
                .iter()
                .map(|row| row.get_visible())
                .collect::<Vec<_>>(),
            vec![true, true, true],
            "three notifications remain individual cards"
        );
        assert!(
            stack_rows(&stack).iter().all(|row| {
                child_named::<gtk4::Button>(row, "notification__close").get_visible()
            }),
            "every card keeps a close button whether it is unread or history"
        );
        assert!(!chip.get_visible());

        let mut four = [
            note("a", "First"),
            note("b", "Second"),
            note("c", "Third"),
            note("d", "Fourth"),
        ];
        four[0].actions = vec![Action {
            key: "reply".to_owned(),
            label: "Reply".to_owned(),
        }];
        stack.set_items(&four);
        assert!(stack.is_collapsed());
        assert_eq!(strips(&stack), 2);
        assert_eq!(
            stack_rows(&stack)
                .iter()
                .map(|row| row.get_visible())
                .collect::<Vec<_>>(),
            vec![true, false, false, false],
            "four notifications collapse to a front card and decorative edges"
        );

        assert!(
            stack
                .first_child()
                .is_some_and(|child| child.has_css_class("notification-stack__strip")),
            "paint order is child order, so the strips must be parented ahead of the front card — \
             one parented after it covers the card it is meant to sit behind"
        );

        assert!(chip.get_visible());
        assert_eq!(chip_text(), "4 notifications");
        let front = &stack_rows(&stack)[0];
        assert!(
            !child_named::<gtk4::Button>(front, "notification__close").get_visible()
                && !child_named::<gtk4::Box>(front, "notification__actions").get_visible(),
            "the collapsed card is a preview, so it exposes no controls for one notification"
        );

        chip.emit_clicked();
        assert!(!stack.is_collapsed(), "the chip opens the stack");
        assert_eq!(
            strips(&stack),
            0,
            "a fanned stack has no edges left to show"
        );
        assert!(stack_rows(&stack).iter().all(|row| row.get_visible()));
        let front = &stack_rows(&stack)[0];
        assert!(
            child_named::<gtk4::Button>(front, "notification__close").get_visible()
                && child_named::<gtk4::Box>(front, "notification__actions").get_visible(),
            "expanding restores the front notification's controls"
        );
        assert_eq!(chip_text(), "Collapse");

        chip.emit_clicked();
        assert!(
            stack.is_collapsed() && strips(&stack) == 2,
            "the same chip closes it: a control that only opens leaves the reader no way back"
        );

        let width = 600;
        let _header_control = stack.header_control();
        let front_height = stack_rows(&stack)[0]
            .measure(gtk4::Orientation::Vertical, width)
            .1;
        let stack_height = stack.measure(gtk4::Orientation::Vertical, width).1;
        assert_eq!(
            stack_height - front_height,
            6,
            "two backplates add only a shallow peek below the front card"
        );

        stack.set_items(&[
            note("a", "First"),
            note("b", "Second"),
            note("c", "Third"),
            note("d", "Fourth"),
            note("e", "Fifth"),
        ]);
        assert_eq!(
            strips(&stack),
            crate::notification_stack::MAX_DEPTH,
            "the stack shows a fixed number of edges however many are behind the front card"
        );

        stack.set_items(&three);
        assert_eq!(stack.imp().depth(), 0);
        stack.set_items(&four);
        assert_eq!(
            stack.imp().depth(),
            crate::notification_stack::MAX_DEPTH,
            "the fourth notification is the point where decorative stack depth appears"
        );

        let front = stack_rows(&stack)[0].clone();
        stack.set_items(&[
            note("b", "Second"),
            note("a", "First again"),
            note("c", "Third"),
            note("d", "Fourth"),
        ]);
        let moved = stack_rows(&stack);
        assert!(
            moved[1] == front,
            "a notification that moves keeps its own widget, so the front card can become one of \
             the cards behind without being rebuilt"
        );

        let stack_fired = Rc::new(RefCell::new(Vec::<String>::new()));
        stack.connect_activated({
            let fired = Rc::clone(&stack_fired);
            move |_, key| fired.borrow_mut().push(format!("activated {key}"))
        });
        stack.connect_dismissed({
            let fired = Rc::clone(&stack_fired);
            move |_, key| fired.borrow_mut().push(format!("dismissed {key}"))
        });
        stack.connect_action_invoked({
            let fired = Rc::clone(&stack_fired);
            move |_, key, action| fired.borrow_mut().push(format!("{key}/{action}"))
        });
        let stack_cleared = Rc::new(Cell::new(0u32));
        stack.connect_clear_requested({
            let cleared = Rc::clone(&stack_cleared);
            move |_| cleared.set(cleared.get() + 1)
        });

        stack.set_collapsed(true);
        secondary_click(&moved[0]);
        assert_eq!(stack_cleared.get(), 1);
        assert!(stack_fired.borrow().is_empty());
        let collapsed_preview = moved[0].clone();
        assert!(
            collapsed_preview.is_focusable(),
            "a collapsed preview owns the card click even without a default action"
        );
        collapsed_preview.emit_by_name::<()>("activated", &[]);
        assert!(
            !stack.is_collapsed() && stack_fired.borrow().is_empty(),
            "the collapsed front card opens the preview without activating its notification"
        );
        assert!(
            !collapsed_preview.is_focusable(),
            "expanding restores the card's actual non-activatable state"
        );
        secondary_click(&moved[0]);
        moved[1].emit_by_name::<()>("activated", &[]);
        child_named::<gtk4::Button>(&moved[2], "notification__close").emit_clicked();
        assert_eq!(
            *stack_fired.borrow(),
            vec![
                "dismissed b".to_owned(),
                "activated a".to_owned(),
                "dismissed c".to_owned(),
            ],
            "right-click and close dismiss expanded cards while activation keeps its own key"
        );

        stack.set_items(&four);
        stack.set_collapsed(true);
        let touched = stack_rows(&stack)[0].clone();
        touched.set_summary(Some("Touched by hand"));
        stack.set_items(&four);
        stack.set_collapsed(true);
        assert_eq!(
            touched.summary().as_deref(),
            Some("Touched by hand"),
            "neither an unchanged slice nor an unchanged collapse state rebuilds the cards"
        );

        stack.set_items(&[note("z", "Only")]);
        assert!(
            !chip.get_visible() && strips(&stack) == 0,
            "one notification has nothing to collapse, so it gets no chip and no edges"
        );

        stack.set_items(&[]);
        assert!(
            stack_rows(&stack).is_empty() && !stack.get_visible(),
            "an emptied stack unparents its cards and takes no space"
        );

        let animated = NotificationStack::new();
        animated.set_items(&four);
        assert!(
            animated.imp().animation.get().is_none(),
            "a standalone stack pays for no animation it did not opt into"
        );
        animated.set_animated(true);
        animated.set_collapsed(false);
        assert!(
            animated.imp().animation.get().is_some()
                && !animated.is_collapsed()
                && stack_rows(&animated).iter().all(|row| row.get_visible()),
            "the explicit opt-in drives the real stack to its expanded state"
        );

        let popover = NotificationsPopover::new();
        let popover_imp = popover.imp();
        let group = |key: &str, app: &str, count: usize| Group {
            key: key.to_owned(),
            app_name: app.to_owned(),
            notifications: (0..count)
                .map(|at| Notification {
                    key: format!("{key}-{at}"),
                    summary: format!("{app} {at}"),
                    ..Notification::default()
                })
                .collect(),
        };

        assert!(
            popover_imp.empty.get_visible()
                && !popover_imp.groups.get_visible()
                && !popover_imp.clear.get_visible(),
            "a popover with nothing in it offers no way to clear it"
        );
        let empty_width = popover.measure(gtk4::Orientation::Horizontal, -1).0;

        assert!(
            popover_imp.scroller.propagates_natural_height()
                && popover_imp.scroller.hscrollbar_policy() == gtk4::PolicyType::Never,
            "a long notification feed scrolls vertically without changing the popover width"
        );

        popover.set_groups(&[group("a", "Telegram", 2), group("b", "PagerDuty", 1)]);
        let sections = children_of::<Section>(&popover_imp.groups.get());
        let popover_stack = child_named::<NotificationStack>(&sections[0], "notification-stack");
        assert!(
            popover_stack.imp().animated.get(),
            "popover groups enable the stack transition at their single construction site"
        );
        let popover_card = children_of::<NotificationCard>(&popover_stack)[0].clone();
        let popup_entry = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        popup_entry.add_css_class("notification-popup__entry");
        let popup_card = NotificationCard::new();
        popup_card.set_summary(Some("Telegram 0"));
        popup_entry.append(&popup_card);
        assert_eq!(
            empty_width,
            popover.measure(gtk4::Orientation::Horizontal, -1).0,
            "an empty notifications popover keeps the same width as a populated one"
        );
        assert_eq!(
            popover_card.measure(gtk4::Orientation::Horizontal, -1).0,
            popup_card.measure(gtk4::Orientation::Horizontal, -1).0,
            "the notification card owns one width in both the popover and popup hierarchies"
        );
        assert_eq!(sections.len(), 2);
        assert!(
            sections
                .iter()
                .all(|section| section.has_css_class("notifications-popover__group")),
            "notification sections carry the group spacing contract"
        );
        assert!(
            sections.iter().all(|section| section.title().is_none()),
            "the app name lives in each card header rather than a duplicated group title"
        );
        assert_eq!(
            sections[0].count(),
            None,
            "the stack control is the group's only count"
        );
        assert_eq!(
            sections[1].count(),
            None,
            "a section does not duplicate the stack's count slot"
        );
        let group_actions = sections[0]
            .imp()
            .trail
            .first_child()
            .and_downcast::<gtk4::Box>()
            .expect("notification group actions");
        let trail_buttons = children_of::<gtk4::Button>(&group_actions);
        assert_eq!(trail_buttons.len(), 2);
        assert!(trail_buttons[0].has_css_class("notification-stack__chip"));
        assert!(trail_buttons[1].has_css_class("section__clear"));
        assert!(popover_imp.clear.get_visible() && !popover_imp.empty.get_visible());

        popover.set_groups(&[group("a", "", 2)]);
        let anonymous = children_of::<Section>(&popover_imp.groups.get())[0].clone();
        assert!(!anonymous.imp().title.get_visible());
        assert!(anonymous.imp().trail.property::<bool>("hexpand"));
        assert_eq!(anonymous.imp().trail.halign(), gtk4::Align::End);

        popover.set_groups(&[group("b", "PagerDuty", 1)]);
        assert_eq!(
            children_of::<Section>(&popover_imp.groups.get()).len(),
            1,
            "a group that goes away takes its section with it"
        );

        let cleared = Rc::new(RefCell::new(Vec::<String>::new()));
        popover.connect_clear_group({
            let cleared = Rc::clone(&cleared);
            move |_, key| cleared.borrow_mut().push(key)
        });
        let dismissed = Rc::new(RefCell::new(Vec::<String>::new()));
        popover.connect_dismissed({
            let dismissed = Rc::clone(&dismissed);
            move |_, key| dismissed.borrow_mut().push(key)
        });

        popover.set_groups(&[group("a", "Telegram", 5)]);
        let dense = children_of::<Section>(&popover_imp.groups.get())[0].clone();
        let dense_stack = child_named::<NotificationStack>(&dense, "notification-stack");
        let shown = |stack: &NotificationStack| {
            children_of::<NotificationCard>(stack)
                .iter()
                .filter(|row| row.get_visible())
                .count()
        };
        let dense_chip = dense_stack
            .imp()
            .chip
            .get()
            .expect("a notification stack builds its chip once")
            .clone();

        assert!(
            dense_stack.is_collapsed()
                && shown(&dense_stack) == 1
                && dense_stack.imp().strips.borrow().len() == crate::notification_stack::MAX_DEPTH
                && dense_chip.get_visible(),
            "a notification group is rendered by the collapsed stack widget"
        );
        assert!(
            child_named::<gtk4::Button>(&dense, "section__clear").get_visible(),
            "a stacked notification group offers a group clear button"
        );

        popover.set_groups(&[group("single", "Telegram", 1)]);
        let single = children_of::<Section>(&popover_imp.groups.get())[0].clone();
        assert!(
            !child_named::<gtk4::Button>(&single, "section__clear").get_visible(),
            "an unstacked notification group does not offer a group clear button"
        );

        popover.set_groups(&[group("a", "Telegram", 5)]);
        let dense = children_of::<Section>(&popover_imp.groups.get())[0].clone();
        let dense_stack = child_named::<NotificationStack>(&dense, "notification-stack");
        let dense_chip = dense_stack
            .imp()
            .chip
            .get()
            .expect("a notification stack builds its chip once")
            .clone();

        secondary_click(&children_of::<NotificationCard>(&dense_stack)[0]);
        assert_eq!(
            *cleared.borrow(),
            vec!["a".to_owned()],
            "right-clicking the collapsed stack reports its group"
        );

        dense_chip.emit_clicked();
        assert!(
            !dense_stack.is_collapsed() && shown(&dense_stack) == 5,
            "the stack control opens the group"
        );
        let dense_rows = children_of::<NotificationCard>(&dense_stack);
        secondary_click(&dense_rows[1]);
        child_named::<gtk4::Button>(&dense_rows[2], "notification__close").emit_clicked();
        assert_eq!(
            *dismissed.borrow(),
            vec!["a-1".to_owned(), "a-2".to_owned()],
            "an expanded stack forwards both right-click and close with the selected card key"
        );

        dense_chip.emit_clicked();
        assert!(
            dense_stack.is_collapsed() && shown(&dense_stack) == 1,
            "the same stack control closes the group"
        );

        popover.set_groups(&[group("a", "Telegram", 1)]);
        assert!(
            !dense_chip.get_visible() && shown(&dense_stack) == 1,
            "a group reduced to one notification takes the stack control away"
        );

        let group_clear = child_named::<gtk4::Button>(&dense, "section__clear");
        assert_eq!(
            group_clear.icon_name().as_deref(),
            Some("window-close-symbolic")
        );
        group_clear.emit_clicked();
        assert_eq!(
            *cleared.borrow(),
            vec!["a".to_owned(), "a".to_owned()],
            "a section reports the group it stands for rather than where it sits"
        );
        assert!(
            !popover_imp.clear.has_css_class("row--danger"),
            "clear all keeps the normal footer treatment"
        );

        let toggles = Rc::new(RefCell::new(Vec::new()));
        popover.connect_dnd_toggled({
            let toggles = Rc::clone(&toggles);
            move |_, silenced| toggles.borrow_mut().push(silenced)
        });

        popover.set_dnd(true);
        assert!(popover.dnd());
        assert!(!popover_imp.notifications.is_active());
        assert_eq!(
            *toggles.borrow(),
            [],
            "showing the state the caller already knows about must not report it back, or the \
             two ends chase each other"
        );

        popover.set_dnd(false);
        assert!(!popover.dnd());
        assert!(popover_imp.notifications.is_active());
        assert!(toggles.borrow().is_empty());

        popover_imp.notifications.set_active(false);
        assert_eq!(
            *toggles.borrow(),
            [true],
            "turning notifications off reports do-not-disturb on"
        );
        popover_imp.notifications.set_active(true);
        assert_eq!(
            *toggles.borrow(),
            [true, false],
            "turning notifications on reports do-not-disturb off"
        );

        assert!(!popover_imp.trouble.get_visible());
        popover.set_trouble(Some("org.freedesktop.Notifications is taken."));
        assert!(popover_imp.trouble.get_visible());
        popover.set_trouble(None);
        assert!(!popover_imp.trouble.get_visible());

        popover.set_groups(&[]);
        assert!(
            popover_imp.empty.get_visible() && !popover_imp.clear.get_visible(),
            "clearing the last notification puts the placeholder back and takes the row away"
        );

        let readout = Readout::new();
        let readout_value = child_named::<gtk4::Label>(&readout, "readout__value");
        let readout_unit = child_named::<gtk4::Label>(&readout, "readout__unit");
        assert!(!readout_value.get_visible() && !readout_unit.get_visible());
        readout.set_value(Some("18"));
        readout.set_unit(Some("°"));
        assert!(readout_value.get_visible() && readout_unit.get_visible());
        assert_eq!(
            *readout.imp().value.text(),
            *"18",
            "the value and the unit are separate labels so each can carry its own size"
        );
        readout.set_unit(None::<&str>);
        assert!(
            !readout_unit.get_visible(),
            "a value with no unit reserves no width for one"
        );

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

        let facts = FactList::new();
        facts.set_facts(&[Fact::new("Humidity", "78%"), Fact::new("Wind", "14 km/h")]);
        let fact_rows: Vec<Row> = children_of(&facts);
        assert_eq!(fact_rows.len(), 2);
        assert_eq!(fact_rows[0].title().as_deref(), Some("Humidity"));
        assert_eq!(fact_rows[1].value().as_deref(), Some("14 km/h"));
        assert!(
            !fact_rows[0].activatable(),
            "a fact states something; it does not lead anywhere, so it does not light up"
        );
        facts.set_facts(&[Fact::new("Humidity", "80%")]);
        assert_eq!(children_of::<Row>(&facts).len(), 1);
        assert_eq!(
            fact_rows[0],
            children_of::<Row>(&facts)[0],
            "a position reuses its row"
        );

        let subclassed = ForecastDay::new();
        let inherited: &Row = subclassed.upcast_ref();
        inherited.set_lead_icon(Some("weather-clear-symbolic"));
        assert_eq!(
            child_named::<gtk4::Image>(&subclassed, "row__icon")
                .icon_name()
                .as_deref(),
            Some("weather-clear-symbolic"),
            "a Row subclass fills the lead through `lead-icon`; `icon-name` on a Gtk.Button is \
             the parent's own property and would replace the row's child instead"
        );
        assert!(
            child_named::<crate::RangeBar>(&subclassed, "range-bar")
                .parent()
                .is_some(),
            "and its template children land in the trail the parent declared"
        );

        let strip = ForecastStrip::new();
        strip.set_hours(&[
            Hour {
                label: "Now".to_owned(),
                icon_name: "weather-showers-symbolic".to_owned(),
                temperature: 18.4,
                now: true,
            },
            Hour {
                label: "16:00".to_owned(),
                icon_name: "weather-clear-symbolic".to_owned(),
                temperature: 16.6,
                now: false,
            },
        ]);
        let times = all_named(&strip, "forecast__time");
        assert_eq!(times.len(), 2);
        assert!(
            times[0].has_css_class("forecast__now") && !times[1].has_css_class("forecast__now"),
            "exactly one column is now"
        );
        let temperatures = all_named(&strip, "forecast__temperature");
        assert_eq!(
            temperatures[0]
                .clone()
                .downcast::<gtk4::Label>()
                .expect("label")
                .text(),
            "18°",
            "the strip owns the rounding and the unit, so two columns cannot disagree"
        );
        let reading = |index: usize| {
            temperatures[index]
                .clone()
                .downcast::<gtk4::Label>()
                .expect("label")
                .text()
                .to_string()
        };
        strip.set_unit("°F");
        assert_eq!(
            (reading(0), reading(1)),
            ("18°F".to_owned(), "17°F".to_owned()),
            "the unit is set from the system the payload declares, and reaches every column"
        );

        let forecast = ForecastList::new();
        let day = |label: &str, precipitation, low: f64, high: f64| Day {
            label: label.to_owned(),
            icon_name: "weather-clear-symbolic".to_owned(),
            precipitation,
            low,
            high,
        };
        forecast.set_days(&[
            day("Today", Some(60), 12.0, 18.0),
            day("Tomorrow", Some(0), 11.0, 20.0),
            day("Sunday", None, 7.0, 26.0),
        ]);
        assert_eq!(
            forecast.scale(),
            (7.0, 26.0),
            "the list owns the scale, so every bar is measured against the same span"
        );
        let bars: Vec<gtk4::Widget> = all_named(&forecast, "range-bar");
        assert_eq!(bars.len(), 3);
        for bar in &bars {
            assert_eq!(
                bar.clone().downcast::<RangeBar>().expect("bar").scale(),
                (7.0, 26.0)
            );
        }
        let chances = all_named(&forecast, "forecast__precipitation");
        let chance = |index: usize| {
            chances[index]
                .clone()
                .downcast::<gtk4::Label>()
                .expect("label")
                .get_visible()
        };
        assert!(chance(0));
        assert!(
            !chance(1) && !chance(2),
            "a zero chance of rain says nothing, and neither does an unknown one"
        );

        let chosen = Rc::new(RefCell::new(Vec::new()));
        forecast.connect_activated({
            let chosen = Rc::clone(&chosen);
            move |_, index| chosen.borrow_mut().push(index)
        });
        children_of::<Row>(&forecast)[1].emit_clicked();
        assert_eq!(*chosen.borrow(), [1u32]);

        let ends = |class: &str| {
            all_named(&forecast, class)
                .first()
                .cloned()
                .and_downcast::<gtk4::Label>()
                .expect("label")
                .text()
                .to_string()
        };
        assert_eq!(
            (ends("forecast__low"), ends("forecast__high")),
            ("12°".to_owned(), "18°".to_owned())
        );
        forecast.set_unit("°C");
        assert_eq!(
            (ends("forecast__low"), ends("forecast__high")),
            ("12°C".to_owned(), "18°C".to_owned()),
            "the list takes the same unit the strip does, on both ends of the range"
        );

        let placeholder = Placeholder::new();
        let empty_icon = child_named::<gtk4::Image>(&placeholder, "placeholder__icon");
        let empty_title = child_named::<gtk4::Label>(&placeholder, "placeholder__title");
        let empty_body = child_named::<gtk4::Label>(&placeholder, "placeholder__description");

        assert!(
            !empty_icon.is_visible() && !empty_title.is_visible() && !empty_body.is_visible(),
            "a placeholder with nothing to say occupies no space"
        );
        placeholder.set_icon_name(Some("network-wireless-offline-symbolic"));
        placeholder.set_title(Some("Wi-Fi is off"));
        placeholder.set_description(Some("Turn it on to see networks."));
        assert!(empty_icon.is_visible() && empty_title.is_visible() && empty_body.is_visible());
        placeholder.set_icon_name(None::<&str>);
        assert!(
            !empty_icon.is_visible(),
            "a cleared icon gives its space back rather than leaving a gap above the title"
        );
        placeholder.set_icon_name(Some("network-wireless-offline-symbolic"));

        assert!(!placeholder.error());
        placeholder.set_error(true);
        assert!(
            placeholder.has_css_class("placeholder--error"),
            "an unreachable service is the same block in a different colour, not a different \
             widget: the shape a user learned for `empty` is the one they read for `broken`"
        );
        placeholder.set_error(false);
        assert!(!placeholder.has_css_class("placeholder--error"));

        placeholder.set_description(Some("W ".repeat(30)));
        let bounded = placeholder.measure(gtk4::Orientation::Horizontal, -1).1;
        placeholder.set_description(Some("W ".repeat(60)));
        assert_eq!(
            placeholder.measure(gtk4::Orientation::Horizontal, -1).1,
            bounded,
            "a placeholder wraps rather than widening the popover around it"
        );

        assert!(row.can_target() && row.activatable());
        row.set_activatable(false);
        assert!(
            !row.can_target() && !row.can_focus(),
            "a row that does nothing takes neither the pointer nor the focus, so it cannot \
             light up under a hover that leads nowhere"
        );

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

        let event = |summary: &str, when: &str, color: Option<gdk::RGBA>| Event {
            summary: summary.to_owned(),
            detail: String::new(),
            when: when.to_owned(),
            color,
        };
        let blue = gdk::RGBA::new(0.2, 0.5, 0.9, 1.0);

        let events = EventList::new();
        events.set_events(&[
            event("Team standup", "09:30", Some(blue)),
            event("Design review", "14:00", None),
        ]);
        let event_rows: Vec<Row> = children_of(&events);
        assert_eq!(event_rows.len(), 2);
        assert_eq!(event_rows[0].title().as_deref(), Some("Team standup"));
        assert!(
            event_rows[1].imp().lead.get_visible(),
            "one event with a color gives every row the same lead column, so the summaries \
             still line up"
        );

        assert_eq!(
            event_rows[0]
                .imp()
                .lead
                .measure(gtk4::Orientation::Horizontal, -1)
                .1,
            (dots::SIZE * 3.0) as i32,
            "an event carries one color, so its lead is one dot wide rather than the three the \
             calendar reserves"
        );

        assert!(
            !event_rows[0].can_target(),
            "an event list nobody is listening to does not light up under the pointer: a hover \
             is a promise that clicking does something"
        );
        events.set_activatable(true);
        assert!(event_rows[0].can_target());

        let activated = Rc::new(RefCell::new(Vec::new()));
        events.connect_activated({
            let activated = Rc::clone(&activated);
            move |_, index| activated.borrow_mut().push(index)
        });
        event_rows[1].emit_clicked();
        assert_eq!(*activated.borrow(), [1u32]);

        events.set_events(&[event("Team standup", "09:30", None)]);
        assert!(
            !children_of::<Row>(&events)[0].imp().lead.get_visible(),
            "with no color anywhere the lead column goes away rather than sitting empty"
        );
        assert_eq!(
            event_rows[0],
            children_of::<Row>(&events)[0],
            "a position reuses its row rather than rebuilding it"
        );
        assert!(
            event_rows[1].parent().is_none(),
            "a shorter list unparents the rows it no longer has events for"
        );

        events.set_events(&[
            event("One", "09:30", None),
            event("Two", "10:00", None),
            event("Three", "11:00", None),
            event("Four", "12:00", None),
        ]);
        events.set_activatable(false);
        events.set_max_rows(3);
        let capped: Vec<Row> = children_of(&events);
        assert_eq!(
            capped.len(),
            4,
            "three events plus the row that counts the rest"
        );
        assert!(capped[3].has_css_class("row--quiet"));
        assert!(
            capped[3].activatable() && !capped[0].activatable(),
            "the overflow row is a control, not an event: it exists only because the caller \
             capped the list, and clicking it is the whole reason it is there"
        );
        assert_eq!(
            capped[3].title().as_deref(),
            Some("1 more event"),
            "one hidden event is not `1 more events`"
        );
        assert_eq!(capped[2].title().as_deref(), Some("Three"));

        events.set_events(&[event("One", "09:30", None)]);
        assert_eq!(
            children_of::<Row>(&events).len(),
            1,
            "a list that now fits drops the overflow row"
        );
        events.set_events(&[
            event("One", "09:30", None),
            event("Two", "10:00", None),
            event("Three", "11:00", None),
            event("Four", "12:00", None),
            event("Five", "13:00", None),
        ]);
        let regrown: Vec<Row> = children_of(&events);
        assert_eq!(regrown.len(), 4);
        assert!(
            regrown[3].has_css_class("row--quiet")
                && regrown[3].title().as_deref() == Some("2 more events"),
            "the overflow row stays last when the list grows back under it"
        );

        let overflowed = Rc::new(Cell::new(0u32));
        events.connect_overflow({
            let overflowed = Rc::clone(&overflowed);
            move |_| overflowed.set(overflowed.get() + 1)
        });
        regrown[3].emit_clicked();
        assert_eq!(overflowed.get(), 1);

        events.set_max_rows(0);
        assert_eq!(
            children_of::<Row>(&events).len(),
            5,
            "no cap shows everything, with nothing left to count"
        );

        events.set_events(&[event(&"ё".repeat(TEXT_MAX_CHARS * 2), "09:30", None)]);
        assert_eq!(
            children_of::<Row>(&events)[0]
                .title()
                .unwrap_or_default()
                .chars()
                .count(),
            TEXT_MAX_CHARS,
            "a calendar summary is another application's string: capped without slicing a \
             multi-byte character"
        );

        let buried = Section::new();
        buried.set_content(Some(&events));
        buried.set_empty(true);
        assert_eq!(
            children_of::<Row>(&events)[0]
                .title()
                .unwrap_or_default()
                .chars()
                .count(),
            TEXT_MAX_CHARS,
            "a row inside a hidden section still reports what it was given: a widget's own \
             `visible` flag is not the same question as whether an ancestor is showing"
        );

        let clock = WorldClock::new();
        let zone = |label: &str, timezone: &str| Zone {
            label: label.to_owned(),
            timezone: timezone.to_owned(),
            note: String::new(),
            icon_name: String::new(),
        };
        clock.set_zones(&[
            zone("Berlin", "Europe/Berlin"),
            zone("Auckland", "Pacific/Auckland"),
            zone("Midway", "Pacific/Midway"),
            zone("Nowhere", "Not/AZone"),
        ]);
        clock.set_now(&glib::DateTime::from_utc(2026, 9, 1, 12, 0, 0.0).expect("instant"));

        let clock_rows: Vec<Row> = children_of(&clock);
        let time_of = |row: &Row| {
            child_named::<gtk4::Label>(row, "world-clock__time")
                .text()
                .to_string()
        };
        assert_eq!(clock_rows.len(), 4);
        assert_eq!(time_of(&clock_rows[0]), "14:00");
        assert_eq!(time_of(&clock_rows[1]), "00:00");
        assert_eq!(
            time_of(&clock_rows[3]),
            "—",
            "a timezone the system cannot resolve reads as unknown rather than silently as UTC"
        );

        assert_eq!(
            clock_rows[0].subtitle(),
            None,
            "a zone on the same date says nothing, so a list of neighbours stays one line each"
        );
        assert_eq!(clock_rows[1].subtitle().as_deref(), Some("Tomorrow"));
        assert_eq!(clock_rows[2].subtitle(), None);
        assert_eq!(clock_rows[3].subtitle(), None);

        let phase_of = |row: &Row| {
            child_named::<gtk4::Image>(row, "world-clock__phase")
                .icon_name()
                .map(|name| name.to_string())
        };
        assert_eq!(
            phase_of(&clock_rows[0]),
            Some("weather-clear-symbolic".to_owned()),
            "14:00 in Berlin is daylight, which is the one thing a world clock is consulted for"
        );
        assert_eq!(
            phase_of(&clock_rows[1]),
            Some("weather-clear-night-symbolic".to_owned()),
            "00:00 in Auckland is not"
        );
        assert_eq!(
            phase_of(&clock_rows[3]),
            None,
            "a zone that did not resolve claims nothing about daylight"
        );

        clock.set_now(&glib::DateTime::from_utc(2026, 9, 1, 5, 0, 0.0).expect("instant"));
        assert_eq!(clock_rows[2].subtitle().as_deref(), Some("Yesterday"));
        assert_eq!(
            phase_of(&clock_rows[1]),
            Some("weather-clear-symbolic".to_owned()),
            "17:00 in Auckland is daylight, so the icon follows the clock rather than the zone"
        );
        assert_eq!(clock_rows[1].subtitle(), None);

        clock.set_twelve_hour(true);
        assert!(
            time_of(&clock_rows[0]).starts_with("7:00"),
            "twelve-hour drops the padding strftime leaves in front of a single digit"
        );
        clock.set_twelve_hour(false);
        assert_eq!(time_of(&clock_rows[0]), "07:00");

        assert_eq!(
            clock_rows[0].tooltip_text().as_deref(),
            Some("Europe/Berlin · CEST (UTC+02:00)"),
            "the label is the city a user named; the tooltip is the zone it actually resolved \
             to, with the offset that makes the time checkable"
        );
        assert_eq!(
            clock_rows[3].tooltip_text().as_deref(),
            Some("Not/AZone"),
            "a zone that does not resolve still names itself, because that is the diagnostic"
        );
        assert!(
            clock_rows[0].can_target(),
            "a clock row still takes the pointer, because that is what raises the tooltip; it \
             just does not light up, since the tooltip is all the click would have given"
        );
        assert!(
            !clock_rows[0].can_focus(),
            "and it is not a tab stop, because there is nothing to activate once you reach it"
        );

        clock.set_now(&glib::DateTime::from_utc(2026, 9, 1, 12, 0, 0.0).expect("instant"));
        clock.set_zones(&[
            Zone {
                note: "12° · Light rain".to_owned(),
                icon_name: "weather-showers-symbolic".to_owned(),
                ..zone("Berlin", "Europe/Berlin")
            },
            Zone {
                note: "9° · Clear".to_owned(),
                ..zone("Auckland", "Pacific/Auckland")
            },
        ]);
        assert_eq!(
            clock_rows[0].subtitle().as_deref(),
            Some("12° · Light rain"),
            "a zone with something to add carries it on the second line"
        );
        assert_eq!(
            phase_of(&clock_rows[0]),
            Some("weather-showers-symbolic".to_owned()),
            "and a zone that knows its weather draws that instead of the sun, so the icon cannot \
             contradict the line under it"
        );
        assert_eq!(
            phase_of(&clock_rows[1]),
            Some("weather-clear-night-symbolic".to_owned()),
            "a zone with no icon of its own still falls back to daylight"
        );
        assert_eq!(
            clock_rows[1].subtitle().as_deref(),
            Some("Tomorrow · 9° · Clear"),
            "and shares that line with the day note rather than taking a third, because a third \
             is what makes a clock list stop being glanceable"
        );

        let scrubber = Scrubber::new();
        let track = child_named::<gtk4::Scale>(&scrubber, "scrubber__track");
        let times = all_named(&scrubber, "scrubber__time");
        let elapsed = times[0].clone().downcast::<gtk4::Label>().expect("elapsed");
        let remaining = times[1]
            .clone()
            .downcast::<gtk4::Label>()
            .expect("remaining");

        assert!(
            !scrubber.seekable() && !track.is_sensitive(),
            "a scrubber starts unseekable, matching a template whose scale is already insensitive; \
             a widget that disagrees with its own blueprint at birth is wrong before anyone \
             touches it"
        );
        scrubber.set_seekable(true);
        assert!(track.is_sensitive());

        scrubber.set_duration(405.0);
        scrubber.set_position(167.0);
        assert_eq!(elapsed.text(), "2:47");
        assert_eq!(
            remaining.text(),
            "\u{2212}3:58",
            "the right-hand figure counts down, with a real minus sign rather than a hyphen"
        );
        assert_eq!(scrubber.position(), 167.0);

        assert_eq!(
            (
                track.adjustment().step_increment(),
                track.adjustment().page_increment()
            ),
            (5.0, 30.0),
            "an arrow key moves five seconds and Page Up thirty. These are set from Rust because \
             blueprint-compiler's adjustment rule rejects an adjustment carrying anything besides \
             lower, upper and value, so nothing in the template guards them"
        );

        scrubber.imp().held.set(Some(167.0));
        scrubber.set_position(300.0);
        assert_eq!(
            scrubber.position(),
            167.0,
            "a player reporting its position once a second loses to a drag in progress, or the \
             slider is pulled out from under the pointer every time one lands"
        );
        scrubber.imp().held.set(None);
        scrubber.set_position(300.0);
        assert_eq!(scrubber.position(), 300.0);
        scrubber.set_position(167.0);

        let seeks = Rc::new(RefCell::new(Vec::new()));
        scrubber.connect_seek({
            let seeks = Rc::clone(&seeks);
            move |_, seconds| seeks.borrow_mut().push(seconds)
        });
        scrubber.emit_by_name::<()>("seek", &[&12.0f64]);
        assert_eq!(*seeks.borrow(), [12.0f64]);

        scrubber.set_duration(0.0);
        assert!(
            !track.get_visible(),
            "a stream with no length has nothing to scrub, so the track goes rather than sitting \
             there full or empty and lying about it"
        );
        assert!(
            !remaining.get_visible(),
            "and nothing remains of a length nobody knows"
        );

        let transport = Transport::new();
        let buttons = children_of::<gtk4::Button>(&transport);
        let [shuffle, previous, play, next, repeat] =
            <[gtk4::Button; 5]>::try_from(buttons).expect("five transport buttons");
        assert!(
            play.has_css_class("transport__play"),
            "play is the middle of five, and the order shuffle-previous-play-next-repeat is the \
             layout rather than an accident of how they were declared"
        );

        assert_eq!(
            play.icon_name().as_deref(),
            Some("media-playback-start-symbolic")
        );
        transport.set_playing(true);
        assert_eq!(
            play.icon_name().as_deref(),
            Some("media-playback-pause-symbolic")
        );

        transport.set_can_next(false);
        assert!(
            next.get_visible() && !next.is_sensitive(),
            "a capability a player lacks dims its button; removing it would move the other four \
             under the pointer between one track and the next"
        );

        assert!(
            !shuffle.get_visible() && !repeat.get_visible(),
            "shuffle and repeat are the two that hide instead, because a player without them has \
             no state for them to show and a permanently dead icon is worse than none"
        );
        transport.set_can_shuffle(true);
        transport.set_can_repeat(true);
        assert!(shuffle.get_visible() && repeat.get_visible());

        transport.set_repeat(Repeat::Track);
        assert_eq!(
            repeat.icon_name().as_deref(),
            Some("media-playlist-repeat-song-symbolic"),
            "repeat-one is a different icon, not a different shade of the same one"
        );
        assert!(repeat.has_css_class("transport--on"));
        transport.set_repeat(Repeat::Playlist);
        assert_eq!(
            repeat.icon_name().as_deref(),
            Some("media-playlist-repeat-symbolic")
        );
        assert!(repeat.has_css_class("transport--on"));
        transport.set_repeat(Repeat::Off);
        assert!(!repeat.has_css_class("transport--on"));

        let actions = Rc::new(RefCell::new(Vec::new()));
        transport.connect_action({
            let actions = Rc::clone(&actions);
            move |_, action| actions.borrow_mut().push(action)
        });
        previous.emit_clicked();
        play.emit_clicked();
        next.emit_clicked();
        shuffle.emit_clicked();
        repeat.emit_clicked();
        assert_eq!(
            *actions.borrow(),
            [
                TransportAction::Previous,
                TransportAction::PlayPause,
                TransportAction::Next,
                TransportAction::Shuffle,
                TransportAction::Repeat,
            ],
            "every button reports which one it was, so one handler covers the whole row"
        );

        let playing = NowPlaying::new();
        let art = child_named::<gtk4::Image>(&playing, "now-playing__art");
        assert_eq!(
            (
                art.icon_name().as_deref(),
                art.has_css_class("now-playing__art--empty")
            ),
            (Some("audio-x-generic-symbolic"), true),
            "a player with no cover yet looks the same as one that lost its cover; the template \
             has to be born in the state set_art(None) would put it in, or the first frame is an \
             empty square nothing ever fills"
        );
        let source_icon = child_named::<gtk4::Image>(&playing, "now-playing__source-icon");
        let source_line = source_icon
            .parent()
            .and_downcast::<gtk4::Box>()
            .expect("source line");

        assert!(
            !source_line.get_visible(),
            "the line above the title is gone entirely until there is an application to name, \
             rather than holding open a gap the title then sits below"
        );
        playing.set_source(Some("Spotify"));
        assert!(source_line.get_visible());
        playing.set_icon_name(Some("audio-x-generic-symbolic"));
        assert!(source_icon.get_visible());
        playing.set_source(None::<&str>);
        assert!(
            source_line.get_visible(),
            "an icon alone still earns the line; it is emptiness of both that removes it"
        );
        playing.set_icon_name(None::<&str>);
        assert!(!source_line.get_visible());

        assert_eq!(
            playing.scrubber(),
            child_named::<Scrubber>(&playing, "scrubber")
        );
        assert_eq!(
            playing.transport(),
            child_named::<Transport>(&playing, "transport")
        );

        let empty = art.measure(gtk4::Orientation::Horizontal, -1).1;
        assert!(
            empty > 32,
            "the square is the stylesheet's, and a widget built before the providers were \
             installed never picks it up — GtkImage then measures its 16px default for every \
             case and the comparison below compares nothing"
        );
        let cover = gdk::MemoryTexture::new(
            192,
            192,
            gdk::MemoryFormat::R8g8b8a8,
            &glib::Bytes::from_owned(vec![0u8; 192 * 192 * 4]),
            192 * 4,
        );
        playing.set_art(Some(&cover));
        assert!(
            !art.has_css_class("now-playing__art--empty"),
            "real art drops the inset that makes the placeholder glyph sit small in its square"
        );
        assert_eq!(
            art.measure(gtk4::Orientation::Horizontal, -1).1,
            empty,
            "and it occupies exactly the square the placeholder held, so a cover arriving late \
             cannot resize the popover around it"
        );

        playing.set_art(None::<&gdk::Paintable>);
        assert_eq!(art.icon_name().as_deref(), Some("audio-x-generic-symbolic"));
        assert!(art.has_css_class("now-playing__art--empty"));
        assert_eq!(
            art.measure(gtk4::Orientation::Horizontal, -1).1,
            empty,
            "and the placeholder comes back to the same square, so losing a cover does not \
             resize the popover either"
        );

        let outputs = ChoiceList::new();
        let choice = |label: &str, detail: &str| Choice {
            label: label.to_owned(),
            detail: detail.to_owned(),
            icon_name: "audio-headphones-symbolic".to_owned(),
        };
        outputs.set_choices(&[
            choice("WH-1000XM5", "Bluetooth"),
            choice("Built-in speakers", ""),
        ]);
        let choice_rows = children_of::<Row>(&outputs);
        assert_eq!(choice_rows.len(), 2);
        assert_eq!(choice_rows[0].subtitle().as_deref(), Some("Bluetooth"));
        assert_eq!(choice_rows[1].subtitle(), None);
        assert!(
            choice_rows.iter().all(|row| row.selectable()),
            "every row reserves the check, so choosing one does not shunt the labels sideways"
        );
        assert!(
            !choice_rows[0].selected() && !choice_rows[1].selected(),
            "a list nobody has chosen from shows no check at all rather than defaulting to the \
             first, which would claim something untrue about the backend"
        );

        outputs.set_selected(Some(1));
        assert!(!choice_rows[0].selected() && choice_rows[1].selected());

        outputs.connect_activated(|list, index| {
            assert_eq!(
                list.selected(),
                Some(index),
                "the check has already moved by the time the handler runs; a list that waits for \
                 the backend to confirm shows the old row as chosen for a whole round trip"
            );
        });
        choice_rows[0].emit_clicked();
        assert_eq!(outputs.selected(), Some(0));
        assert!(choice_rows[0].selected() && !choice_rows[1].selected());

        outputs.set_choices(&[choice("Built-in speakers", "")]);
        assert_eq!(
            outputs.selected(),
            None,
            "any change to the list drops the selection, because it is positional: index 0 \
             named the headphones a moment ago and names the speakers now, so keeping it would \
             quietly check the wrong device"
        );
        assert_eq!(children_of::<Row>(&outputs).len(), 1);

        outputs.set_selected(Some(7));
        assert_eq!(
            outputs.selected(),
            None,
            "and an index past the end is refused rather than stored to confuse the next render"
        );

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

        let players = PlayerList::new();
        players.set_players(&[
            Player {
                key: "firefox".to_owned(),
                name: "Firefox".to_owned(),
                icon_name: "web-browser-symbolic".to_owned(),
                title: "How the Chip Shortage Ends".to_owned(),
                artist: "Odd Lots".to_owned(),
                playing: false,
            },
            Player {
                key: "vlc".to_owned(),
                name: "VLC".to_owned(),
                icon_name: "video-x-generic-symbolic".to_owned(),
                title: "The Wire".to_owned(),
                artist: String::new(),
                playing: true,
            },
        ]);
        let player_rows = children_of::<PlayerRow>(&players);
        assert_eq!(player_rows.len(), 2);

        let first: &Row = player_rows[0].upcast_ref();
        let second: &Row = player_rows[1].upcast_ref();
        assert_eq!(first.subtitle().as_deref(), Some("Odd Lots · Firefox"));
        assert_eq!(
            second.subtitle().as_deref(),
            Some("VLC"),
            "a video with no artist says which application is playing it rather than showing a \
             bare separator"
        );

        let toggle = child_named::<gtk4::Button>(&player_rows[1], "player-row__toggle");
        assert_eq!(
            toggle.icon_name().as_deref(),
            Some("media-playback-pause-symbolic")
        );

        let promoted = Rc::new(RefCell::new(Vec::new()));
        players.connect_activated({
            let promoted = Rc::clone(&promoted);
            move |_, key| promoted.borrow_mut().push(key)
        });
        let toggled = Rc::new(RefCell::new(Vec::new()));
        players.connect_toggled({
            let toggled = Rc::clone(&toggled);
            move |_, key| toggled.borrow_mut().push(key)
        });
        player_rows[1].emit_clicked();
        toggle.emit_clicked();
        assert_eq!(*promoted.borrow(), ["vlc".to_owned()]);
        assert_eq!(
            *toggled.borrow(),
            ["vlc".to_owned()],
            "the button in the trail is a second gesture on the same row, and each carries the \
             same key so one list handles both"
        );

        players.set_players(&[Player {
            key: "vlc".to_owned(),
            name: "VLC".to_owned(),
            ..Default::default()
        }]);
        children_of::<PlayerRow>(&players)[0].emit_clicked();
        assert_eq!(
            *promoted.borrow(),
            ["vlc".to_owned(), "vlc".to_owned()],
            "a row is reused in place, so the key it reports is read back at the moment it fires \
             rather than captured when the row was built"
        );

        let split = SplitRow::new();
        split.set_title(Some("DP-1 · DELL U2720Q"));
        split.set_value(Some("144 Hz"));
        assert_eq!(
            split.row().title().as_deref(),
            Some("DP-1 · DELL U2720Q"),
            "a forwarded property reaches the row rather than living twice"
        );

        let built = gtk4::Builder::from_string(
            r#"<interface><object class="SplitRow" id="split">
                 <child type="trail"><object class="GtkSwitch" id="knob"/></child>
               </object></interface>"#,
        );
        let declared: SplitRow = built.object("split").expect("SplitRow builds from a .ui");
        let knob: gtk4::Switch = built.object("knob").expect("the trail child builds");
        assert_eq!(
            knob.ancestor(Row::static_type()),
            Some(declared.row().upcast::<gtk4::Widget>()),
            "a [trail] child lands inside the row, so the detail button stays last"
        );
        assert_eq!(
            split.detail().parent().as_ref(),
            Some(split.upcast_ref::<gtk4::Widget>()),
            "the detail button is the wrapper's own child, never the row's"
        );

        let fired: Rc<RefCell<Vec<&str>>> = Rc::new(RefCell::new(Vec::new()));
        split.connect_activated({
            let fired = Rc::clone(&fired);
            move |_| fired.borrow_mut().push("activated")
        });
        split.connect_details({
            let fired = Rc::clone(&fired);
            move |_| fired.borrow_mut().push("details")
        });

        split.row().emit_clicked();
        assert_eq!(
            *fired.borrow(),
            ["activated"],
            "the row body is the primary action and says nothing about details"
        );
        split.detail().emit_clicked();
        assert_eq!(
            *fired.borrow(),
            ["activated", "details"],
            "the detail button is the only way in, and does not also act"
        );

        let pager = Pager::new();
        assert!(
            !pager.is_visible(),
            "a pager with no slots hides itself rather than reserving width on the bar"
        );

        let slot = |id: u64, label: &str| Slot {
            id,
            label: label.to_owned(),
            ..Slot::default()
        };
        pager.set_slots(&[slot(4, "1"), slot(7, "2"), slot(9, "3")]);
        assert!(pager.is_visible(), "a pager with slots shows itself");
        assert_eq!(
            children_of::<PagerItem>(&pager).len(),
            3,
            "one item per slot, built once and reused"
        );

        pager.set_slots(&[slot(4, "1")]);
        assert_eq!(
            children_of::<PagerItem>(&pager).len(),
            1,
            "a shorter list unparents the items it no longer has data for"
        );

        pager.set_slots(&[slot(21, "1"), slot(22, "2")]);
        let item = &children_of::<PagerItem>(&pager)[0];
        assert!(
            item.observe_controllers().n_items() == 0,
            "a slot takes no click of its own: the strip is one surface, and a button per slot \
             would swallow the primary press the popover opens on"
        );

        let pressed: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
        pager.connect_pressed({
            let pressed = Rc::clone(&pressed);
            move |_| *pressed.borrow_mut() += 1
        });
        pager.emit_by_name::<()>("pressed", &[]);
        assert_eq!(*pressed.borrow(), 1);

        assert!(
            pager.anchor().is_none(),
            "a strip nobody has pressed anchors the arrow on itself rather than on a stale item"
        );

        pager.allocate(120, 24, -1, None);
        let items = children_of::<PagerItem>(&pager);
        let second = items[1].compute_bounds(&pager).expect("allocated item");
        assert_eq!(
            pager.item_at(
                f64::from(second.x() + second.width() / 2.0),
                f64::from(second.y() + second.height() / 2.0),
            ),
            Some(items[1].clone()),
            "the arrow points at the workspace that was pressed, so a press resolves to its item"
        );
        assert!(
            pager.item_at(-1.0, -1.0).is_none(),
            "a press that landed on no item anchors nothing rather than the first one"
        );

        let untouched = child_named::<gtk4::Label>(&pager, "pager-item__label");
        untouched.set_text("tampered");
        pager.set_slots(&[slot(21, "1"), slot(22, "renamed")]);
        assert_eq!(
            untouched.text(),
            "tampered",
            "one changed slot rewrites one item; the rest never reach GTK"
        );

        let label = child_named::<gtk4::Label>(&pager, "pager-item__label");
        assert!(
            !label.is_visible(),
            "a dot carries no text, so the label stays out of the measurement"
        );
        pager.set_shape(Shape::Labels);
        assert!(
            label.is_visible(),
            "labels is the shape that shows the label"
        );
        assert!(
            pager.has_css_class("pager--labels") && !pager.has_css_class("pager--dots"),
            "the shape is one class, so a stylesheet never sees both at once"
        );

        let pill = &children_of::<PagerItem>(&pager)[0];
        let (wide, ..) = pill.measure(gtk4::Orientation::Horizontal, -1);
        let (tall, ..) = pill.measure(gtk4::Orientation::Vertical, -1);
        assert_eq!(
            wide, tall,
            "a one-character number sits in a circle, so padding and min-width are chosen together"
        );

        pager.set_slots(&[Slot {
            id: 21,
            label: "1".to_owned(),
            urgent: true,
            focus: Focus::Here,
            ..Slot::default()
        }]);
        let item = &children_of::<PagerItem>(&pager)[0];
        assert!(
            item.has_css_class("pager-item--urgent") && item.has_css_class("pager-item--here"),
            "urgency is drawn on top of focus rather than replacing it"
        );

        let stepped: Rc<RefCell<Vec<(bool, bool)>>> = Rc::new(RefCell::new(Vec::new()));
        pager.connect_stepped({
            let stepped = Rc::clone(&stepped);
            move |_, horizontal, forward| stepped.borrow_mut().push((horizontal, forward))
        });
        pager.emit_by_name::<()>("stepped", &[&true, &false]);
        assert_eq!(
            *stepped.borrow(),
            [(true, false)],
            "the typed wrapper agrees with the declared parameters, which nothing checks at compile time"
        );

        assert!(
            !pager.has_css_class("pager--vertical") && pager.valign() == gtk4::Align::Center,
            "a pager starts on a horizontal bar, centered across it"
        );

        pager.set_orientation(gtk4::Orientation::Vertical);
        assert!(
            pager.has_css_class("pager--vertical"),
            "the strip has to say which way it runs, because the active dot lengthens along it \
             and a rule keyed on width alone widens the whole column instead"
        );
        assert_eq!(
            pager.halign(),
            gtk4::Align::Center,
            "a vertical strip that fills the bar's width stretches every dot into a bar of its own"
        );

        pager.set_orientation(gtk4::Orientation::Horizontal);
        assert!(
            !pager.has_css_class("pager--vertical") && pager.valign() == gtk4::Align::Center,
            "and back, since a panel's position is a setting that changes under a running applet"
        );

        let popover = WorkspacesPopover::new();
        let session = |title: &str| {
            vec![
                Workspace {
                    id: 1,
                    label: "chats".to_owned(),
                    detail: "1 window".to_owned(),
                    output: "DP-2".to_owned(),
                    focused: false,
                    urgent: false,
                    windows: vec![WorkspaceWindow {
                        id: 9,
                        title: title.to_owned(),
                        app_id: "ghostty".to_owned(),
                        focused: false,
                        urgent: false,
                    }],
                },
                Workspace {
                    id: 2,
                    label: "browser".to_owned(),
                    detail: "1 window".to_owned(),
                    output: "DP-2".to_owned(),
                    focused: true,
                    urgent: false,
                    windows: vec![WorkspaceWindow {
                        id: 11,
                        title: "a browser".to_owned(),
                        app_id: "chrome".to_owned(),
                        focused: true,
                        urgent: false,
                    }],
                },
                Workspace {
                    id: 5,
                    label: "1".to_owned(),
                    detail: "empty".to_owned(),
                    output: "eDP-1".to_owned(),
                    focused: false,
                    urgent: false,
                    windows: Vec::new(),
                },
            ]
        };
        popover.set_workspaces(&session("a terminal"));

        let sections = children_of::<WorkspaceSection>(&child_named::<WorkspaceList>(
            &popover,
            "workspace-list",
        ));
        assert_eq!(
            sections.len(),
            2,
            "workspaces are grouped by the display they are on, which is half of what the list \
             is for"
        );

        let chosen: Rc<RefCell<Vec<u64>>> = Rc::new(RefCell::new(Vec::new()));
        popover.connect_activated({
            let chosen = Rc::clone(&chosen);
            move |id| chosen.borrow_mut().push(id)
        });
        let column = child_named::<gtk4::Box>(&sections[0], "section__content")
            .first_child()
            .and_downcast::<gtk4::Box>()
            .expect("a section holds one column of rows");
        let rows = children_of::<SplitRow>(&column);
        assert_eq!(rows.len(), 2, "DP-2 carries two of the three workspaces");
        rows[1].emit_by_name::<()>("activated", &[]);
        assert_eq!(
            *chosen.borrow(),
            [2],
            "a row reports the workspace it stands for, which is the id the focus command needs"
        );

        let drawer = child_named::<gtk4::Box>(&popover, "drawer-page")
            .ancestor(gtk4::Revealer::static_type())
            .and_downcast::<gtk4::Revealer>()
            .expect("the page sits inside the drawer");
        assert!(
            !drawer.reveals_child(),
            "a popover opens showing the list, not one workspace's detail"
        );
        assert_eq!(
            drawer
                .parent()
                .and_downcast::<gtk4::Box>()
                .expect("the drawer is laid out beside the list")
                .orientation(),
            gtk4::Orientation::Horizontal,
            "the drawer opens to the side: pushing it underneath walks the list off the bottom of \
             the screen and takes the row that would close it with it"
        );
        assert_eq!(
            drawer.transition_type(),
            gtk4::RevealerTransitionType::SlideRight,
            "and it slides in along the axis it grows on"
        );

        rows[0].emit_by_name::<()>("details", &[]);
        assert!(
            drawer.reveals_child(),
            "the chevron is the only way into a workspace's windows, so it has to open the drawer"
        );
        let windows = children_of::<Row>(&child_named::<gtk4::Box>(&popover, "drawer-page"));
        assert_eq!(
            windows.len(),
            1,
            "the drawer lists the windows of the workspace whose chevron was pressed"
        );
        assert_eq!(windows[0].title().as_deref(), Some("a terminal"));

        let focused: Rc<RefCell<Vec<u64>>> = Rc::new(RefCell::new(Vec::new()));
        popover.connect_window_activated({
            let focused = Rc::clone(&focused);
            move |id| focused.borrow_mut().push(id)
        });
        windows[0].emit_by_name::<()>("clicked", &[]);
        assert_eq!(
            *focused.borrow(),
            [9],
            "a window in the drawer is the only place a single window can be focused from, now \
             that the strip itself opens the popover"
        );

        let first_row = || {
            let listed = children_of::<WorkspaceSection>(&child_named::<WorkspaceList>(
                &popover,
                "workspace-list",
            ));
            let column = child_named::<gtk4::Box>(&listed[0], "section__content")
                .first_child()
                .and_downcast::<gtk4::Box>()
                .expect("a section holds one column of rows");
            children_of::<SplitRow>(&column)[0].clone()
        };

        let standing = first_row();
        popover.set_workspaces(&session("a browser"));
        assert!(
            drawer.reveals_child(),
            "an event arriving must not close a drawer the user opened"
        );
        assert_eq!(
            children_of::<Row>(&child_named::<gtk4::Box>(&popover, "drawer-page"))[0]
                .title()
                .as_deref(),
            Some("a browser"),
            "an open drawer follows the session: a window renaming itself shows through"
        );
        assert_eq!(
            standing,
            first_row(),
            "a window retitling itself must not rebuild the workspace list under the pointer, and \
             a title changes on every keystroke"
        );
        assert_eq!(
            children_of::<Row>(&child_named::<gtk4::Box>(&popover, "drawer-page"))[0],
            windows[0],
            "and it keeps its own row too: the drawer reconciles by window id, so a retitle \
             rewrites a label rather than replacing the row under the pointer"
        );

        rows[1].emit_by_name::<()>("details", &[]);
        let switched = children_of::<Row>(&child_named::<gtk4::Box>(&popover, "drawer-page"));
        assert_ne!(
            switched[0], windows[0],
            "another workspace's window is another row, not the first one refilled"
        );
        switched[0].emit_by_name::<()>("clicked", &[]);
        assert_eq!(
            *focused.borrow(),
            [9, 11],
            "a row is only ever reused for the window it was built for, so it can report that \
             window without consulting anything"
        );

        rows[1].emit_by_name::<()>("details", &[]);
        assert!(
            !drawer.reveals_child(),
            "the same chevron closes what it opened"
        );

        for make in [
            (|| Row::new().upcast::<gtk4::Widget>()) as fn() -> gtk4::Widget,
            || Notice::new().upcast(),
            || EventRow::new().upcast(),
            || PlayerRow::new().upcast(),
            || ClockRow::new().upcast(),
            || ForecastDay::new().upcast(),
            || ForecastHour::new().upcast(),
            || Hero::new().upcast(),
        ] {
            drop(make());
        }

        let popover = CalendarPopover::new();
        let event = |summary: &str| Event {
            summary: summary.to_owned(),
            detail: String::new(),
            when: "09:00 · 1 h".to_owned(),
            color: None,
        };
        let drawer = || child_named::<gtk4::Revealer>(&popover, "calendar-popover__drawer");

        let footer = || child_named::<gtk4::Box>(&popover, "popover-shell__footer");

        popover.set_footer(None);
        assert!(
            !child_named::<Row>(&popover, "calendar-popover__footer").is_visible(),
            "a footer with no label is a row that would do nothing when clicked"
        );
        assert!(
            !footer().is_visible(),
            "an empty footer still costs its own padding and the hairline above it"
        );

        popover.set_footer(Some("Open calendar"));
        assert!(child_named::<Row>(&popover, "calendar-popover__footer").is_visible());
        assert!(footer().is_visible());

        popover.set_zones(&[]);
        assert!(
            !child_named::<Section>(&popover, "calendar-popover__zones").is_visible(),
            "an empty world clock is a section heading over nothing"
        );

        let few: Vec<Event> = (0..3)
            .map(|index| event(&format!("event {index}")))
            .collect();
        popover.set_day("Today", &few);
        assert!(
            !drawer().reveals_child(),
            "three events fit, so nothing was hidden and the drawer has nothing to hold"
        );

        let many: Vec<Event> = (0..9)
            .map(|index| event(&format!("event {index}")))
            .collect();
        popover.set_day("Today", &many);
        assert!(
            !drawer().reveals_child(),
            "the overflow row is an offer, not a drawer that springs open on its own"
        );

        let overflow = child_named::<EventList>(&popover, "calendar-popover__events");
        overflow.emit_by_name::<()>("overflow", &[]);
        assert!(
            drawer().reveals_child(),
            "taking that offer is what the drawer is wired to"
        );

        overflow.emit_by_name::<()>("overflow", &[]);
        assert!(
            !drawer().reveals_child(),
            "the control that opens a drawer is the one that closes it"
        );
        overflow.emit_by_name::<()>("overflow", &[]);

        popover.set_day("Today", &few);
        assert!(
            !drawer().reveals_child(),
            "back under the cap the drawer closes rather than standing open on nothing"
        );

        popover.set_day("Tuesday", &few);
        assert_eq!(
            popover.imp().everything.title().as_deref(),
            Some("Tuesday"),
            "the drawer holds the whole of one day, so it is named after that day"
        );

        let placeholder = || popover.imp().states.visible_child_name();
        assert_eq!(
            placeholder().as_deref(),
            Some("nothing"),
            "a day with nothing on it is the wording the template opens with"
        );
        popover.set_day_truncated(true);
        assert_eq!(
            placeholder().as_deref(),
            Some("truncated"),
            "both wordings live in the template, so neither is a Rust string no translator sees"
        );
        popover.set_day_truncated(false);
        assert_eq!(placeholder().as_deref(), Some("nothing"));

        popover.imp().calendar.show_month(2026, 12);
        let months = Rc::new(RefCell::new(Vec::new()));
        popover.connect_month_shown({
            let months = Rc::clone(&months);
            move |_, year, month| months.borrow_mut().push((year, month))
        });

        popover.imp().calendar.step(1);
        popover.imp().calendar.step(1);
        assert_eq!(
            months.borrow().as_slice(),
            [(2027, 1), (2027, 2)],
            "the panel asks the daemon for the months it is showing, so every step has to reach it"
        );
        assert_eq!(popover.shown_month(), (2027, 2));

        let next = NextEventPopover::new();
        let hero = || next.imp().hero.clone();
        let quiet = (hero().title(), hero().subtitle());
        assert!(
            quiet.0.is_some() && quiet.1.is_some(),
            "the empty wording is the template's own, so no translator has to find it in Rust"
        );

        next.set_footer(None);
        assert!(!child_named::<gtk4::Box>(&next, "popover-shell__footer").is_visible());
        next.set_footer(Some("Open calendar"));
        assert!(child_named::<gtk4::Box>(&next, "popover-shell__footer").is_visible());

        next.set_countdown(None);
        assert!(
            !next.imp().countdown.get_visible(),
            "an all-day entry has no minute to count, and an empty readout still costs its slot"
        );
        next.set_countdown(Some(("12", "min")));
        assert!(next.imp().countdown.get_visible());

        next.set_upcoming(&[]);
        assert!(next.imp().upcoming.empty());
        next.set_upcoming(&few);
        assert!(!next.imp().upcoming.empty());

        next.set_heading("Design review", Some("14:00–15:00"));
        assert_eq!(hero().title().as_deref(), Some("Design review"));

        next.set_nothing();
        assert_eq!(
            (hero().title(), hero().subtitle()),
            quiet,
            "the last event ending restores the wording rather than leaving a finished one up"
        );
        assert!(!next.imp().countdown.get_visible());
        assert!(
            !next.imp().upcoming.empty(),
            "nothing is *next* — the list reaches further than the bar does, so emptying it here \
             would wipe entries the horizon still holds"
        );

        popover.imp().calendar.select(Ymd::new(2027, 2, 3));
        assert_eq!(
            months.borrow().len(),
            2,
            "picking a day inside the month already shown asks the daemon for nothing new"
        );

        let mpris = MprisPopover::new();
        let seen = Rc::new(RefCell::new(Vec::new()));
        mpris.connect_raise_requested({
            let seen = seen.clone();
            move |_, key| seen.borrow_mut().push(("raise", key))
        });
        mpris.connect_toggle_requested({
            let seen = seen.clone();
            move |_, key| seen.borrow_mut().push(("toggle", key))
        });

        mpris.set_others(None);
        assert!(
            !mpris.imp().others.is_visible(),
            "a section switched off is hidden, not shown holding its own placeholder"
        );

        mpris.set_others(Some(&[]));
        assert!(
            mpris.imp().others.is_visible() && mpris.imp().others.empty(),
            "one player running is not an empty popover, it is a popover with no others"
        );
        mpris.set_others(Some(&[
            Player {
                key: "firefox".to_owned(),
                name: "Firefox".to_owned(),
                icon_name: "firefox".to_owned(),
                title: "Texas Sun".to_owned(),
                artist: "Khruangbin".to_owned(),
                playing: false,
            },
            Player {
                key: "mpv".to_owned(),
                name: "mpv".to_owned(),
                icon_name: "mpv".to_owned(),
                title: "Pisces".to_owned(),
                artist: "Jinjer".to_owned(),
                playing: true,
            },
        ]));
        assert!(!mpris.imp().others.empty());

        let rows = mpris.imp().list.imp().rows.borrow().clone();
        rows[1].emit_by_name::<()>("clicked", &[]);
        rows[0].emit_by_name::<()>("toggled", &[]);
        assert_eq!(
            *seen.borrow(),
            [
                ("raise", "mpv".to_owned()),
                ("toggle", "firefox".to_owned())
            ],
            "a row reports the key of whatever player currently occupies it, through the popover"
        );

        mpris.set_footer(None);
        assert!(!child_named::<gtk4::Box>(&mpris, "popover-shell__footer").is_visible());
        mpris.set_footer(Some("Open Spotify"));
        assert!(child_named::<gtk4::Box>(&mpris, "popover-shell__footer").is_visible());

        assert!(
            mpris.player().scrubber().is_ancestor(&mpris),
            "the primary player is composed whole, not reassembled out of its parts here"
        );

        let weather = WeatherPopover::new();
        let weather_drawer = weather.imp().drawer.get();

        let page = |key: &str, title: &str| WeatherPage {
            key: key.to_owned(),
            title: title.to_owned(),
            description: None,
            facts: vec![Fact::new("High", "18 °C")],
        };

        weather.set_heading("weather-showers-symbolic", "Vilnius", Some("Light rain"));
        weather.set_reading(Some(("18", "°")));
        assert!(child_named::<Readout>(&weather, "readout").get_visible());
        weather.set_reading(None);
        assert!(
            !child_named::<Readout>(&weather, "readout").get_visible(),
            "no reading reserves no space in the hero"
        );

        weather.set_hours(&[]);
        weather.set_days(&[]);
        assert!(
            !weather.imp().hourly.get_visible() && !weather.imp().hourly_rule.get_visible(),
            "an empty strip takes its hairline with it"
        );
        weather.set_days(&[Day {
            label: "Today".to_owned(),
            icon_name: "weather-clear-symbolic".to_owned(),
            precipitation: None,
            low: 11.0,
            high: 18.0,
        }]);
        assert!(weather.imp().daily.get_visible() && weather.imp().daily_rule.get_visible());

        assert!(!weather_drawer.reveals_child());
        weather.open("day0");
        assert!(
            !weather_drawer.reveals_child(),
            "a page nothing built cannot be opened"
        );

        weather.set_pages(&[page("day0", "Tomorrow"), page("day1", "Wednesday")]);
        weather.open("day0");
        assert_eq!(weather.is_open().as_deref(), Some("day0"));
        weather.open("day1");
        assert_eq!(
            weather.is_open().as_deref(),
            Some("day1"),
            "another page switches rather than closing"
        );
        weather.open("day1");
        assert_eq!(
            weather.is_open(),
            None,
            "the control that opened the drawer is the one that closes it"
        );

        weather.open("day1");
        weather.set_pages(&[page("day0", "Tomorrow")]);
        assert_eq!(
            weather.is_open(),
            None,
            "a drawer must not stand open on a page that has gone away"
        );

        let advisory = |title: &str, page: Option<&str>, severity| Advisory {
            severity,
            icon_name: "dialog-warning-symbolic".to_owned(),
            title: title.to_owned(),
            subtitle: Some("LHMT".to_owned()),
            page: page.map(str::to_owned),
        };

        assert!(children_of::<Notice>(&weather.imp().alerts.get()).is_empty());
        assert!(
            !weather.imp().alerts.get_visible(),
            "an empty alert box would still cost the space between it and the nowcast"
        );
        weather.set_pages(&[page("day0", "Tomorrow"), page("alert0", "Storm")]);
        weather.set_alerts(&[
            advisory("Thunderstorm warning", Some("alert0"), Severity::Error),
            advisory("Wind advisory", None, Severity::Warning),
        ]);

        let raised = children_of::<Notice>(&weather.imp().alerts.get());
        assert_eq!(raised.len(), 2);
        assert!(raised[0].has_css_class("notice--error"));
        assert!(
            raised[0].can_target() && !raised[1].can_target(),
            "a notice with a page leads somewhere and one without it only states something"
        );

        raised[0].emit_by_name::<()>("clicked", &[]);
        assert_eq!(weather.is_open().as_deref(), Some("alert0"));

        raised[0].emit_by_name::<()>("clicked", &[]);
        assert_eq!(
            weather.is_open(),
            None,
            "one handler, not one per reconcile: a second click closes rather than reopening"
        );

        weather.set_alerts(&[advisory("Wind advisory", None, Severity::Warning)]);
        assert_eq!(
            children_of::<Notice>(&weather.imp().alerts.get()).len(),
            1,
            "a cleared alert is unparented rather than left behind empty"
        );
        weather.set_alerts(&[]);
        assert!(children_of::<Notice>(&weather.imp().alerts.get()).is_empty());
        assert!(!weather.imp().alerts.get_visible());

        weather.set_nowcast(None);
        assert!(!weather.imp().nowcast.get_visible());
        weather.set_nowcast(Some(&advisory(
            "Rain starting in 25 minutes",
            None,
            Severity::Info,
        )));
        assert!(weather.imp().nowcast.get_visible());

        weather.set_footer(None);
        assert!(!weather.imp().footer.get_visible());

        let strip = TrayStrip::new();
        let chip = |key: &str| TrayChip {
            key: key.to_owned(),
            spec: spec(key),
        };
        let shown_keys = |strip: &TrayStrip| -> Vec<String> {
            strip
                .imp()
                .shown
                .borrow()
                .iter()
                .map(|(key, _)| key.clone())
                .collect()
        };

        strip.set_items(&[chip("a"), chip("b"), chip("c")]);
        assert!(strip.is_visible());
        assert_eq!(shown_keys(&strip), ["a", "b", "c"]);
        assert!(
            strip.imp().hidden.borrow().is_empty(),
            "no cap means no overflow"
        );
        let chevron = strip.imp().chevron.borrow().clone().expect("chevron");
        assert!(!chevron.get_visible(), "a chevron over nothing is hidden");

        let first = strip.imp().shown.borrow()[0].1.clone();
        strip.set_items(&[chip("c"), chip("a"), chip("b")]);
        assert_eq!(shown_keys(&strip), ["c", "a", "b"]);
        assert_eq!(
            strip.imp().shown.borrow()[1].1,
            first,
            "a key keeps its widget across a reorder rather than rebuilding it"
        );

        strip.set_max_visible(2);
        assert_eq!(shown_keys(&strip), ["c", "a"]);
        assert_eq!(strip.imp().hidden.borrow().len(), 1);
        assert!(chevron.get_visible());

        let drawer = strip.imp().drawer.borrow().clone().expect("drawer");
        assert!(!drawer.reveals_child());
        chevron.set_active(true);
        assert!(drawer.reveals_child(), "the chevron opens the overflow");
        chevron.set_active(false);
        assert!(
            !drawer.reveals_child(),
            "the control that opens the overflow closes it"
        );

        chevron.set_active(true);
        strip.set_max_visible(0);
        assert!(
            !drawer.reveals_child() && !chevron.get_visible(),
            "an emptied overflow does not leave its drawer standing open on nothing"
        );

        let fired = Rc::new(RefCell::new(Vec::new()));
        strip.connect_activated({
            let fired = Rc::clone(&fired);
            move |_, key, button| fired.borrow_mut().push((key, button))
        });
        strip.emit_by_name::<()>("activated", &[&"c".to_owned(), &3u32]);
        assert_eq!(*fired.borrow(), [("c".to_owned(), 3u32)]);

        let scrolled = Rc::new(RefCell::new(Vec::new()));
        strip.connect_scrolled({
            let scrolled = Rc::clone(&scrolled);
            move |_, key, dx, dy| scrolled.borrow_mut().push((key, dx, dy))
        });
        strip.emit_by_name::<()>("scrolled", &[&"a".to_owned(), &0.0f64, &-1.0f64]);
        assert_eq!(*scrolled.borrow(), [("a".to_owned(), 0.0f64, -1.0f64)]);

        let order = |strip: &TrayStrip| -> Vec<String> {
            let mut names = Vec::new();
            let mut child = strip.first_child();
            while let Some(node) = child {
                names.push(node.type_().name().to_owned());
                child = node.next_sibling();
            }
            names
        };
        assert_eq!(
            order(&strip),
            ["GtkToggleButton", "GtkRevealer", "GtkBox"],
            "the chevron leads, so the overflow opens into the bar and not off its edge"
        );
        assert!(
            chevron.has_css_class("flat"),
            "the chevron keeps its frameless class, or the theme paints a button behind the icon"
        );
        assert_eq!(
            chevron.icon_name().as_deref(),
            Some("pan-start-symbolic"),
            "the chevron points back over the closed drawer"
        );
        strip.set_max_visible(2);
        chevron.set_active(true);
        assert!(
            chevron.has_css_class("tray-strip__chevron--open"),
            "the open drawer turns the chevron the way its chips travelled"
        );
        chevron.set_active(false);
        assert!(!chevron.has_css_class("tray-strip__chevron--open"));
        strip.set_max_visible(0);
        strip.set_overflow_edge(Edge::End);
        assert_eq!(
            order(&strip),
            ["GtkBox", "GtkRevealer", "GtkToggleButton"],
            "the other edge mirrors it, for an applet on the other side of the panel"
        );
        assert_eq!(
            chevron.icon_name().as_deref(),
            Some("pan-end-symbolic"),
            "and the chevron turns with it"
        );
        strip.set_orientation(gtk4::Orientation::Vertical);
        assert_eq!(chevron.icon_name().as_deref(), Some("pan-down-symbolic"));
        strip.set_orientation(gtk4::Orientation::Horizontal);
        strip.set_overflow_edge(Edge::Start);

        strip.set_items(&[]);
        assert!(!strip.is_visible(), "an empty strip reserves no bar space");

        let card = TooltipCard::new();
        assert!(
            !card.get_visible(),
            "a card with nothing in it shows no tooltip rather than an empty box"
        );
        card.set_title(Some("Nextcloud"));
        assert!(card.get_visible());
        assert_eq!(card.title().as_deref(), Some("Nextcloud"));

        let long = "ы".repeat(tooltip_card::TITLE_MAX_CHARS * 2);
        card.set_title(Some(long.as_str()));
        assert_eq!(
            card.title().unwrap_or_default().chars().count(),
            tooltip_card::TITLE_MAX_CHARS,
            "a hostile title is cut by characters, not bytes"
        );

        card.set_body(Some("Synced\nLast sync 2 minutes ago"));
        assert_eq!(
            card.body().as_deref(),
            Some("Synced\nLast sync 2 minutes ago")
        );
        card.set_status(Some("Needs attention"));
        assert_eq!(card.status().as_deref(), Some("Needs attention"));

        card.set_title(None::<&str>);
        card.set_body(None::<&str>);
        card.set_status(None::<&str>);
        assert!(
            !card.get_visible(),
            "emptying every field hides the card again"
        );
        card.set_icon(Some(&gio::ThemedIcon::new("folder-symbolic").upcast()));
        assert!(card.get_visible(), "an icon alone is still a tooltip");
        assert!(
            !card.imp().text.get_visible(),
            "an icon-only card reserves no text column"
        );
    }

    fn texture(width: i32, height: i32) -> gdk::Texture {
        gdk::MemoryTexture::new(
            width,
            height,
            gdk::MemoryFormat::R8g8b8,
            &glib::Bytes::from_owned(vec![0u8; (width * height * 3) as usize]),
            (width * 3) as usize,
        )
        .upcast()
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

    fn all_named(parent: &impl IsA<gtk4::Widget>, class: &str) -> Vec<gtk4::Widget> {
        fn walk(widget: &gtk4::Widget, class: &str, found: &mut Vec<gtk4::Widget>) {
            if widget.has_css_class(class) {
                found.push(widget.clone());
            }
            let mut child = widget.first_child();
            while let Some(candidate) = child {
                walk(&candidate, class, found);
                child = candidate.next_sibling();
            }
        }

        let mut found = Vec::new();
        walk(parent.as_ref(), class, &mut found);
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
