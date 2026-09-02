mod calendar;
mod choice_list;
mod dots;
mod event_list;
mod fact_list;
mod forecast;
mod hero;
mod indicator;
mod indicator_group;
mod notice;
mod now_playing;
mod pager;
mod panel;
mod placeholder;
mod player_list;
mod popover_shell;
mod range_bar;
mod readout;
pub(crate) mod row;
mod scrubber;
mod section;
mod split_row;
mod theme;
mod transport;
mod world_clock;

pub use calendar::{Calendar, Ymd};
pub use choice_list::{Choice, ChoiceList};
pub use event_list::{Event, EventList, EventRow};
pub use fact_list::{Fact, FactList};
pub use forecast::{Day, ForecastDay, ForecastHour, ForecastList, ForecastStrip, Hour};
pub use hero::Hero;
pub use indicator::{Indicator, IndicatorSpec};
pub use indicator_group::IndicatorGroup;
pub use notice::{Notice, Severity};
pub use now_playing::NowPlaying;
pub use pager::{Focus, Pager, PagerItem, Shape, Slot};
pub use panel::Panel;
pub use placeholder::Placeholder;
pub use player_list::{Player, PlayerList, PlayerRow};
pub use popover_shell::PopoverShell;
pub use range_bar::RangeBar;
pub use readout::Readout;
pub use row::Row;
pub use scrubber::Scrubber;
pub use section::Section;
pub use split_row::SplitRow;
pub use theme::Styles;
pub use transport::{Repeat, Transport, TransportAction};
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
    use gtk4::prelude::*;

    let (icon, tooltip) = match playing {
        true => ("media-playback-pause-symbolic", "Pause"),
        false => ("media-playback-start-symbolic", "Play"),
    };
    button.set_icon_name(icon);
    button.set_tooltip_text(Some(tooltip));
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
    use gtk4::gdk;
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
        let _styles = Styles::install();

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
            "Today is meaningless on the month that contains today"
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

        let players = PlayerList::new();
        players.set_players(&[
            Player {
                name: "Firefox".to_owned(),
                icon_name: "web-browser-symbolic".to_owned(),
                title: "How the Chip Shortage Ends".to_owned(),
                artist: "Odd Lots".to_owned(),
                playing: false,
            },
            Player {
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
            move |_, index| promoted.borrow_mut().push(index)
        });
        let toggled = Rc::new(RefCell::new(Vec::new()));
        players.connect_toggled({
            let toggled = Rc::clone(&toggled);
            move |_, index| toggled.borrow_mut().push(index)
        });
        player_rows[1].emit_clicked();
        toggle.emit_clicked();
        assert_eq!(*promoted.borrow(), [1u32]);
        assert_eq!(
            *toggled.borrow(),
            [1u32],
            "the button in the trail is a second gesture on the same row, and each carries the \
             same index so one list handles both"
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

        let activated: Rc<RefCell<Vec<u64>>> = Rc::new(RefCell::new(Vec::new()));
        pager.connect_activated({
            let activated = Rc::clone(&activated);
            move |_, id| activated.borrow_mut().push(id)
        });
        pager.set_slots(&[slot(21, "1"), slot(22, "2")]);
        children_of::<PagerItem>(&pager)[0].emit_clicked();
        assert_eq!(
            *activated.borrow(),
            [21],
            "an item reports the id currently at its position, not the one it was built with"
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
        pager.set_shape(Shape::Numbers);
        assert!(
            label.is_visible(),
            "numbers is the shape that shows the label"
        );
        assert!(
            pager.has_css_class("pager--numbers") && !pager.has_css_class("pager--dots"),
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
