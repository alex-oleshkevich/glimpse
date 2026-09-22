mod artwork;
mod audio_popover;
mod battery_popover;
mod bluetooth_pairing_dialog;
mod bluetooth_popover;
mod brightness_popover;
mod calendar;
mod calendar_popover;
mod choice_list;
mod clipboard_list;
mod clipboard_popover;
mod display_list;
mod display_popover;
mod dots;
pub mod drawer;
mod event_list;
mod fact_list;
mod fader;
mod forecast;
mod hero;
mod idle_popover;
mod indicator;
mod indicator_group;
mod inhibitor_list;
mod keyboard_popover;
mod monitors;
mod mpris_popover;
mod network_popover;
mod network_secret_dialog;
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
mod places_popover;
mod player_list;
mod popover_shell;
mod printing_popover;
mod privacy_popover;
mod range_bar;
mod readout;
mod reconcile;
mod removable_popover;
pub(crate) mod row;
mod scrubber;
mod section;
mod session_popover;
mod source_list;
mod split_row;
mod switch_row;
mod theme;
mod tooltip_card;
mod transport;
mod tray_strip;
mod weather_popover;
mod workspace_list;
mod workspace_name_popover;
mod workspace_section;
mod workspaces_popover;
mod world_clock;

pub use artwork::{artwork, thumbnail};
pub use audio_popover::{
    AudioPopover, Block as AudioBlock, Details as AudioDetails, Entry as AudioEntry,
};
pub use battery_popover::{
    BatteryPopover, ChargeLimit as BatteryChargeLimit, Device as BatteryDevice,
};
pub use bluetooth_pairing_dialog::{
    Entry as PairingEntry, PASSKEY_MAX, PIN_MAX, PairingAnswer, PairingDialog,
};
pub use bluetooth_popover::{
    Ask as BluetoothAsk, BluetoothPopover, Details as BluetoothDetails, Entry as BluetoothEntry,
    Line as BluetoothLine, Place as BluetoothPlace,
};
pub use brightness_popover::{BrightnessPopover, NightLight};
pub use calendar::{Calendar, Ymd};
pub use calendar_popover::CalendarPopover;
pub use choice_list::{Choice, ChoiceList};
pub use clipboard_list::{Actions as ClipActions, Clip, ClipboardList};
pub use clipboard_popover::ClipboardPopover;
pub use display_list::{Display, DisplayList, DisplayLogical, DisplayMode};
pub use display_popover::DisplayPopover;
pub use event_list::{Event, EventList, EventRow};
pub use fact_list::{Fact, FactList};
pub use fader::Fader;
pub use forecast::{Day, ForecastDay, ForecastHour, ForecastList, ForecastStrip, Hour};
pub use hero::Hero;
pub use idle_popover::IdlePopover;
pub use indicator::{Indicator, IndicatorSpec};
pub use indicator_group::IndicatorGroup;
pub use inhibitor_list::{InhibitorEntry, InhibitorList, InhibitorSource, InhibitorTargets};
pub use keyboard_popover::{KeyboardPopover, Layout as KeyboardLayout};
pub use monitors::watch_monitors;
pub use mpris_popover::MprisPopover;
pub use network_popover::{
    Ask as NetworkAsk, Details as NetworkDetails, Entered as NetworkEntered, Entry as NetworkEntry,
    Line as NetworkLine, NetworkPopover, Place as NetworkPlace,
};
pub use network_secret_dialog::{SecretAnswer, SecretDialog};
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
pub use places_popover::{Entry as PlacesEntry, PlacesPopover, Trash as PlacesTrash};
pub use player_list::{Player, PlayerList, PlayerRow};
pub use popover_shell::PopoverShell;
pub use printing_popover::{
    Detail as PrintingDetail, Job as PrintingJob, Printer as PrintingPrinter, PrintingPopover,
};
pub use privacy_popover::{PrivacyPopover, Usage as PrivacyUsage};
pub use range_bar::RangeBar;
pub use readout::Readout;
pub use removable_popover::{Drive as RemovableDrive, RemovablePopover, Volume as RemovableVolume};
pub use row::Row;
pub use scrubber::{Scrubber, clock};
pub use section::Section;
pub use session_popover::{
    ActionState as SessionActionState, HIBERNATE, LOCK, LOG_OUT, POWER_OFF, REBOOT, SUSPEND,
    SessionChoice, SessionPopover,
};
pub use source_list::{Source, SourceList};
pub use split_row::SplitRow;
pub use switch_row::SwitchRow;
pub use theme::{Sheets, Styles};
pub use tooltip_card::TooltipCard;
pub use transport::{Repeat, Transport, TransportAction};
pub use tray_strip::{Edge, TrayChip, TrayStrip};
pub use weather_popover::{Advisory, Page as WeatherPage, WeatherPopover, alert_page, day_page};
pub use workspace_list::{Window as WorkspaceWindow, Workspace, WorkspaceList};
pub use workspace_name_popover::WorkspaceNamePopover;
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
    set_text_capped(label, value, TEXT_MAX_CHARS);
}

/// The same setter for a field whose own cap is not the shared one. Passing a longer string through
/// `set_text` silently re-truncates it to `TEXT_MAX_CHARS`, which makes the caller's constant a lie
/// no test of that constant can catch.
pub(crate) fn set_text_capped(label: &gtk4::Label, value: Option<&str>, cap: usize) {
    use gtk4::prelude::*;

    let text = truncate(value.unwrap_or_default(), cap);
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

pub(crate) fn percent_of(value: f64, maximum: f64) -> f64 {
    if maximum <= 0.0 {
        return 0.0;
    }
    (value / maximum * 100.0).round()
}

pub(crate) fn percent_text(value: f64) -> String {
    use gettextrs::gettext;

    gettext("{percent}%").replace("{percent}", &value.round().to_string())
}

pub fn register_resources() -> Result<(), glib::Error> {
    gio::resources_register_include!("glimpse-widgets.gresource")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bluetooth_pairing_dialog::PIN_MAX;
    use adw::prelude::AlertDialogExt;
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

        let inhibitor_list = InhibitorList::new();
        assert!(children_of::<gtk4::Box>(&inhibitor_list).is_empty());
        assert_eq!(
            inhibitor_list.measure(gtk4::Orientation::Vertical, -1).1,
            0,
            "an empty list reserves no height"
        );

        fn inhibitor(id: u64, label: &str, can_release: bool) -> InhibitorEntry {
            InhibitorEntry {
                id,
                source: InhibitorSource::ScreenSaver,
                label: label.to_owned(),
                status: "screen sharing".to_owned(),
                targets: InhibitorTargets {
                    idle: true,
                    ..InhibitorTargets::default()
                },
                can_release,
            }
        }

        inhibitor_list.set_inhibitors(&[inhibitor(1, "Zoom", true), inhibitor(2, "OBS", false)]);
        let holders = children_of::<gtk4::Box>(&inhibitor_list);
        assert_eq!(holders.len(), 2);
        let first = holders[0].first_child().unwrap().downcast::<Row>().unwrap();
        let _chevron = child_named::<gtk4::Image>(&first, "drawer-chevron");
        let first_panel = holders[0]
            .last_child()
            .unwrap()
            .downcast::<gtk4::Revealer>()
            .unwrap();
        let second_panel = holders[1]
            .last_child()
            .unwrap()
            .downcast::<gtk4::Revealer>()
            .unwrap();
        let card = first_panel
            .child()
            .unwrap()
            .downcast::<gtk4::Box>()
            .unwrap();
        let cancel = children_of::<Row>(&card).remove(0);
        assert!(cancel.activatable());
        assert!(!first_panel.reveals_child());

        let touched_row = first.clone();
        touched_row.set_title(Some("Touched by hand"));
        inhibitor_list.set_inhibitors(&[inhibitor(1, "Zoom", true), inhibitor(2, "OBS", false)]);
        assert_eq!(
            touched_row.title().as_deref(),
            Some("Touched by hand"),
            "an unchanged slice is not re-applied: re-rendering it would overwrite what was \
             touched by hand"
        );

        let title_label = child_named::<gtk4::Label>(&first, "row__title");
        let title_changes = Rc::new(Cell::new(0u32));
        title_label.connect_label_notify({
            let title_changes = Rc::clone(&title_changes);
            move |_| title_changes.set(title_changes.get() + 1)
        });
        inhibitor_list.set_inhibitors(&[
            inhibitor(1, "Zoom (sharing)", true),
            inhibitor(2, "OBS", false),
        ]);
        assert_eq!(
            title_changes.get(),
            1,
            "a changed slice re-applies the row it changed, undoing the hand touch"
        );

        let reported = Rc::new(RefCell::new(Vec::<u64>::new()));
        inhibitor_list.connect_release_requested({
            let reported = Rc::clone(&reported);
            move |_, id| reported.borrow_mut().push(id)
        });
        first.emit_clicked();
        assert!(first_panel.reveals_child());
        assert!(inhibitor_list.is_open());
        assert!(cancel.get_visible());
        assert!(
            reported.borrow().is_empty(),
            "opening details does not cancel"
        );
        let second = holders[1].first_child().unwrap().downcast::<Row>().unwrap();
        second.emit_clicked();
        assert!(!first_panel.reveals_child());
        assert!(second_panel.reveals_child());
        let second_card = second_panel
            .child()
            .unwrap()
            .downcast::<gtk4::Box>()
            .unwrap();
        let second_cancel = children_of::<Row>(&second_card).remove(0);
        assert!(!second_cancel.get_visible());
        second.emit_clicked();
        assert!(!second_panel.reveals_child());
        first.emit_clicked();
        cancel.emit_clicked();
        assert_eq!(*reported.borrow(), vec![1]);

        inhibitor_list.set_inhibitors(&[inhibitor(9, "Zoom", true), inhibitor(2, "OBS", false)]);
        let reused = children_of::<gtk4::Box>(&inhibitor_list);
        assert_eq!(
            reused[0], holders[0],
            "row 0 is reused in place, not rebuilt"
        );
        assert!(
            !inhibitor_list.is_open(),
            "a removed inhibitor closes its details"
        );
        cancel.emit_clicked();
        assert_eq!(
            *reported.borrow(),
            vec![1, 9],
            "the same action row now reports the id its row currently represents, not the id \
             captured when the row was first built"
        );

        let entry_without_tabs = InhibitorEntry {
            id: 5,
            source: InhibitorSource::Login1,
            label: "systemd-inhibit".to_owned(),
            status: "installing updates".to_owned(),
            targets: InhibitorTargets {
                idle: true,
                shutdown: true,
                power_key: true,
                ..InhibitorTargets::default()
            },
            can_release: false,
        };
        inhibitor_list.set_inhibitors(&[entry_without_tabs]);
        let holder_without_tabs = children_of::<gtk4::Box>(&inhibitor_list).remove(0);
        let row_without_tabs = holder_without_tabs
            .first_child()
            .unwrap()
            .downcast::<Row>()
            .unwrap();
        let chip_icon = child_named::<gtk4::Image>(&row_without_tabs, "row__icon");
        assert_eq!(
            chip_icon.icon_name().as_deref(),
            Some("system-run-symbolic")
        );
        let panel = holder_without_tabs
            .last_child()
            .unwrap()
            .downcast::<gtk4::Revealer>()
            .unwrap();
        let card = panel.child().unwrap().downcast::<gtk4::Box>().unwrap();
        let cancel = children_of::<Row>(&card).remove(0);
        row_without_tabs.emit_clicked();
        assert!(panel.reveals_child());
        assert!(
            !cancel.get_visible(),
            "an external inhibitor cannot be canceled"
        );
        let facts = children_of::<FactList>(&card).remove(0);
        let values = children_of::<Row>(&facts)
            .iter()
            .filter_map(Row::value)
            .collect::<Vec<_>>();
        assert!(values.iter().any(|value| value.contains("Shutdown")));
        let hostile = "ё".repeat(TEXT_MAX_CHARS * 2);
        inhibitor_list.set_inhibitors(&[InhibitorEntry {
            id: 7,
            source: InhibitorSource::Portal,
            label: hostile.clone(),
            status: hostile,
            targets: InhibitorTargets::default(),
            can_release: false,
        }]);
        let hostile_holder = children_of::<gtk4::Box>(&inhibitor_list).remove(0);
        let hostile_row = hostile_holder
            .first_child()
            .unwrap()
            .downcast::<Row>()
            .unwrap();
        let hostile_title = child_named::<gtk4::Label>(&hostile_row, "row__title");
        let hostile_subtitle = child_named::<gtk4::Label>(&hostile_row, "row__subtitle");
        assert_eq!(hostile_title.text().chars().count(), TEXT_MAX_CHARS);
        assert_eq!(hostile_subtitle.text().chars().count(), TEXT_MAX_CHARS);

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
        assert!(
            children_of::<Row>(&forecast).is_empty(),
            "a day is parented with the panel that unfolds under it, not directly"
        );
        forecast.imp().rows.borrow()[1].emit_clicked();
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

        let toggle = SwitchRow::new();
        let flips = Rc::new(RefCell::new(Vec::new()));
        toggle.connect_toggled({
            let flips = Rc::clone(&flips);
            move |_, on| flips.borrow_mut().push(on)
        });

        toggle
            .upcast_ref::<Row>()
            .set_title(Some("Connect automatically"));
        let knob = child_named::<gtk4::Switch>(&toggle, "switch-row__knob");
        toggle.set_active(true);
        assert!(toggle.active() && knob.is_active());
        assert!(
            flips.borrow().is_empty(),
            "showing the state the caller already knows about must not report it back"
        );

        toggle.emit_clicked();
        assert_eq!(
            *flips.borrow(),
            [false],
            "the row body is the target too, and it reports exactly one flip"
        );
        assert!(!toggle.active(), "the knob follows the row it belongs to");

        knob.set_active(true);
        assert_eq!(
            *flips.borrow(),
            [false, true],
            "the knob and the row body share one emitter, so neither doubles the other"
        );

        assert!(!toggle.locked());
        toggle.set_locked(true);
        assert!(!knob.is_sensitive(), "a locked row disables the knob");
        assert!(
            toggle.is_sensitive(),
            "the row itself stays sensitive so its subtitle is not dimmed along with the knob"
        );

        let sensitive_changes = Rc::new(Cell::new(0u32));
        knob.connect_sensitive_notify({
            let sensitive_changes = Rc::clone(&sensitive_changes);
            move |_| sensitive_changes.set(sensitive_changes.get() + 1)
        });
        toggle.set_locked(true);
        assert_eq!(
            sensitive_changes.get(),
            0,
            "locking an already-locked row does not touch the knob a second time"
        );

        let flips_before = flips.borrow().len();
        toggle.emit_clicked();
        assert_eq!(
            flips.borrow().len(),
            flips_before,
            "a locked row's body click is inert"
        );
        assert!(
            toggle.active(),
            "a locked row does not change value on a body click"
        );

        toggle.set_active(false);
        assert!(
            !toggle.active() && !knob.is_active(),
            "render still reaches a locked switch"
        );
        assert_eq!(
            flips.borrow().len(),
            flips_before,
            "a programmatic set stays quiet under lock, same as unlocked"
        );

        toggle.set_locked(false);
        assert_eq!(sensitive_changes.get(), 1);
        assert!(knob.is_sensitive());
        toggle.emit_clicked();
        assert_eq!(
            flips.borrow().len(),
            flips_before + 1,
            "unlocking gives the row body its click back"
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

        let fader = Fader::new();
        let mute = child_named::<gtk4::ToggleButton>(&fader, "fader__mute");
        let track = child_named::<gtk4::Scale>(&fader, "fader__track");
        fader.set_value(62.0);
        assert_eq!(fader.value(), 62.0);

        fader.set_value(150.0);
        assert_eq!(
            fader.value(),
            100.0,
            "a device already above 100% is pinned at the top of the fader's own range rather \
             than pretending the extra headroom exists"
        );

        let redraws = Rc::new(Cell::new(0u32));
        track.connect_value_changed({
            let redraws = Rc::clone(&redraws);
            move |_| redraws.set(redraws.get() + 1)
        });
        fader.set_value(100.0);
        assert_eq!(redraws.get(), 0, "an unchanged value is not reapplied");

        assert_eq!(
            (
                track.adjustment().step_increment(),
                track.adjustment().page_increment()
            ),
            (1.0, 5.0),
            "the increments are set from Rust, as a function of the configured maximum, \
             because blueprint-compiler's adjustment rule rejects an adjustment carrying \
             anything besides lower, upper and value"
        );

        let changed = Rc::new(RefCell::new(Vec::new()));
        fader.connect_changed({
            let changed = Rc::clone(&changed);
            move |_, value| changed.borrow_mut().push(value)
        });
        fader.set_value(38.0);
        assert!(
            changed.borrow().is_empty(),
            "a programmatic set_value renders the new position without reporting it back, or a \
             state update arriving from the backend would look exactly like a drag"
        );

        fader.imp().held.set(Some(38.0));
        fader.set_value(80.0);
        assert_eq!(
            fader.value(),
            38.0,
            "a value arriving mid-drag loses to the drag in progress"
        );
        fader.imp().held.set(None);
        fader.set_value(80.0);
        assert_eq!(fader.value(), 80.0);

        let toggles = Rc::new(RefCell::new(Vec::new()));
        fader.connect_toggled({
            let toggles = Rc::clone(&toggles);
            move |_, muted| toggles.borrow_mut().push(muted)
        });

        fader.set_muted(true);
        assert!(mute.is_active() && fader.muted());
        assert!(
            toggles.borrow().is_empty(),
            "showing a muted state the caller already knows about must not report it back"
        );

        fader.set_muted(true);
        assert!(
            toggles.borrow().is_empty(),
            "an unchanged muted flag is not reapplied"
        );

        mute.set_active(false);
        assert_eq!(
            *toggles.borrow(),
            [false],
            "a click on the mute button is the one thing that reports back"
        );

        assert_eq!(fader.icon_name(), None);
        let icon_changes = Rc::new(Cell::new(0u32));
        mute.connect_icon_name_notify({
            let icon_changes = Rc::clone(&icon_changes);
            move |_| icon_changes.set(icon_changes.get() + 1)
        });
        fader.set_icon_name(Some("audio-volume-high-symbolic".to_owned()));
        assert_eq!(
            fader.icon_name().as_deref(),
            Some("audio-volume-high-symbolic")
        );
        assert_eq!(icon_changes.get(), 1);
        fader.set_icon_name(Some("audio-volume-high-symbolic".to_owned()));
        assert_eq!(icon_changes.get(), 1, "an equal icon name is not reapplied");

        let fader = Fader::new();
        assert_eq!(fader.maximum(), 100.0);

        let track = child_named::<gtk4::Scale>(&fader, "fader__track");
        let page_notifies = Rc::new(Cell::new(0u32));
        track
            .adjustment()
            .connect_notify_local(Some("page-increment"), {
                let page_notifies = Rc::clone(&page_notifies);
                move |_, _| page_notifies.set(page_notifies.get() + 1)
            });

        fader.set_maximum(3.0);
        assert_eq!(
            (
                track.adjustment().step_increment(),
                track.adjustment().page_increment()
            ),
            (1.0, 1.0),
            "max(1, max/100) and max(1, max/20) both floor at 1 on a three-position control"
        );
        assert_eq!(page_notifies.get(), 1);
        fader.set_value(4.0);
        assert_eq!(
            fader.value(),
            3.0,
            "set_value clamps to the configured maximum, never a literal 100"
        );
        fader.set_maximum(3.0);
        assert_eq!(
            page_notifies.get(),
            1,
            "an unchanged maximum is not reapplied"
        );

        let fader = Fader::new();
        fader.set_value(60.0);
        fader.set_maximum(3.0);
        assert_eq!(
            fader.value(),
            3.0,
            "lowering the maximum below the current value must not leave the slider drawn \
             past the end of its own track"
        );

        let fader = Fader::new();
        fader.set_maximum(-5.0);
        assert_eq!(
            fader.maximum(),
            0.0,
            "a negative maximum is clamped inside the setter itself — a paramspec minimum \
             bound was tried first and measured to panic on this exact input, since \
             glib-rs treats the C side's own silent coercion as an error"
        );
        fader.set_value(1.0);
        assert_eq!(
            fader.value(),
            0.0,
            "a maximum of zero leaves nothing to set a positive value to"
        );

        let fader = Fader::new();
        fader.set_maximum(400000.0);
        let track = child_named::<gtk4::Scale>(&fader, "fader__track");
        assert_eq!(
            (
                track.adjustment().step_increment(),
                track.adjustment().page_increment()
            ),
            (4000.0, 20000.0),
            "increments scale with a wide native range"
        );
        let changed = Rc::new(RefCell::new(Vec::new()));
        fader.connect_changed({
            let changed = Rc::clone(&changed);
            move |_, value| changed.borrow_mut().push(value)
        });
        let _: bool = track.emit_by_name("change-value", &[&gtk4::ScrollType::None, &400000.0f64]);
        assert_eq!(
            *changed.borrow(),
            [400000.0],
            "GtkRange's own class handler re-clamps the adjustment to its current bounds \
             after every connected handler runs, so value() settles at 400000.0 whether or \
             not this handler's own clamp used the configured maximum — only the changed \
             signal, read from inside the handler before that class handler runs, tells a \
             fixed clamp apart from a literal 100.0"
        );

        let fader = Fader::new();
        assert_eq!(fader.floor(), 0.0);
        assert_eq!(fader.maximum(), 100.0);

        fader.set_floor(5.0);
        assert_eq!(fader.floor(), 5.0);
        fader.set_value(0.0);
        assert_eq!(
            fader.value(),
            5.0,
            "set_value clamps to the configured floor, never a literal 0"
        );

        fader.set_value(50.0);
        let track = child_named::<gtk4::Scale>(&fader, "fader__track");
        let changed = Rc::new(RefCell::new(Vec::new()));
        fader.connect_changed({
            let changed = Rc::clone(&changed);
            move |_, value| changed.borrow_mut().push(value)
        });
        let _: bool = track.emit_by_name("change-value", &[&gtk4::ScrollType::None, &0.0f64]);
        assert_eq!(
            *changed.borrow(),
            [5.0],
            "GtkRange's own class handler re-clamps the adjustment to its current bounds \
             after every connected handler runs, so value() settles at the floor whether or \
             not this handler's own clamp used the configured floor — only the changed \
             signal, read from inside the handler before that class handler runs, tells a \
             floor-aware clamp apart from a literal 0.0"
        );

        let fader = Fader::new();
        fader.set_value(10.0);
        fader.set_floor(50.0);
        assert_eq!(
            fader.value(),
            50.0,
            "GtkAdjustment::set_lower does not re-clamp value on its own, so a raised floor \
             pulls the value up to it explicitly"
        );

        let fader = Fader::new();
        fader.set_floor(150.0);
        assert_eq!(
            fader.floor(),
            100.0,
            "a floor above the current maximum is clamped down to it rather than inverting \
             the range"
        );
        assert_eq!(
            fader.maximum(),
            100.0,
            "the maximum itself is left untouched"
        );

        let fader = Fader::new();
        fader.set_floor(-5.0);
        assert_eq!(
            fader.floor(),
            0.0,
            "a negative floor is clamped inside the setter itself, exactly as a negative \
             maximum already is"
        );

        let fader = Fader::new();
        assert!(fader.toggleable());
        let mute = child_named::<gtk4::ToggleButton>(&fader, "fader__mute");
        let icon = child_named::<gtk4::Image>(&fader, "fader__icon");
        assert!(mute.get_visible());
        assert!(!icon.get_visible());

        fader.set_toggleable(false);
        assert!(!fader.toggleable());
        assert!(!mute.get_visible());
        assert!(icon.get_visible());
        assert!(
            fader.is_sensitive(),
            "a plain icon presentation has nothing to mute and must not disable the fader"
        );
        assert!(
            icon.is_sensitive(),
            "an insensitive image renders dimmed, which is the exact bug being fixed"
        );

        fader.set_icon_name(Some("display-brightness-symbolic".to_owned()));
        assert_eq!(
            icon.icon_name().as_deref(),
            Some("display-brightness-symbolic"),
            "icon-name sets the icon under either presentation"
        );

        let visible_notifies = Rc::new(Cell::new(0u32));
        icon.connect_notify_local(Some("visible"), {
            let visible_notifies = Rc::clone(&visible_notifies);
            move |_, _| visible_notifies.set(visible_notifies.get() + 1)
        });
        fader.set_toggleable(false);
        assert_eq!(
            visible_notifies.get(),
            0,
            "an unchanged toggleable is not reapplied"
        );

        fader.set_toggleable(true);
        assert!(mute.get_visible());
        assert!(!icon.get_visible());

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
        assert!(
            !next.imp().upcoming.get_visible(),
            "the popover opens for one event; a caption over an empty list is the empty state \
             this applet does not have"
        );
        next.set_upcoming(&few);
        assert!(next.imp().upcoming.get_visible());

        next.set_heading("Design review", Some("14:00–15:00"));
        assert_eq!(hero().title().as_deref(), Some("Design review"));

        next.set_join(Some((
            "Join Google Meet",
            "meet.google.com/aaa-bbbb-ccc",
            "https://meet.google.com/aaa-bbbb-ccc",
        )));
        assert!(next.imp().join.get_visible());
        next.set_join(None);
        assert!(!next.imp().join.get_visible());

        next.set_facts(&[Fact::new("Calendar", "Work")]);
        assert!(next.imp().details.get_visible());
        next.set_facts(&[]);
        assert!(!next.imp().details.get_visible());

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

        let day_panel = |index: usize| -> Option<gtk4::Revealer> {
            weather
                .imp()
                .days
                .imp()
                .holders
                .borrow()
                .get(index)
                .and_then(|holder| holder.last_child())
                .and_downcast::<gtk4::Revealer>()
        };
        let alert_notices = |weather: &WeatherPopover| -> Vec<Notice> {
            children_of::<gtk4::Box>(&weather.imp().alerts.get())
                .iter()
                .filter_map(|holder| holder.first_child().and_downcast::<Notice>())
                .collect()
        };

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

        assert_eq!(weather.is_open(), None);
        weather.open("day0");
        assert_eq!(
            weather.is_open(),
            None,
            "a detail nothing built cannot be opened"
        );

        weather.set_days(&[
            Day {
                label: "Tomorrow".to_owned(),
                icon_name: "weather-clear-symbolic".to_owned(),
                precipitation: None,
                low: 11.0,
                high: 18.0,
            },
            Day {
                label: "Wednesday".to_owned(),
                icon_name: "weather-clear-symbolic".to_owned(),
                precipitation: None,
                low: 12.0,
                high: 19.0,
            },
        ]);
        weather.set_pages(&[page("day0", "Tomorrow"), page("day1", "Wednesday")]);
        weather.open("day0");
        assert_eq!(weather.is_open().as_deref(), Some("day0"));
        assert!(
            day_panel(0).is_some_and(|panel| panel.is_ancestor(&weather.imp().days.get())),
            "a day's detail unfolds inside the list it belongs to, not beside the column"
        );
        assert!(
            children_of::<ForecastDay>(&weather.imp().days.get()).is_empty(),
            "every day travels with its own panel, so the list's children are holders"
        );
        assert!(
            weather.imp().hero.has_css_class("receded")
                && weather.imp().hourly.has_css_class("receded"),
            "an open detail is read against a quiet card, so everything else recedes"
        );
        weather.open("day1");
        assert_eq!(
            weather.is_open().as_deref(),
            Some("day1"),
            "another day switches rather than closing"
        );
        assert!(
            day_panel(0).is_some_and(|panel| !panel.reveals_child()),
            "opening one day closes the one that was open"
        );
        weather.open("day1");
        assert_eq!(
            weather.is_open(),
            None,
            "the control that opened the detail is the one that closes it"
        );

        weather.open("day1");
        weather.set_pages(&[page("day0", "Tomorrow")]);
        assert_eq!(
            weather.is_open(),
            None,
            "a detail must not stand open on a page that has gone away"
        );
        assert!(
            !weather.imp().hero.has_css_class("receded"),
            "and the card it was read against comes back up with it"
        );

        let advisory = |title: &str, page: Option<&str>, severity| Advisory {
            severity,
            icon_name: "dialog-warning-symbolic".to_owned(),
            title: title.to_owned(),
            subtitle: Some("LHMT".to_owned()),
            page: page.map(str::to_owned),
        };

        assert!(alert_notices(&weather).is_empty());
        assert!(
            !weather.imp().alerts.get_visible(),
            "an empty alert box would still cost the space between it and the nowcast"
        );
        weather.set_pages(&[page("day0", "Tomorrow"), page("alert0", "Storm")]);
        weather.set_alerts(&[
            advisory("Thunderstorm warning", Some("alert0"), Severity::Error),
            advisory("Wind advisory", None, Severity::Warning),
        ]);

        let raised = alert_notices(&weather);
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

        let ordered = WeatherPopover::new();
        ordered.set_alerts(&[advisory(
            "Thunderstorm warning",
            Some("alert0"),
            Severity::Error,
        )]);
        ordered.set_pages(&[page("alert0", "Storm")]);
        ordered.open("alert0");
        assert_eq!(
            ordered.is_open().as_deref(),
            Some("alert0"),
            "a notice and its page arrive through different setters, and neither order may lose it"
        );

        weather.set_alerts(&[advisory("Wind advisory", None, Severity::Warning)]);
        assert_eq!(
            alert_notices(&weather).len(),
            1,
            "a cleared alert is unparented rather than left behind empty"
        );
        weather.set_alerts(&[]);
        assert!(alert_notices(&weather).is_empty());
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

        let long_body = "é".repeat(tooltip_card::BODY_MAX_CHARS + 40);
        card.set_body(Some(long_body.as_str()));
        assert_eq!(
            card.body().unwrap_or_default().chars().count(),
            tooltip_card::BODY_MAX_CHARS,
            "the card's own cap is what applies; the shared setter's 128 would silently win"
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

        let popover = BluetoothPopover::new();
        assert!(
            popover.imp().nearby.property::<bool>("empty"),
            "a section with no rows starts empty, or the first scan draws a heading over nothing"
        );
        popover.set_adapter(
            "Bluetooth",
            "No device connected",
            "bluetooth-symbolic",
            true,
            true,
        );
        assert!(popover.imp().power.is_active());
        assert!(!popover.imp().connected.get_visible());

        let entry = |id: &str, place: BluetoothPlace| BluetoothEntry {
            id: id.to_owned(),
            title: id.to_owned(),
            subtitle: "Headset".to_owned(),
            icon: "audio-headset-symbolic".to_owned(),
            place,
            value: String::new(),
            selected: false,
            busy: false,
        };
        popover.set_entries(&[
            entry("a", BluetoothPlace::Connected),
            entry("b", BluetoothPlace::Paired),
            entry("c", BluetoothPlace::Paired),
        ]);
        assert!(popover.imp().connected.get_visible());
        assert_eq!(popover.imp().paired_held.borrow().len(), 2);
        assert!(
            !popover.imp().nearby.get_visible(),
            "nearby is a scan, not a resting state"
        );

        let first = popover
            .imp()
            .connected_held
            .borrow()
            .first()
            .cloned()
            .expect("a connected row")
            .1;
        popover.set_entries(&[
            entry("a", BluetoothPlace::Connected),
            entry("b", BluetoothPlace::Paired),
        ]);
        assert_eq!(
            popover
                .imp()
                .connected_held
                .borrow()
                .first()
                .map(|(_, holder)| holder.clone()),
            Some(first),
            "a key that stays keeps its widget, and its panel with it"
        );

        let acted = Rc::new(RefCell::new(Vec::new()));
        popover.connect_acted({
            let acted = Rc::clone(&acted);
            move |_, id, action| acted.borrow_mut().push(format!("{id}/{action}"))
        });

        let panel = |id: &str| -> gtk4::Revealer {
            let imp = popover.imp();
            for held in [&imp.connected_held, &imp.paired_held, &imp.nearby_held] {
                if let Some((_, holder)) = held.borrow().iter().find(|(key, _)| key == id) {
                    return holder
                        .last_child()
                        .and_downcast::<gtk4::Revealer>()
                        .expect("a device row carries its own panel");
                }
            }
            panic!("no row is holding {id}");
        };
        let panel_rows = |id: &str| -> Vec<Row> {
            let page = panel(id)
                .child()
                .and_downcast::<gtk4::Box>()
                .expect("an opened panel has a page");
            children_of::<Row>(&page)
        };
        let head = |id: &str| -> gtk4::Widget {
            let imp = popover.imp();
            for held in [&imp.connected_held, &imp.paired_held, &imp.nearby_held] {
                if let Some((_, holder)) = held.borrow().iter().find(|(key, _)| key == id) {
                    return holder.first_child().expect("a holder leads with its row");
                }
            }
            panic!("no row is holding {id}");
        };

        assert!(
            popover
                .imp()
                .paired_held
                .borrow()
                .iter()
                .all(|(_, holder)| {
                    holder
                        .last_child()
                        .and_downcast::<gtk4::Revealer>()
                        .is_some_and(|panel| !panel.reveals_child())
                }),
            "a device reveals nothing until it is selected"
        );
        popover.set_details(Some(&BluetoothDetails {
            id: "a".to_owned(),
            lines: vec![
                BluetoothLine {
                    action: "disconnect".to_owned(),
                    title: "Disconnect".to_owned(),
                    activates: true,
                    ..Default::default()
                },
                BluetoothLine {
                    action: "trust".to_owned(),
                    title: "Connect automatically".to_owned(),
                    toggle: Some(false),
                    ..Default::default()
                },
                BluetoothLine {
                    action: "address".to_owned(),
                    title: "Address".to_owned(),
                    value: "F8:4E:17:BC:EE:D5".to_owned(),
                    ..Default::default()
                },
                BluetoothLine {
                    action: "forget".to_owned(),
                    title: "Forget this device".to_owned(),
                    activates: true,
                    ..Default::default()
                },
            ],
        }));
        assert!(
            panel("a").reveals_child(),
            "the detail belongs under the device it describes, not beside the list"
        );
        assert!(
            !head("a").has_css_class("receded"),
            "the device that was opened is the one thing that must not recede"
        );
        assert!(
            head("b").has_css_class("receded") && popover.imp().hero.has_css_class("receded"),
            "everything the panel is read against recedes while it is open"
        );

        let lines = panel_rows("a");
        assert_eq!(lines.len(), 4);
        assert!(
            lines[1].clone().downcast::<SwitchRow>().is_ok(),
            "a line carrying a toggle is a SwitchRow, not a row with a switch dropped in it"
        );
        assert!(
            lines[0].activatable() && lines[3].activatable(),
            "a line that acts keeps the pointer"
        );
        let toggled = Rc::new(RefCell::new(Vec::new()));
        popover.connect_toggled({
            let toggled = Rc::clone(&toggled);
            move |_, id, action, on| toggled.borrow_mut().push(format!("{id}/{action}/{on}"))
        });
        lines[1].emit_clicked();
        lines[1].emit_clicked();
        assert_eq!(
            *toggled.borrow(),
            ["a/trust/true", "a/trust/false"],
            "a toggle line acts through its own row body, exactly once per press and both ways"
        );
        assert!(
            !lines[2].activatable(),
            "a line that only states something must not light up under a hover leading nowhere"
        );
        lines[0].emit_by_name::<()>("clicked", &[]);
        assert_eq!(*acted.borrow(), ["a/disconnect"]);

        let switch = lines[1]
            .trail()
            .and_downcast::<gtk4::Switch>()
            .expect("a toggle row carries a switch");
        assert!(!switch.is_active());
        popover.set_details(Some(&BluetoothDetails {
            id: "a".to_owned(),
            lines: vec![
                BluetoothLine {
                    action: "disconnect".to_owned(),
                    title: "Disconnect".to_owned(),
                    activates: true,
                    ..Default::default()
                },
                BluetoothLine {
                    action: "trust".to_owned(),
                    title: "Connect automatically".to_owned(),
                    toggle: Some(true),
                    ..Default::default()
                },
                BluetoothLine {
                    action: "address".to_owned(),
                    title: "Address".to_owned(),
                    value: "F8:4E:17:BC:EE:D5".to_owned(),
                    ..Default::default()
                },
            ],
        }));
        assert!(
            switch.is_active(),
            "a reused row must follow the backend, not the last click"
        );
        assert_eq!(
            *acted.borrow(),
            ["a/disconnect"],
            "redressing the switch is not the user toggling it"
        );

        popover.set_details(Some(&BluetoothDetails {
            id: "b".to_owned(),
            lines: vec![
                BluetoothLine {
                    action: "disconnect".to_owned(),
                    title: "Disconnect".to_owned(),
                    activates: true,
                    ..Default::default()
                },
                BluetoothLine {
                    action: "trust".to_owned(),
                    title: "Connect automatically".to_owned(),
                    toggle: Some(false),
                    ..Default::default()
                },
            ],
        }));
        assert!(
            !panel("a").reveals_child() && panel("b").reveals_child(),
            "opening one device closes the one that was open"
        );
        acted.borrow_mut().clear();
        panel_rows("b")[0].emit_by_name::<()>("clicked", &[]);
        assert_eq!(
            *acted.borrow(),
            ["b/disconnect"],
            "a row whose action key repeats across devices must not act on the one no longer shown"
        );

        popover.set_scanning(true);
        popover.set_entries(&[
            entry("a", BluetoothPlace::Connected),
            entry("b", BluetoothPlace::Paired),
            entry("n", BluetoothPlace::Nearby),
        ]);
        popover.set_details(Some(&BluetoothDetails {
            id: "n".to_owned(),
            lines: vec![BluetoothLine {
                action: "pair".to_owned(),
                title: "Pair this device".to_owned(),
                activates: true,
                ..Default::default()
            }],
        }));
        assert!(
            panel("n").reveals_child() && popover.imp().hero.has_css_class("receded"),
            "a nearby device opens like any other"
        );
        popover.set_scanning(false);
        assert!(
            !popover.imp().hero.has_css_class("receded")
                && !head("a").has_css_class("receded")
                && !panel("n").reveals_child(),
            "the scan ending takes the nearby card away, so nothing may stay dimmed against it"
        );
        popover.set_details(None);
        popover.set_entries(&[
            entry("a", BluetoothPlace::Connected),
            entry("b", BluetoothPlace::Paired),
        ]);

        let width =
            |popover: &BluetoothPopover| popover.measure(gtk4::Orientation::Horizontal, -1).1;
        {
            let probe = BluetoothPopover::new();
            probe.set_adapter(
                "Bluetooth",
                "No device connected",
                "bluetooth-symbolic",
                true,
                true,
            );
            probe.set_entries(&[entry("a", BluetoothPlace::Connected)]);
            let floor = width(&probe);

            probe.set_adapter(
                "Bluetooth",
                "Connected to Bose QuietComfort Ultra and 2 others",
                "bluetooth-active-symbolic",
                true,
                true,
            );
            assert_eq!(
                width(&probe),
                floor,
                "a hero subtitle that grows with the device list widened the whole card"
            );

            probe.set_details(Some(&BluetoothDetails {
                id: "a".to_owned(),
                lines: vec![BluetoothLine {
                    action: "services".to_owned(),
                    title: "Services".to_owned(),
                    value: "Audio, Calls, Remote control, Network, File transfer".to_owned(),
                    ..Default::default()
                }],
            }));
            assert_eq!(
                width(&probe),
                floor,
                "a row's value has to stop asking for room at some point, or opening a device \
                 resizes the card under the pointer that opened it"
            );

            probe.set_details(None);
            let answers = Rc::new(RefCell::new(Vec::new()));
            probe.connect_answered({
                let answers = Rc::clone(&answers);
                move |_, accepted| answers.borrow_mut().push(accepted)
            });

            probe.set_prompt(Some(&BluetoothAsk {
                key: "pair:/org/bluez/hci0/dev_a".to_owned(),
                device: "Pixel 9 Pro".to_owned(),
                question: "Is this the code shown on the device?".to_owned(),
                code: "419 274".to_owned(),
                progress: String::new(),
                accept: "Confirm".to_owned(),
                cancel: "Cancel".to_owned(),
                destructive: false,
            }));
            assert_eq!(
                width(&probe),
                floor,
                "a prompt taking over the card must not resize the surface it took over"
            );
            assert_eq!(
                probe.imp().pages.visible_child_name().as_deref(),
                Some("prompt")
            );
            assert!(
                !probe.imp().hero.get_sensitive() && !probe.imp().footer.get_sensitive(),
                "a prompt owns the surface, so nothing behind it stays pressable"
            );
            assert!(probe.imp().prompt_accept.get_visible());
            assert!(probe.imp().prompt_code.get_visible());

            probe.set_prompt(Some(&BluetoothAsk {
                key: "pair:/org/bluez/hci0/dev_long".to_owned(),
                device: "HP OfficeJet 8025".to_owned(),
                question: "Type this PIN on the device.".to_owned(),
                code: "0".repeat(32),
                progress: String::new(),
                accept: String::new(),
                cancel: "Cancel".to_owned(),
                destructive: false,
            }));
            assert_eq!(
                width(&probe),
                floor,
                "a code the peer chose must not be able to widen the card"
            );

            probe.imp().prompt_accept.emit_by_name::<()>("clicked", &[]);
            probe.imp().prompt_cancel.emit_by_name::<()>("clicked", &[]);
            assert_eq!(*answers.borrow(), [true, false]);

            probe.set_prompt(Some(&BluetoothAsk {
                key: "pair:/org/bluez/hci0/dev_b".to_owned(),
                device: "UE BOOM 3".to_owned(),
                question: "This device wants to pair with this computer.".to_owned(),
                code: String::new(),
                progress: String::new(),
                accept: String::new(),
                cancel: "Deny".to_owned(),
                destructive: false,
            }));
            assert!(
                !probe.imp().prompt_accept.get_visible(),
                "a prompt with nothing to accept offers only the way out"
            );
            assert!(!probe.imp().prompt_code.get_visible());
            assert!(
                probe
                    .imp()
                    .prompt_actions
                    .has_css_class("prompt__actions--bare")
            );

            probe.set_prompt(None);
            assert_eq!(
                probe.imp().pages.visible_child_name().as_deref(),
                Some("devices"),
                "an answered prompt hands the card back to the device list"
            );
            assert!(probe.imp().hero.get_sensitive() && probe.imp().footer.get_sensitive());
        }
        popover.set_details(None);
        let closed = width(&popover);
        popover.set_details(Some(&BluetoothDetails {
            id: "a".to_owned(),
            lines: vec![BluetoothLine {
                action: "address".to_owned(),
                title: "Address".to_owned(),
                value: "F8:4E:17:BC:EE:D5".to_owned(),
                ..Default::default()
            }],
        }));
        assert_eq!(
            width(&popover),
            closed,
            "a card that widens when a device opens moves every row under the pointer that opened it"
        );

        popover.set_details(None);
        assert!(!panel("a").reveals_child() && !panel("b").reveals_child());
        assert!(
            !head("a").has_css_class("receded") && !popover.imp().hero.has_css_class("receded"),
            "closing the panel gives the card back"
        );

        let flips = Rc::new(RefCell::new(Vec::new()));
        popover.connect_scanning({
            let flips = Rc::clone(&flips);
            move |_, on| flips.borrow_mut().push(("search", on))
        });
        popover.connect_discoverable({
            let flips = Rc::clone(&flips);
            move |_, on| flips.borrow_mut().push(("discoverable", on))
        });

        assert!(!popover.imp().search.active());
        popover.set_scanning(true);
        assert!(popover.imp().search.active());
        assert!(popover.imp().nearby.get_visible());
        assert!(
            popover.imp().nearby.property::<bool>("empty"),
            "a scan with nothing found yet shows the placeholder, not a header over nothing"
        );
        popover.set_scanning(false);
        assert!(!popover.imp().search.active());

        popover.set_discoverable(true);
        assert!(popover.imp().discoverable.active());
        popover.set_discoverable(false);
        assert!(
            flips.borrow().is_empty(),
            "reconciling a switch from the adapter must not look like the user flipping it, or \
             every published state would fire a command back at bluez"
        );

        popover.set_discoverable(true);
        flips.borrow_mut().clear();
        popover.imp().search.emit_clicked();
        popover.imp().discoverable.emit_clicked();
        assert_eq!(
            *flips.borrow(),
            [("search", true), ("discoverable", false)],
            "a press moves the knob, and the knob's notify is the one emitter"
        );
        assert!(popover.imp().search.active() && !popover.imp().discoverable.active());

        popover.set_controls_sensitive(false);
        assert!(
            !popover.imp().search.get_sensitive() && !popover.imp().discoverable.get_sensitive()
        );
        popover.set_controls_sensitive(true);

        let dialog = PairingDialog::new();
        let answers = Rc::new(RefCell::new(Vec::new()));
        dialog.connect_answered({
            let answers = Rc::clone(&answers);
            move |_, answer| answers.borrow_mut().push(answer)
        });

        let rewrites = Rc::new(Cell::new(0u32));
        dialog.connect_heading_notify({
            let rewrites = Rc::clone(&rewrites);
            move |_| rewrites.set(rewrites.get() + 1)
        });

        const KEYBOARD: &str = "/org/bluez/hci0/dev_K3";

        dialog.ask(KEYBOARD, "Keychron K3", PairingEntry::Pin);
        assert_eq!(rewrites.get(), 1);
        dialog.ask(KEYBOARD, "Keychron K3", PairingEntry::Pin);
        assert_eq!(
            rewrites.get(),
            1,
            "an unchanged heading must not be written again"
        );
        assert!(dialog.imp().entry.get_visible());
        assert!(
            !dialog.is_response_enabled("ok"),
            "an empty PIN cannot be sent"
        );
        dialog.imp().entry.set_text("0000");
        assert!(dialog.is_response_enabled("ok"));
        dialog.ask(KEYBOARD, "Keychron K3 Keyboard", PairingEntry::Pin);
        assert_eq!(
            dialog.imp().entry.text(),
            "0000",
            "bluez resolving the device name re-asks, and must not wipe a half-typed PIN"
        );
        dialog.ask(
            "/org/bluez/hci0/dev_OTHER",
            "Pixel 9 Pro",
            PairingEntry::Pin,
        );
        assert!(
            dialog.imp().entry.text().is_empty(),
            "a second device asking must not inherit the PIN typed for the first"
        );
        assert!(!dialog.is_response_enabled("ok"));
        dialog.ask(KEYBOARD, "Keychron K3", PairingEntry::Pin);
        dialog.imp().entry.set_text("0000");
        dialog.imp().entry.set_max_length(0);
        dialog.imp().entry.set_text(&"a".repeat(PIN_MAX + 1));
        assert!(
            !dialog.is_response_enabled("ok"),
            "seventeen characters is one past what BlueZ takes"
        );
        assert!(dialog.imp().entry.has_css_class("error"));

        dialog.ask(
            "/org/bluez/hci0/dev_BO_SE",
            "Bose QuietComfort 45",
            PairingEntry::Passkey,
        );
        assert_eq!(rewrites.get(), 2, "a different prompt does write it");
        dialog.imp().entry.set_max_length(0);
        dialog.imp().entry.set_text("1000000");
        assert!(
            !dialog.is_response_enabled("ok"),
            "a passkey above 999999 cannot be sent"
        );
        dialog.imp().entry.set_text("123456");
        assert!(dialog.is_response_enabled("ok"));
        assert_eq!(dialog.close_response(), "cancel");

        dialog.emit_by_name::<()>("response", &[&"ok".to_owned()]);
        assert_eq!(*answers.borrow(), [PairingAnswer::Passkey(123_456)]);

        answers.borrow_mut().clear();
        dialog.emit_by_name::<()>("response", &[&dialog.close_response().to_string()]);
        assert_eq!(
            *answers.borrow(),
            [PairingAnswer::Deny],
            "Esc closes with the close response, which is a refusal"
        );

        let network = NetworkPopover::new();
        network.set_radio(
            "Wi-Fi",
            "Skylink",
            "network-wireless-signal-good-symbolic",
            true,
            true,
        );

        let entries = vec![
            NetworkEntry {
                id: "/ap/1".to_owned(),
                title: "Skylink".to_owned(),
                subtitle: "WPA2 \u{b7} 5 GHz".to_owned(),
                icon: "network-wireless-signal-good-symbolic".to_owned(),
                place: NetworkPlace::Networks,
                secured: true,
                selected: true,
                busy: false,
            },
            NetworkEntry {
                id: "/s/1".to_owned(),
                title: "Skylink 2G".to_owned(),
                place: NetworkPlace::Known,
                ..Default::default()
            },
        ];
        network.set_entries(&entries);
        assert_eq!(
            all_named(&network, "network-popover__row").len(),
            2,
            "one row per entry, split across their sections"
        );

        let held = all_named(&network, "network-popover__row");
        let mut renamed = entries.clone();
        renamed[0].title = "Skylink 5G".to_owned();
        network.set_entries(&renamed);
        let again = all_named(&network, "network-popover__row");
        assert!(
            held[0] == again[0],
            "a row whose content changed is updated in place, not replaced"
        );
        assert_eq!(
            again[0]
                .clone()
                .downcast::<SplitRow>()
                .expect("a split row")
                .row()
                .title()
                .map(|title| title.to_string()),
            Some("Skylink 5G".to_owned()),
            "and it actually took the new title"
        );

        let net_panel = |id: &str| -> gtk4::Revealer {
            let imp = network.imp();
            for held in [
                &imp.network_held,
                &imp.known_held,
                &imp.wired_held,
                &imp.vpn_held,
            ] {
                if let Some((_, holder)) = held.borrow().iter().find(|(key, _)| key == id) {
                    return holder
                        .last_child()
                        .and_downcast::<gtk4::Revealer>()
                        .expect("a network row carries its own panel");
                }
            }
            panic!("no row is holding {id}");
        };
        let net_head = |id: &str| -> gtk4::Widget {
            let imp = network.imp();
            for held in [
                &imp.network_held,
                &imp.known_held,
                &imp.wired_held,
                &imp.vpn_held,
            ] {
                if let Some((_, holder)) = held.borrow().iter().find(|(key, _)| key == id) {
                    return holder.first_child().expect("a holder leads with its row");
                }
            }
            panic!("no row is holding {id}");
        };
        let net_rows = |id: &str| -> Vec<Row> {
            let page = net_panel(id)
                .child()
                .and_downcast::<gtk4::Box>()
                .expect("an opened panel has a page");
            children_of::<Row>(&page)
        };

        assert!(
            net_head("/ap/1")
                .downcast::<SplitRow>()
                .expect("a split row")
                .row()
                .trail()
                .is_some_and(|lock| lock.get_visible()),
            "a secured network says so with a padlock"
        );
        assert!(
            net_head("/s/1")
                .downcast::<SplitRow>()
                .expect("a split row")
                .row()
                .trail()
                .is_some_and(|lock| !lock.get_visible()),
            "and an unsecured one hides the same padlock rather than growing a second widget"
        );

        assert!(
            !net_panel("/ap/1").reveals_child(),
            "a network reveals nothing until it is selected"
        );

        let lines = vec![
            NetworkLine {
                action: "connect".to_owned(),
                title: "Connect".to_owned(),
                activates: true,
                ..Default::default()
            },
            NetworkLine {
                action: "autoconnect".to_owned(),
                title: "Connect automatically".to_owned(),
                toggle: Some(false),
                ..Default::default()
            },
            NetworkLine {
                action: "forget".to_owned(),
                title: "Forget this network".to_owned(),
                activates: true,
                ..Default::default()
            },
        ];
        network.set_details(Some(&NetworkDetails {
            id: "/ap/1".to_owned(),
            lines: lines.clone(),
        }));
        assert!(
            net_panel("/ap/1").reveals_child(),
            "the detail belongs under the network it describes, not beside the list"
        );
        assert!(
            !net_head("/ap/1").has_css_class("receded")
                && net_head("/s/1").has_css_class("receded")
                && network.imp().hero.has_css_class("receded"),
            "everything the panel is read against recedes while it is open"
        );
        let opened = net_rows("/ap/1");
        assert_eq!(opened.len(), 3);
        assert!(
            opened[1].clone().downcast::<SwitchRow>().is_ok(),
            "a line carrying a toggle is a SwitchRow, not a row with a switch dropped in it"
        );

        let acted = Rc::new(RefCell::new(Vec::new()));
        network.connect_acted({
            let acted = Rc::clone(&acted);
            move |_, id, action| acted.borrow_mut().push(format!("{id}/{action}"))
        });
        network.set_details(Some(&NetworkDetails {
            id: "/s/1".to_owned(),
            lines,
        }));
        assert!(
            !net_panel("/ap/1").reveals_child() && net_panel("/s/1").reveals_child(),
            "opening one network closes the one that was open"
        );
        net_rows("/s/1")[2].emit_by_name::<()>("clicked", &[]);
        assert_eq!(
            *acted.borrow(),
            ["/s/1/forget"],
            "a row whose action key repeats across networks must not forget the one no longer shown"
        );

        network.set_details(None);
        assert!(
            !net_panel("/s/1").reveals_child()
                && !net_head("/ap/1").has_css_class("receded")
                && !network.imp().hero.has_css_class("receded"),
            "closing the card takes the dimming with it"
        );

        network.set_entries(&[]);
        assert!(
            !network.imp().networks.get_visible(),
            "with no networks and no scan there is nothing to head"
        );
        network.set_scanning(true);
        assert!(
            network.imp().networks.get_visible() && network.imp().networks.empty(),
            "a scan that has found nothing yet is the whole reason the placeholder exists; a \
             hidden section shows it to nobody"
        );
        network.set_scanning(false);
        network.set_entries(&entries);

        let answers = Rc::new(RefCell::new(Vec::new()));
        network.connect_answered({
            let answers = Rc::clone(&answers);
            move |_, accepted, entered| answers.borrow_mut().push((accepted, entered.to_owned()))
        });

        assert!(
            !network.prompting() && network.imp().hero.is_sensitive(),
            "nothing is being asked, so the list is the page"
        );
        network.set_prompt(Some(&NetworkAsk {
            key: "hidden".to_owned(),
            network: "Hidden network".to_owned(),
            question: "Type the name the network broadcasts nothing about.".to_owned(),
            entered: NetworkEntered::Name,
            accept: "Continue".to_owned(),
            choices: Vec::new(),
            open_choice: None,
        }));
        assert!(
            network.prompting()
                && network.imp().prompt_name.get_visible()
                && !network.imp().prompt_secret.get_visible(),
            "a network with no name to show asks for one, not for a password"
        );
        assert!(
            !network.imp().hero.is_sensitive(),
            "the Wi-Fi switch is not a way out of a question that is being asked"
        );
        assert!(
            !network.can_submit(),
            "an empty box cannot be submitted, here as in the dialog"
        );

        network.type_in("Skylink Guest");
        assert!(network.can_submit());
        network
            .imp()
            .prompt_accept
            .emit_by_name::<()>("clicked", &[]);
        assert_eq!(
            *answers.borrow(),
            [(true, "Skylink Guest".to_owned())],
            "the name reaches the applet, which asks for the password next"
        );

        network.set_prompt(Some(&NetworkAsk {
            key: "secret:Skylink Guest".to_owned(),
            network: "Skylink Guest".to_owned(),
            question: "The network needs a password before this computer can join it.".to_owned(),
            entered: NetworkEntered::Secret,
            accept: "Connect".to_owned(),
            choices: Vec::new(),
            open_choice: None,
        }));
        assert!(
            network.imp().prompt_secret.get_visible() && !network.imp().prompt_name.get_visible(),
            "the second ask is a password box, with the peek icon a name box must not have"
        );
        assert!(
            !network.can_submit(),
            "a new question starts empty; carrying the name over would submit it as a password"
        );

        network.type_in("wrongpassword");
        assert!(network.can_submit());
        network.set_prompt(Some(&NetworkAsk {
            key: "802-11-wireless-security:Skylink Guest".to_owned(),
            network: "Skylink Guest".to_owned(),
            question: "Skylink Guest refused that password. Check it and try again.".to_owned(),
            entered: NetworkEntered::Secret,
            accept: "Try again".to_owned(),
            choices: Vec::new(),
            open_choice: None,
        }));
        assert!(
            !network.can_submit(),
            "a retry clears the password that was refused; leaving it invites sending it again"
        );

        network.set_prompt(Some(&NetworkAsk {
            key: "hidden-secret:Skylink Guest".to_owned(),
            network: "Skylink Guest".to_owned(),
            question: "Choose how the network is secured, then type its password.".to_owned(),
            entered: NetworkEntered::Secret,
            accept: "Connect".to_owned(),
            choices: vec![
                "None".to_owned(),
                "WEP".to_owned(),
                "WPA & WPA2 Personal".to_owned(),
                "WPA3 Personal".to_owned(),
            ],
            open_choice: Some(0),
        }));
        assert!(
            network.imp().prompt_security.get_visible(),
            "a hidden network has no beacon to read its security off, so the user picks it"
        );
        assert!(
            !network.imp().prompt_secret.get_visible() && network.can_submit(),
            "an open network takes no password, and demanding one makes it unjoinable"
        );
        network.imp().prompt_security.set_selected(2);
        assert!(
            network.imp().prompt_secret.get_visible() && !network.can_submit(),
            "choosing WPA brings the password box back, empty and unsubmittable"
        );
        assert_eq!(network.chosen(), 2);

        network.type_in("hunter2hunter2");
        answers.borrow_mut().clear();
        network
            .imp()
            .prompt_cancel
            .emit_by_name::<()>("clicked", &[]);
        assert_eq!(
            *answers.borrow(),
            [(false, String::new())],
            "cancelling discards what was typed rather than sending it"
        );

        network.set_prompt(None);
        assert!(
            !network.prompting()
                && network.imp().hero.is_sensitive()
                && network.imp().pages.visible_child_name().as_deref() == Some("networks"),
            "the question answered, the list comes back"
        );

        let secret = SecretDialog::new();
        secret.ask("psk:Skylink", "Skylink", false, NetworkEntered::Passphrase);
        let first = secret.heading().map(|one| one.to_string());
        assert!(
            !secret.body().is_empty(),
            "the first ask explains why it is asking"
        );

        secret.type_in("the-wrong-one");
        secret.ask("psk:Skylink", "Skylink", true, NetworkEntered::Passphrase);
        assert!(
            secret.is_blank(),
            "a retry clears the rejected password; leaving it invites retyping the same thing"
        );
        assert_ne!(
            secret.heading().map(|one| one.to_string()),
            first,
            "a retry must not look like the first ask, or the same password is typed again"
        );
        assert!(
            secret.heading().is_some_and(|one| !one.is_empty()),
            "the retry says the last password was refused"
        );

        network.set_overflow(Some("3 more networks"));
        network.set_overflow(None);

        let audio = AudioPopover::new();
        assert_eq!(audio.imp().hero.title().as_deref(), Some("Sound"));
        assert!(
            !audio.imp().output.has_css_class("accent")
                && !audio.imp().input.has_css_class("accent"),
            "neither fader is accented; both take the default white track the brightness fader uses"
        );

        audio.set_readout(Some("42%"));
        assert_eq!(audio.imp().readout.value().as_deref(), Some("42%"));
        audio.set_footer(Some("Sound settings"));
        assert!(audio.imp().footer.get_visible());

        let out_entries = vec![
            AudioEntry {
                id: "sink-1".to_owned(),
                title: "Speakers".to_owned(),
                icon: Some("audio-speakers-symbolic".to_owned()),
                selected: true,
                ..Default::default()
            },
            AudioEntry {
                id: "sink-2".to_owned(),
                title: "Headphones".to_owned(),
                ..Default::default()
            },
        ];
        audio.set_outputs(&out_entries);
        assert_eq!(
            all_named(&audio, "audio-popover__device").len(),
            2,
            "one row per output device"
        );
        assert!(audio.imp().outputs.get_visible());

        let held = all_named(&audio, "audio-popover__device");
        audio.set_outputs(&out_entries);
        assert_eq!(
            all_named(&audio, "audio-popover__device"),
            held,
            "an unchanged slice is not re-applied"
        );

        let mut renamed = out_entries.clone();
        renamed[0].title = "Studio monitors".to_owned();
        audio.set_outputs(&renamed);
        let again = all_named(&audio, "audio-popover__device");
        assert!(
            held[0] == again[0],
            "a row whose content changed is updated in place, not replaced"
        );
        assert_eq!(
            again[0]
                .clone()
                .downcast::<Row>()
                .expect("a device row")
                .title()
                .map(|title| title.to_string()),
            Some("Studio monitors".to_owned())
        );

        let selected = Rc::new(RefCell::new(Vec::new()));
        audio.connect_device_selected({
            let selected = Rc::clone(&selected);
            move |_, dir, id| selected.borrow_mut().push(format!("{dir}/{id}"))
        });
        again[1]
            .clone()
            .downcast::<Row>()
            .expect("a device row")
            .emit_clicked();
        assert_eq!(*selected.borrow(), ["output/sink-2"]);

        audio.set_outputs(&[]);
        assert!(
            !audio.imp().outputs.get_visible(),
            "with no output devices there is nothing to head"
        );
        audio.set_outputs(&out_entries);

        audio.set_inputs(&[AudioEntry {
            id: "source-1".to_owned(),
            title: "Microphone".to_owned(),
            ..Default::default()
        }]);
        assert!(audio.imp().inputs.get_visible());

        let levels = Rc::new(RefCell::new(Vec::new()));
        audio.connect_level_changed({
            let levels = Rc::clone(&levels);
            move |_, dir, value| levels.borrow_mut().push(format!("{dir}/{value}"))
        });
        let toggles = Rc::new(RefCell::new(Vec::new()));
        audio.connect_level_toggled({
            let toggles = Rc::clone(&toggles);
            move |_, dir, muted| toggles.borrow_mut().push(format!("{dir}/{muted}"))
        });

        audio.set_output_level(64.0, false, Some("audio-volume-high-symbolic"));
        assert_eq!(audio.imp().output.value(), 64.0);
        audio
            .imp()
            .output
            .emit_by_name::<()>("changed", &[&64.0f64]);
        audio.imp().input.emit_by_name::<()>("changed", &[&30.0f64]);
        assert_eq!(*levels.borrow(), ["output/64", "input/30"]);
        audio.imp().output.emit_by_name::<()>("toggled", &[&true]);
        assert_eq!(*toggles.borrow(), ["output/true"]);

        let apps = vec![
            AudioEntry {
                id: "app-a".to_owned(),
                title: "Firefox".to_owned(),
                value: Some("64%".to_owned()),
                ..Default::default()
            },
            AudioEntry {
                id: "app-b".to_owned(),
                title: "OBS Studio".to_owned(),
                ..Default::default()
            },
        ];
        audio.set_apps(&apps);
        assert_eq!(all_named(&audio, "audio-popover__app").len(), 2);
        assert!(!audio.imp().apps.empty());

        let app_panel = |id: &str| -> gtk4::Revealer {
            let imp = audio.imp();
            let held = imp.app_held.borrow();
            let (_, holder) = held.iter().find(|(key, _)| key == id).expect("a held app");
            holder
                .last_child()
                .and_downcast::<gtk4::Revealer>()
                .expect("an app row carries its own panel")
        };
        let app_head = |id: &str| -> gtk4::Widget {
            let imp = audio.imp();
            let held = imp.app_held.borrow();
            let (_, holder) = held.iter().find(|(key, _)| key == id).expect("a held app");
            holder.first_child().expect("a holder leads with its row")
        };

        assert!(
            !app_panel("app-a").reveals_child(),
            "an application reveals nothing until it is selected"
        );

        let app_selected = Rc::new(RefCell::new(Vec::new()));
        audio.connect_app_selected({
            let app_selected = Rc::clone(&app_selected);
            move |_, id| app_selected.borrow_mut().push(id.to_owned())
        });
        app_head("app-a")
            .downcast::<Row>()
            .expect("an app row")
            .emit_clicked();
        assert_eq!(*app_selected.borrow(), ["app-a"]);

        audio.set_details(Some(&AudioDetails {
            id: "app-a".to_owned(),
            blocks: vec![AudioBlock {
                dir: "output".to_owned(),
                volume: 64.0,
                adjustable: true,
                devices: vec![AudioEntry {
                    id: "sink-1".to_owned(),
                    title: "Speakers".to_owned(),
                    selected: true,
                    ..Default::default()
                }],
                ..Default::default()
            }],
        }));
        assert!(
            app_panel("app-a").reveals_child(),
            "the card belongs under the application it describes, not beside the list"
        );
        assert!(
            app_head("app-a").has_css_class("open") && !app_head("app-a").has_css_class("receded"),
            "recede marks exactly one row open"
        );
        assert!(
            app_head("app-b").has_css_class("receded") && !app_head("app-b").has_css_class("open"),
            "and every other application row recedes"
        );
        assert!(
            audio.imp().output.has_css_class("receded")
                && audio.imp().input.has_css_class("receded"),
            "both master faders recede while a card is open"
        );
        assert!(audio.imp().hero.has_css_class("receded"));
        assert!(audio.imp().footer.has_css_class("receded"));

        let card_blocks = |id: &str| -> Vec<gtk4::Box> {
            let panel = app_panel(id);
            let card = panel
                .child()
                .and_downcast::<gtk4::Box>()
                .expect("an opened panel has a card");
            children_of::<gtk4::Box>(&card)
        };
        let heading_label = |block: &gtk4::Box| -> gtk4::Label {
            children_of::<gtk4::Label>(block)
                .into_iter()
                .next()
                .expect("a block has a heading label")
        };
        let block_fader = |block: &gtk4::Box| -> Fader {
            children_of::<Fader>(block)
                .into_iter()
                .next()
                .expect("a block has a fader")
        };
        let block_devices_box = |block: &gtk4::Box| -> gtk4::Box {
            children_of::<gtk4::Box>(block)
                .into_iter()
                .next()
                .expect("a block has a device rows box")
        };

        let single = card_blocks("app-a");
        assert_eq!(
            single.len(),
            1,
            "an application with one role renders one block"
        );
        assert!(
            !heading_label(&single[0]).get_visible(),
            "a single-role application draws no heading"
        );
        assert_eq!(block_fader(&single[0]).value(), 64.0);
        assert!(block_fader(&single[0]).is_sensitive());

        audio.set_details(Some(&AudioDetails {
            id: "app-b".to_owned(),
            blocks: vec![
                AudioBlock {
                    dir: "output".to_owned(),
                    heading: Some("Output".to_owned()),
                    volume: 40.0,
                    adjustable: true,
                    ..Default::default()
                },
                AudioBlock {
                    dir: "input".to_owned(),
                    heading: Some("Input".to_owned()),
                    muted: true,
                    adjustable: false,
                    devices: vec![AudioEntry {
                        id: "source-1".to_owned(),
                        title: "Microphone".to_owned(),
                        selected: true,
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
        }));
        assert!(
            !app_panel("app-a").reveals_child(),
            "opening one application closes the one that was open"
        );
        assert!(app_panel("app-b").reveals_child());

        let pair = card_blocks("app-b");
        assert_eq!(
            pair.len(),
            2,
            "an application with two roles renders two blocks"
        );
        assert_eq!(heading_label(&pair[0]).text().as_str(), "Output");
        assert_eq!(
            heading_label(&pair[1]).text().as_str(),
            "Input",
            "blocks render in output-then-input order"
        );
        assert!(block_fader(&pair[0]).is_sensitive());
        assert!(
            !block_fader(&pair[1]).is_sensitive(),
            "a block with adjustable: false yields an insensitive fader"
        );

        let device_rows = children_of::<Row>(&block_devices_box(&pair[1]));
        assert_eq!(device_rows.len(), 1);
        assert!(device_rows[0].selected());

        let moved = Rc::new(RefCell::new(Vec::new()));
        audio.connect_app_moved({
            let moved = Rc::clone(&moved);
            move |_, app, dir, device| moved.borrow_mut().push(format!("{app}/{dir}/{device}"))
        });
        device_rows[0].emit_clicked();
        assert_eq!(*moved.borrow(), ["app-b/input/source-1"]);

        let app_levels = Rc::new(RefCell::new(Vec::new()));
        audio.connect_app_level_changed({
            let app_levels = Rc::clone(&app_levels);
            move |_, app, dir, value| app_levels.borrow_mut().push(format!("{app}/{dir}/{value}"))
        });
        block_fader(&pair[0]).emit_by_name::<()>("changed", &[&77.0f64]);
        assert_eq!(*app_levels.borrow(), ["app-b/output/77"]);

        let app_toggles = Rc::new(RefCell::new(Vec::new()));
        audio.connect_app_level_toggled({
            let app_toggles = Rc::clone(&app_toggles);
            move |_, app, dir, muted| {
                app_toggles
                    .borrow_mut()
                    .push(format!("{app}/{dir}/{muted}"))
            }
        });
        block_fader(&pair[1]).emit_by_name::<()>("toggled", &[&true]);
        assert_eq!(*app_toggles.borrow(), ["app-b/input/true"]);

        audio.set_details(None);
        assert!(
            !app_panel("app-a").reveals_child() && !app_panel("app-b").reveals_child(),
            "set_details(None) closes every holder"
        );
        assert!(
            !app_head("app-a").has_css_class("receded")
                && !app_head("app-b").has_css_class("receded"),
            "closing the card takes the dimming with it"
        );

        audio.set_details(Some(&AudioDetails {
            id: "app-ghost".to_owned(),
            blocks: Vec::new(),
        }));
        assert!(
            !app_panel("app-a").reveals_child() && !app_panel("app-b").reveals_child(),
            "an id not in the list opens nothing"
        );

        audio.set_apps(&[]);
        assert!(
            audio.imp().apps.empty(),
            "no applications is what the quiet placeholder exists for"
        );
        assert!(
            audio.imp().apps.get_visible(),
            "the Applications section stays, showing the placeholder rather than disappearing"
        );
        audio.set_apps(&apps);

        audio.set_overflow(Some("2 more outputs"), None, Some("3 more apps"));
        assert!(audio.imp().more_outputs.get_visible());
        assert!(!audio.imp().more_inputs.get_visible());
        assert!(audio.imp().more_apps.get_visible());

        let expanded = Rc::new(RefCell::new(Vec::new()));
        audio.connect_expanded({
            let expanded = Rc::clone(&expanded);
            move |_, place| expanded.borrow_mut().push(place.to_owned())
        });
        audio.imp().more_outputs.emit_clicked();
        assert_eq!(*expanded.borrow(), ["outputs"]);
        audio.set_overflow(None, None, None);
        assert!(!audio.imp().more_outputs.get_visible() && !audio.imp().more_apps.get_visible());

        let footer_activated = Rc::new(Cell::new(0u32));
        audio.connect_footer_activated({
            let footer_activated = Rc::clone(&footer_activated);
            move |_| footer_activated.set(footer_activated.get() + 1)
        });
        audio.imp().footer.emit_clicked();
        assert_eq!(footer_activated.get(), 1);

        let sources = SourceList::new();
        assert!(
            sources.first_child().is_none(),
            "a source list renders nothing before the first set"
        );

        let source = |key: &str, name: &str, value: f64, maximum: f64, floor: f64| Source {
            key: key.to_owned(),
            name: name.to_owned(),
            value,
            maximum,
            floor,
        };

        let changed = Rc::new(RefCell::new(Vec::<(String, f64)>::new()));
        sources.connect_changed({
            let changed = Rc::clone(&changed);
            move |_, key, value| changed.borrow_mut().push((key, value))
        });

        sources.set_sources(&[
            source("built-in", "Built-in", 40.0, 100.0, 0.0),
            source("dp-1", "DP-1", 60.0, 100.0, 0.0),
            source("dp-2", "DP-2", 80.0, 100.0, 5.0),
        ]);

        let rows: Vec<Row> = children_of(&sources);
        let faders: Vec<Fader> = children_of(&sources);
        assert_eq!(rows.len(), 3);
        assert_eq!(faders.len(), 3);
        assert_eq!(rows[0].title().as_deref(), Some("Built-in"));
        assert_eq!(rows[2].title().as_deref(), Some("DP-2"));
        assert!(
            !rows[0].activatable(),
            "a titled row above a fader takes no click of its own"
        );
        assert_eq!(
            rows[0].parent(),
            Some(sources.clone().upcast::<gtk4::Widget>())
        );
        assert_eq!(
            faders[0].parent(),
            Some(sources.clone().upcast::<gtk4::Widget>()),
            "the fader is a sibling of the row, not a child of it, so the row's own \
             non-activatable state cannot reach it"
        );

        assert_eq!(faders[0].value(), 40.0);
        assert_eq!(faders[2].value(), 80.0);

        assert_eq!(
            faders[2].floor(),
            5.0,
            "a source's own floor becomes the fader's floor"
        );
        assert_eq!(faders[2].maximum(), 100.0);

        faders[1].emit_by_name::<()>("changed", &[&65.0f64]);
        assert_eq!(
            *changed.borrow(),
            vec![("dp-1".to_owned(), 65.0)],
            "moving one fader reports that source's own key and no other"
        );
        changed.borrow_mut().clear();

        let first_row = rows[0].clone();
        let first_fader = faders[0].clone();
        sources.set_sources(&[
            source("built-in", "Built-in", 55.0, 100.0, 0.0),
            source("dp-1", "DP-1", 65.0, 100.0, 0.0),
            source("dp-2", "DP-2", 80.0, 100.0, 5.0),
        ]);
        let rows_after: Vec<Row> = children_of(&sources);
        let faders_after: Vec<Fader> = children_of(&sources);
        assert_eq!(
            rows_after[0], first_row,
            "an untouched row is reused rather than rebuilt"
        );
        assert_eq!(faders_after[0], first_fader, "and so is its fader");
        assert_eq!(faders_after[0].value(), 55.0);

        let renders_before = sources.imp().renders.get();
        sources.set_sources(&[
            source("built-in", "Built-in", 55.0, 100.0, 0.0),
            source("dp-1", "DP-1", 65.0, 100.0, 0.0),
            source("dp-2", "DP-2", 80.0, 100.0, 5.0),
        ]);
        assert_eq!(
            sources.imp().renders.get(),
            renders_before,
            "an identical slice returns early and never re-renders"
        );

        sources.set_sources(&[source("hostile", &"ё".repeat(300), 10.0, 100.0, 0.0)]);
        let hostile_rows: Vec<Row> = children_of(&sources);
        assert_eq!(hostile_rows.len(), 1);
        assert!(
            child_named::<gtk4::Label>(&hostile_rows[0], "row__title")
                .text()
                .chars()
                .count()
                <= TEXT_MAX_CHARS,
            "an unbounded, multi-byte source name is capped without slicing a character"
        );

        sources.set_sources(&[]);
        assert!(
            sources.first_child().is_none(),
            "an emptied source list renders no children"
        );

        let displays = DisplayList::new();
        assert!(
            displays.first_child().is_none(),
            "a display list renders nothing before the first snapshot"
        );

        let mode = DisplayMode {
            width: 1920,
            height: 1080,
            refresh_mhz: 60_000,
        };
        let logical = DisplayLogical {
            x: 0,
            y: 0,
            scale: 1.5,
        };
        let built_in = Display {
            connector: "eDP-1".to_owned(),
            label: "Built-in display".to_owned(),
            current_mode: Some(mode.clone()),
            logical: Some(logical.clone()),
            enabled: true,
            ..Display::default()
        };
        let external = Display {
            connector: "DP-1".to_owned(),
            label: "DELL U2720Q".to_owned(),
            make: Some("Dell Inc.".to_owned()),
            model: Some("U2720Q".to_owned()),
            serial: Some("8QK1P93".to_owned()),
            current_mode: Some(mode.clone()),
            logical: Some(logical.clone()),
            enabled: true,
        };

        let detail_of = |holder: &gtk4::Box| -> (FactList, SwitchRow) {
            let body = crate::drawer::panel(holder)
                .and_then(|panel| panel.child())
                .expect("a detail body");
            let facts = body
                .first_child()
                .and_downcast::<FactList>()
                .expect("a fact list");
            let switch = facts
                .next_sibling()
                .and_downcast::<SwitchRow>()
                .expect("a switch row");
            (facts, switch)
        };
        let heads_of = |holders: &[gtk4::Box]| -> Vec<Row> {
            holders.iter().filter_map(crate::drawer::head).collect()
        };

        displays.set_displays(&[built_in.clone(), external.clone()]);
        let heads: Vec<Row> = heads_of(&children_of(&displays));
        let holders: Vec<gtk4::Box> = children_of(&displays);
        assert_eq!(heads.len(), 2);
        assert_eq!(holders.len(), 2);
        for head in &heads {
            let _chevron = child_named::<gtk4::Image>(head, "drawer-chevron");
        }

        let (built_in_facts, built_in_switch) = detail_of(&holders[0]);
        let (external_facts, external_switch) = detail_of(&holders[1]);
        let built_in_fact_rows: Vec<Row> = children_of(&built_in_facts);
        let external_fact_rows: Vec<Row> = children_of(&external_facts);
        assert_eq!(
            built_in_fact_rows.len(),
            4,
            "the connector always leads the detail; a display with no make, model or serial omits \
             those lines rather than rendering Unknown or an empty row"
        );
        assert_eq!(external_fact_rows.len(), 7);
        assert!(
            !built_in_fact_rows
                .iter()
                .any(|row| row.value().as_deref() == Some("8QK1P93"))
        );
        assert!(
            external_fact_rows
                .iter()
                .any(|row| row.value().as_deref() == Some("8QK1P93"))
        );
        assert!(
            built_in_fact_rows
                .iter()
                .any(|row| row.title().as_deref() == Some("Connector")),
            "the head names the display, so the connector is a line of the detail"
        );

        assert!(!built_in_switch.locked());
        assert!(
            !external_switch.locked(),
            "two enabled outputs: the guard bites on neither"
        );

        let solo = built_in.clone();
        let mut off = external.clone();
        off.enabled = false;
        displays.set_displays(&[solo.clone(), off.clone()]);
        let holders: Vec<gtk4::Box> = children_of(&displays);
        let (_, solo_switch) = detail_of(&holders[0]);
        let (_, off_switch) = detail_of(&holders[1]);
        assert!(
            solo_switch.locked(),
            "the only enabled output's switch cannot be turned off"
        );
        assert!(
            solo_switch.is_sensitive(),
            "the row itself stays sensitive so its explanation is not dimmed along with the knob"
        );
        let solo_head: &Row = solo_switch.upcast_ref();
        assert_eq!(
            solo_head.subtitle().as_deref(),
            Some("The last enabled display can't be turned off")
        );
        assert!(
            !off_switch.locked(),
            "a disabled output is never locked, or its user would be stranded"
        );
        assert!(!off_switch.active());

        let enable_requests = Rc::new(RefCell::new(Vec::<(String, bool)>::new()));
        displays.connect_enable_requested({
            let enable_requests = Rc::clone(&enable_requests);
            move |_, connector, enabled| enable_requests.borrow_mut().push((connector, enabled))
        });
        off_switch.emit_by_name::<()>("toggled", &[&true]);
        assert_eq!(
            *enable_requests.borrow(),
            vec![("DP-1".to_owned(), true)],
            "toggling a disabled output's switch requests it be enabled, naming its own connector"
        );

        displays.set_displays(&[built_in.clone(), external.clone()]);
        let holders: Vec<gtk4::Box> = children_of(&displays);
        let heads: Vec<Row> = heads_of(&holders);
        let panel_of = |holder: &gtk4::Box| crate::drawer::panel(holder).expect("a revealer");

        assert!(!panel_of(&holders[0]).reveals_child());
        assert!(!heads[0].has_css_class(crate::drawer::OPEN));

        heads[0].emit_clicked();
        assert!(
            panel_of(&holders[0]).reveals_child() && heads[0].has_css_class(crate::drawer::OPEN),
            "activating a head opens its own detail"
        );
        assert!(
            !panel_of(&holders[1]).reveals_child()
                && heads[1].has_css_class(crate::drawer::RECEDED)
                && !heads[1].has_css_class(crate::drawer::OPEN),
            "every other holder recedes, derived from what is revealed rather than from \
             remembered state"
        );

        heads[0].emit_clicked();
        assert!(
            !panel_of(&holders[0]).reveals_child() && !heads[0].has_css_class(crate::drawer::OPEN),
            "activating the open head again closes it"
        );
        assert!(
            !heads[1].has_css_class(crate::drawer::RECEDED),
            "closing the only open detail lifts the receded state everywhere"
        );

        heads[0].emit_clicked();
        assert!(panel_of(&holders[0]).reveals_child());
        let kept_head = heads[0].clone();
        let renders_before = displays.imp().renders.get();
        displays.set_displays(&[built_in.clone(), external.clone()]);
        assert_eq!(
            displays.imp().renders.get(),
            renders_before,
            "an identical slice returns early and never re-renders"
        );
        let heads_after: Vec<Row> = heads_of(&children_of(&displays));
        assert_eq!(heads_after.len(), 2);
        assert_eq!(heads_after[0], kept_head, "rows are reused, not rebuilt");
        assert!(
            panel_of(&holders[0]).reveals_child(),
            "an unchanged slice must not close a detail the user has open"
        );

        let hostile = Display {
            connector: "DP-2".to_owned(),
            make: Some("ё".repeat(300)),
            model: Some("ё".repeat(300)),
            serial: Some("ё".repeat(300)),
            enabled: true,
            ..Display::default()
        };
        displays.set_displays(&[hostile]);
        let holders: Vec<gtk4::Box> = children_of(&displays);
        let (hostile_facts, _) = detail_of(&holders[0]);
        assert!(
            children_of::<Row>(&hostile_facts).iter().all(|row| {
                row.value()
                    .map(|value| value.chars().count() <= TEXT_MAX_CHARS)
                    .unwrap_or(true)
            }),
            "an unbounded, multi-byte EDID string is capped without slicing a character"
        );

        displays.set_displays(&[]);
        assert!(
            displays.first_child().is_none(),
            "an emptied display list renders no children"
        );

        let gate = DisplayList::new();
        let mut gate_off = external.clone();
        gate_off.enabled = false;
        gate.set_displays(&[built_in.clone(), gate_off.clone()]);
        let gate_holders: Vec<gtk4::Box> = children_of(&gate);
        assert_eq!(gate_holders.len(), 2);

        let facts_of = |holder: &gtk4::Box| -> FactList {
            crate::drawer::panel(holder)
                .and_then(|panel| panel.child())
                .and_then(|body| body.first_child())
                .and_downcast::<FactList>()
                .expect("a fact list")
        };

        let (_, gate_built_in_switch) = detail_of(&gate_holders[0]);
        assert!(
            gate_built_in_switch.locked(),
            "AC-1: output_power true, the default, keeps today's behaviour unchanged"
        );

        let gate_heads: Vec<Row> = heads_of(&gate_holders);
        gate_heads[0].emit_clicked();
        let gate_panel = crate::drawer::panel(&gate_holders[0]).expect("a revealer");
        assert!(
            gate_panel.reveals_child(),
            "the detail opens before the gate changes"
        );

        gate.set_output_power(false);
        let gate_holders_off: Vec<gtk4::Box> = children_of(&gate);
        assert_eq!(
            gate_holders_off, gate_holders,
            "gating the switches off reuses the existing rows rather than rebuilding them"
        );
        assert!(
            gate_panel.reveals_child(),
            "AC-6: closing the gate on a display that is not changing must not shut an open \
             detail under the user's hand"
        );
        for holder in &gate_holders_off {
            assert!(
                facts_of(holder).next_sibling().is_none(),
                "AC-2/AC-5: output_power false omits the enable switch entirely, even on the \
                 sole enabled output, rather than leaving it present and locked"
            );
        }
        assert_eq!(
            children_of::<Row>(&facts_of(&gate_holders_off[0])).len(),
            4,
            "AC-3: the built-in display's own facts survive the gate"
        );
        assert_eq!(
            children_of::<Row>(&facts_of(&gate_holders_off[1])).len(),
            7,
            "AC-3: so do the external display's, connector through position"
        );

        gate.set_output_power(true);
        assert!(
            gate_panel.reveals_child(),
            "AC-6: reopening the gate must not shut the detail either"
        );
        let (_, gate_built_in_switch_again) = detail_of(&gate_holders[0]);
        let (_, gate_off_switch_again) = detail_of(&gate_holders[1]);
        assert_eq!(
            gate_built_in_switch_again, gate_built_in_switch,
            "AC-4: the switch that comes back is the same row, not a rebuilt one"
        );
        assert!(
            gate_built_in_switch_again.locked(),
            "AC-4: the sole enabled output relocks once the gate reopens"
        );
        assert!(
            !gate_off_switch_again.locked(),
            "AC-4: a disabled output's switch is never locked"
        );

        let brightness = BrightnessPopover::new();
        assert!(
            !brightness.imp().primary.get_visible() && !brightness.imp().devices.get_visible(),
            "AC-1: with no sources, neither the primary fader nor the source list renders"
        );

        let level = |key: &str, name: &str, value: f64| Source {
            key: key.to_owned(),
            name: name.to_owned(),
            value,
            maximum: 100.0,
            floor: 0.0,
        };

        brightness.set_sources(&[level("built-in", "Built-in", 42.0)]);
        assert!(
            brightness.imp().primary.get_visible(),
            "AC-1: one source renders the primary fader"
        );
        assert!(
            !brightness.imp().devices.get_visible(),
            "AC-1: one source keeps the source list hidden"
        );
        assert_eq!(brightness.imp().primary.value(), 42.0);
        assert_eq!(
            brightness.imp().readout.value().as_deref(),
            Some("42"),
            "AC-2: the hero readout carries the percentage"
        );
        assert_eq!(brightness.imp().readout.unit().as_deref(), Some("%"));
        assert!(
            brightness
                .imp()
                .primary
                .tooltip_text()
                .is_some_and(|text| text.contains("42")),
            "AC-2: the fader carries its exact value in a tooltip instead of a readout"
        );

        brightness.set_sources(&[Source {
            key: "built-in".to_owned(),
            name: "Built-in".to_owned(),
            value: 200_000.0,
            maximum: 400_000.0,
            floor: 0.0,
        }]);
        assert_eq!(
            brightness.imp().primary.value(),
            200_000.0,
            "the fader keeps the hardware's own native range"
        );
        assert_eq!(
            brightness.imp().readout.value().as_deref(),
            Some("50"),
            "the readout is a percentage of the maximum, not the raw native value"
        );
        assert!(
            brightness
                .imp()
                .primary
                .tooltip_text()
                .is_some_and(|text| text.contains("50") && !text.contains("200000")),
            "the tooltip is a percentage too; a native value would read 200000%"
        );

        brightness.set_sources(&[
            level("built-in", "Built-in", 40.0),
            level("dp-1", "DP-1", 60.0),
            level("dp-2", "DP-2", 80.0),
        ]);
        assert!(
            brightness.imp().primary.get_visible() && brightness.imp().devices.get_visible(),
            "AC-1: three sources render both the primary fader and the source list"
        );
        assert_eq!(
            brightness.imp().primary.value(),
            40.0,
            "the primary rail mirrors the caller's own first, current source"
        );
        let non_primary_faders: Vec<Fader> = children_of(&*brightness.imp().devices);
        assert_eq!(
            non_primary_faders.len(),
            2,
            "the source list renders only sources[1..], or the current source would get two \
             faders that do not track each other"
        );
        assert_eq!(non_primary_faders[0].value(), 60.0);
        assert_eq!(non_primary_faders[1].value(), 80.0);
        assert!(
            non_primary_faders
                .iter()
                .all(|fader| fader.tooltip_text().is_some_and(|text| !text.is_empty())),
            "AC-2: every source-list fader carries its exact value in a tooltip too, not only \
             the primary rail"
        );

        brightness.set_sources(&[]);
        assert!(
            !brightness.imp().primary.get_visible() && !brightness.imp().devices.get_visible(),
            "AC-1: going back to no sources hides both again, without panicking"
        );
        brightness.set_sources(&[level("built-in", "Built-in", 55.0)]);

        assert!(
            !brightness.imp().night_light.get_visible(),
            "AC-4: with no night light snapshot ever seen, the whole section is absent"
        );

        brightness.set_night_light(Some(&NightLight {
            enabled: false,
            temperature: 6500,
        }));
        assert!(brightness.imp().night_light.get_visible());
        assert!(brightness.imp().night_light.is_sensitive());
        assert!(
            !brightness.imp().temperature.get_visible(),
            "AC-3: the temperature rail is hidden, not merely insensitive, while the switch is off"
        );

        brightness.set_night_light(Some(&NightLight {
            enabled: true,
            temperature: 4200,
        }));
        assert!(brightness.imp().temperature.get_visible());
        assert_eq!(brightness.imp().temperature.value(), 4200.0);
        assert!(
            brightness
                .imp()
                .temperature
                .tooltip_text()
                .is_some_and(|text| text.contains("4200")),
            "the temperature rail carries its exact value in a tooltip too"
        );
        assert!(
            brightness.imp().temperature.has_css_class("fader--warm"),
            "AC-6: the warm tint is a class scoped to this one fader"
        );
        assert!(
            !brightness.imp().temperature.has_css_class("accent"),
            "AC-6: the warm tint is not the accent colour the audio popover's own fader uses"
        );

        brightness.set_night_light(None);
        assert!(
            brightness.imp().night_light.get_visible()
                && !brightness.imp().night_light.is_sensitive(),
            "AC-5: losing the provider keeps the last-good section visible but greys it"
        );
        assert_eq!(
            brightness.imp().temperature.value(),
            4200.0,
            "AC-5: the last-good values stay on screen rather than resetting"
        );
        assert!(
            brightness.imp().temperature.get_visible(),
            "the last-known state was 'on', so the rail stays up while greyed"
        );

        brightness.set_night_light(Some(&NightLight {
            enabled: true,
            temperature: 3000,
        }));
        assert!(
            brightness.imp().night_light.is_sensitive(),
            "a fresh snapshot lifts the greyed state"
        );

        let brightness_changed = Rc::new(RefCell::new(Vec::<(String, f64)>::new()));
        brightness.connect_changed({
            let brightness_changed = Rc::clone(&brightness_changed);
            move |_, key, value| {
                brightness_changed
                    .borrow_mut()
                    .push((key.to_owned(), value))
            }
        });
        brightness
            .imp()
            .primary
            .emit_by_name::<()>("changed", &[&33.0f64]);
        assert_eq!(
            *brightness_changed.borrow(),
            [("built-in".to_owned(), 33.0)],
            "the primary fader reports the key of the source it mirrors"
        );
        brightness_changed.borrow_mut().clear();
        brightness
            .imp()
            .devices
            .emit_by_name::<()>("changed", &[&"dp-1".to_owned(), &70.0f64]);
        assert_eq!(
            *brightness_changed.borrow(),
            [("dp-1".to_owned(), 70.0)],
            "a source list change is forwarded with its own key"
        );

        let night_toggled = Rc::new(RefCell::new(Vec::<bool>::new()));
        brightness.connect_night_light_toggled({
            let night_toggled = Rc::clone(&night_toggled);
            move |_, on| night_toggled.borrow_mut().push(on)
        });
        brightness
            .imp()
            .enabled
            .emit_by_name::<()>("toggled", &[&false]);
        assert_eq!(*night_toggled.borrow(), [false]);
        assert!(
            !brightness.imp().temperature.get_visible(),
            "UI state never waits on a round trip: the rail follows the knob optimistically, \
             before any set_night_light call reconciles it"
        );

        brightness
            .imp()
            .enabled
            .emit_by_name::<()>("toggled", &[&true]);
        assert_eq!(*night_toggled.borrow(), [false, true]);
        assert!(
            brightness.imp().temperature.get_visible(),
            "and reappears the same optimistic way when the knob flips back"
        );

        let night_changed = Rc::new(RefCell::new(Vec::<f64>::new()));
        brightness.connect_night_light_changed({
            let night_changed = Rc::clone(&night_changed);
            move |_, value| night_changed.borrow_mut().push(value)
        });
        brightness
            .imp()
            .temperature
            .emit_by_name::<()>("changed", &[&3400.0f64]);
        assert_eq!(*night_changed.borrow(), [3400.0]);

        brightness.set_footer(Some("Display settings"));
        assert!(brightness.imp().footer.get_visible());
        let brightness_footer = Rc::new(Cell::new(0u32));
        brightness.connect_footer_activated({
            let brightness_footer = Rc::clone(&brightness_footer);
            move |_| brightness_footer.set(brightness_footer.get() + 1)
        });
        brightness.imp().footer.emit_clicked();
        assert_eq!(brightness_footer.get(), 1);

        let display_popover = DisplayPopover::new();
        assert!(
            !display_popover.imp().section.get_visible(),
            "AC-12: no outputs renders empty, without panicking"
        );

        display_popover.set_output_power(true);
        display_popover.set_displays(&[built_in.clone(), external.clone()]);
        assert!(
            display_popover.imp().section.get_visible(),
            "AC-9: two outputs render the section"
        );
        let popover_holders: Vec<gtk4::Box> = children_of(&*display_popover.imp().devices);
        assert_eq!(
            heads_of(&popover_holders).len(),
            2,
            "AC-9: both outputs list"
        );
        assert!(
            display_popover.imp().blank.get_visible(),
            "AC-9: the blank row is present when output power is supported"
        );
        assert_eq!(
            display_popover.imp().blank.subtitle().as_deref(),
            Some("Any input wakes them again"),
            "AC-11: the blank row's subtitle says input wakes the screens"
        );
        assert_eq!(
            display_popover.imp().blank.type_(),
            Row::static_type(),
            "AC-11: the blank row is a Row, never a SwitchRow — DPMS has no state to sit in, so \
             a knob that moved without the screen following it would be lying"
        );

        let devices_renders_before = display_popover.imp().devices.imp().renders.get();
        display_popover.set_output_power(false);
        assert!(
            !display_popover.imp().blank.get_visible(),
            "AC-10: the blank row is absent entirely, not merely insensitive, when output power \
             is unsupported"
        );
        assert_eq!(
            children_of::<gtk4::Box>(&*display_popover.imp().devices).len(),
            2,
            "gating output power keeps both outputs listed"
        );
        assert!(
            display_popover.imp().devices.imp().renders.get() > devices_renders_before,
            "AC-10: DisplayPopover forwards output_power to DisplayList, which gates every \
             per-output switch on its own render (proven directly on DisplayList, above)"
        );

        display_popover.set_displays(&[]);
        assert!(
            !display_popover.imp().section.get_visible(),
            "AC-12: emptying the outputs renders empty again"
        );

        display_popover.set_output_power(true);
        display_popover.set_displays(std::slice::from_ref(&built_in));
        assert!(display_popover.imp().blank.get_visible());

        let popover_enable_requests = Rc::new(RefCell::new(Vec::<(String, bool)>::new()));
        display_popover.connect_enable_requested({
            let popover_enable_requests = Rc::clone(&popover_enable_requests);
            move |_, connector, enabled| {
                popover_enable_requests
                    .borrow_mut()
                    .push((connector.to_owned(), enabled))
            }
        });
        display_popover
            .imp()
            .devices
            .emit_by_name::<()>("enable-requested", &[&"eDP-1".to_owned(), &false]);
        assert_eq!(
            *popover_enable_requests.borrow(),
            [("eDP-1".to_owned(), false)],
            "a DisplayList request is forwarded as the popover's own signal"
        );

        let blanked = Rc::new(Cell::new(0u32));
        display_popover.connect_blanked({
            let blanked = Rc::clone(&blanked);
            move |_| blanked.set(blanked.get() + 1)
        });
        display_popover.imp().blank.emit_clicked();
        assert_eq!(blanked.get(), 1);

        display_popover.set_footer(Some("Display settings"));
        assert!(display_popover.imp().footer.get_visible());

        let popover_heads: Vec<Row> = heads_of(&children_of(&*display_popover.imp().devices));
        popover_heads[0].emit_clicked();
        assert!(
            display_popover
                .imp()
                .hero
                .has_css_class(crate::drawer::RECEDED)
                && display_popover
                    .imp()
                    .blank
                    .has_css_class(crate::drawer::RECEDED)
                && display_popover
                    .imp()
                    .footer
                    .has_css_class(crate::drawer::RECEDED),
            "opening display details dims the popover chrome while preserving the open row"
        );
        popover_heads[0].emit_clicked();
        assert!(
            !display_popover
                .imp()
                .hero
                .has_css_class(crate::drawer::RECEDED)
                && !display_popover
                    .imp()
                    .blank
                    .has_css_class(crate::drawer::RECEDED)
                && !display_popover
                    .imp()
                    .footer
                    .has_css_class(crate::drawer::RECEDED),
            "closing details restores the popover chrome"
        );

        let popover_footer = Rc::new(Cell::new(0u32));
        display_popover.connect_footer_activated({
            let popover_footer = Rc::clone(&popover_footer);
            move |_| popover_footer.set(popover_footer.get() + 1)
        });
        display_popover.imp().footer.emit_clicked();
        assert_eq!(popover_footer.get(), 1);
        let idle = IdlePopover::new();
        let idle_hold = idle.imp().hold.clone();
        let idle_hold_row = idle.imp().hold_row.clone();
        let _hold_chevron = child_named::<gtk4::Image>(&idle_hold_row, "drawer-chevron");
        let idle_hold_panel = idle.imp().hold_panel.clone();
        let idle_list_rule = idle.imp().list_rule.clone();
        assert!(
            !idle_list_rule.get_visible(),
            "no entries yet, so the hairline above the list shows nothing"
        );

        idle.set_heading("preferences-desktop-screensaver-symbolic", "Idle", None);
        idle.set_hold_active(true);
        assert!(idle_hold.is_active());

        let idle_toggled = Rc::new(RefCell::new(Vec::<bool>::new()));
        idle.connect_hold_toggled({
            let idle_toggled = Rc::clone(&idle_toggled);
            move |_, on| idle_toggled.borrow_mut().push(on)
        });
        idle.set_hold_active(false);
        assert!(
            idle_toggled.borrow().is_empty(),
            "dressing the switch programmatically must not report it as a user action"
        );
        idle_hold.set_active(true);
        assert_eq!(*idle_toggled.borrow(), [true]);

        assert!(
            !idle_hold_panel.reveals_child(),
            "the duration choices start closed"
        );
        assert!(
            idle_hold_panel
                .child()
                .is_some_and(|card| card.has_css_class("detail-card")),
            "duration choices use the same detail card as other drawers"
        );
        idle.set_inhibitors(&[inhibitor(1, "Zoom", true)]);
        idle.set_footer(Some("Idle settings"));
        idle_hold_row.emit_clicked();
        assert!(
            idle_hold_panel.reveals_child()
                && idle.imp().hold_row.has_css_class(crate::drawer::OPEN)
                && idle.imp().hero.has_css_class(crate::drawer::RECEDED)
                && idle.imp().list.has_css_class(crate::drawer::RECEDED)
                && idle.imp().list_rule.has_css_class(crate::drawer::RECEDED)
                && idle.imp().footer.has_css_class(crate::drawer::RECEDED)
                && idle
                    .imp()
                    .shell
                    .imp()
                    .hero_rule
                    .has_css_class(crate::drawer::RECEDED)
                && idle
                    .imp()
                    .shell
                    .imp()
                    .footer_rule
                    .has_css_class(crate::drawer::RECEDED),
            "the duration row opens its choices and dims the resting popover"
        );
        idle_hold_row.emit_clicked();
        assert!(
            !idle_hold_panel.reveals_child()
                && !idle.imp().hold_row.has_css_class(crate::drawer::OPEN)
                && !idle.imp().hero.has_css_class(crate::drawer::RECEDED)
                && !idle.imp().list.has_css_class(crate::drawer::RECEDED)
                && !idle.imp().list_rule.has_css_class(crate::drawer::RECEDED)
                && !idle.imp().footer.has_css_class(crate::drawer::RECEDED)
                && !idle
                    .imp()
                    .shell
                    .imp()
                    .hero_rule
                    .has_css_class(crate::drawer::RECEDED)
                && !idle
                    .imp()
                    .shell
                    .imp()
                    .footer_rule
                    .has_css_class(crate::drawer::RECEDED),
            "the same duration row closes its choices and restores the popover"
        );

        let idle_requested = Rc::new(RefCell::new(Vec::<u32>::new()));
        idle.connect_hold_requested({
            let idle_requested = Rc::clone(&idle_requested);
            move |_, seconds| idle_requested.borrow_mut().push(seconds)
        });
        for (button, seconds) in [
            (&idle.imp().preset_15m, 900u32),
            (&idle.imp().preset_30m, 1800),
            (&idle.imp().preset_1h, 3600),
            (&idle.imp().preset_2h, 7200),
            (&idle.imp().preset_4h, 14400),
            (&idle.imp().preset_indefinite, 0),
        ] {
            button.emit_clicked();
            assert_eq!(
                *idle_requested.borrow().last().unwrap(),
                seconds,
                "each preset button must ask for its own duration, not a neighbour's"
            );
        }
        assert_eq!(*idle_requested.borrow(), [900, 1800, 3600, 7200, 14400, 0]);

        idle.set_inhibitors(&[inhibitor(1, "Zoom", true)]);
        assert!(
            idle_list_rule.get_visible(),
            "an entry pulls the hairline above the list back in"
        );
        let idle_released = Rc::new(RefCell::new(Vec::<u64>::new()));
        idle.connect_release_requested({
            let idle_released = Rc::clone(&idle_released);
            move |_, id| idle_released.borrow_mut().push(id)
        });
        let holder = children_of::<gtk4::Box>(&*idle.imp().list).remove(0);
        let tile = holder.first_child().unwrap().downcast::<Row>().unwrap();
        let panel = holder
            .last_child()
            .unwrap()
            .downcast::<gtk4::Revealer>()
            .unwrap();
        tile.emit_clicked();
        assert!(panel.reveals_child());
        assert!(idle.imp().hero.has_css_class(crate::drawer::RECEDED));
        let card = panel.child().unwrap().downcast::<gtk4::Box>().unwrap();
        children_of::<Row>(&card).remove(0).emit_clicked();
        assert_eq!(*idle_released.borrow(), [1]);

        idle.set_inhibitors(&[]);
        assert!(
            !idle_list_rule.get_visible() && !idle.imp().hero.has_css_class(crate::drawer::RECEDED),
            "the hairline and dimming clear when the list empties"
        );

        let idle_footer_activated = Rc::new(Cell::new(0u32));
        idle.connect_footer_activated({
            let idle_footer_activated = Rc::clone(&idle_footer_activated);
            move |_| idle_footer_activated.set(idle_footer_activated.get() + 1)
        });
        idle.set_footer(Some("Idle settings"));
        assert!(
            !idle.imp().shell.imp().footer_rule.get_visible(),
            "an empty inhibitor list leaves no divider between Keep awake and settings"
        );
        idle.set_inhibitors(&[inhibitor(2, "Firefox", false)]);
        assert!(idle.imp().shell.imp().footer_rule.get_visible());
        idle.set_inhibitors(&[]);
        assert!(!idle.imp().shell.imp().footer_rule.get_visible());
        idle.imp().footer.emit_clicked();
        assert_eq!(idle_footer_activated.get(), 1);

        let session = SessionPopover::new();
        assert!(
            session
                .layout_manager()
                .is_some_and(|layout| layout.is::<gtk4::BinLayout>()),
            "the session popover root has a bin layout"
        );
        let session_actions = Rc::new(RefCell::new(Vec::new()));
        session.connect_action_requested({
            let session_actions = Rc::clone(&session_actions);
            move |_, action| session_actions.borrow_mut().push(action.to_owned())
        });
        let hibernate = SessionActionState {
            visible: true,
            enabled: false,
            subtitle: Some("Blocked by an active inhibitor.".to_owned()),
        };
        session.set_action(HIBERNATE, &hibernate);
        assert!(!session.imp().hibernate.is_sensitive());
        let notifies = Rc::new(Cell::new(0u32));
        session.imp().hibernate.connect_notify_local(None, {
            let notifies = Rc::clone(&notifies);
            move |_, _| notifies.set(notifies.get() + 1)
        });
        session.set_action(HIBERNATE, &hibernate);
        assert_eq!(
            notifies.get(),
            0,
            "an unchanged action must not write the row"
        );
        session.imp().lock.emit_clicked();
        assert_eq!(*session_actions.borrow(), [LOCK]);
        session.set_updates(Some("Updates available"));
        assert!(session.imp().updates_section.get_visible());
        assert_eq!(
            session.imp().updates.value().as_deref(),
            Some("Updates available")
        );
        session.set_updates(None);
        assert!(!session.imp().updates_section.get_visible());
        session.set_sessions(&[SessionChoice {
            id: "other".to_owned(),
            user: "Other user".to_owned(),
            subtitle: Some("wayland".to_owned()),
        }]);
        assert!(session.imp().sessions_section.get_visible());
        let activated = Rc::new(RefCell::new(Vec::new()));
        session.connect_activate_session({
            let activated = Rc::clone(&activated);
            move |_, id| activated.borrow_mut().push(id.to_owned())
        });
        session
            .imp()
            .sessions
            .first_child()
            .unwrap()
            .downcast::<Row>()
            .unwrap()
            .emit_clicked();
        assert_eq!(*activated.borrow(), ["other"]);
        session.set_sessions(&[]);
        assert!(session.imp().sessions.first_child().is_none());
        assert!(!session.imp().sessions_section.get_visible());

        let battery = BatteryPopover::new();
        assert!(
            battery
                .layout_manager()
                .is_some_and(|layout| layout.is::<gtk4::BinLayout>()),
            "the battery popover root has a bin layout"
        );
        battery.set_heading(
            Some("battery-full-charged-symbolic"),
            Some("Fully charged"),
            Some(100),
        );
        assert_eq!(battery.imp().readout.value().as_deref(), Some("100"));
        assert!(battery.imp().readout.get_visible());
        battery.set_profiles(&[], None);
        assert!(!battery.imp().profiles_section.get_visible());
        battery.set_profiles(
            &[Choice {
                label: "Balanced".to_owned(),
                detail: String::new(),
                icon_name: "power-profile-balanced-symbolic".to_owned(),
            }],
            Some(0),
        );
        assert!(battery.imp().profiles_section.get_visible());
        battery.set_devices(&[]);
        assert!(!battery.imp().devices_section.get_visible());
        battery.set_devices(&[BatteryDevice {
            name: "MX Master 3S".to_owned(),
            subtitle: "Mouse".to_owned(),
            icon_name: "input-mouse-symbolic".to_owned(),
            value: "41%".to_owned(),
        }]);
        assert!(battery.imp().devices_section.get_visible());
        battery.set_details(&[], None);
        assert!(!battery.imp().details_holder.get_visible());
        assert!(!battery.imp().details_panel.reveals_child());
        battery.set_details(
            &[Fact::new("Charge", "100%")],
            Some(&BatteryChargeLimit {
                enabled: false,
                subtitle: "Stops at 80% to slow wear".to_owned(),
            }),
        );
        assert!(battery.imp().details_holder.get_visible());
        assert!(battery.imp().charge_limit.get_visible());
        assert!(!battery.imp().charge_limit.active());
        battery.imp().details_row.emit_clicked();
        assert!(battery.imp().details_panel.reveals_child());
        battery.close_details();
        assert!(!battery.imp().details_panel.reveals_child());
        let battery_profiles = Rc::new(Cell::new(None));
        battery.connect_profile_activated({
            let battery_profiles = Rc::clone(&battery_profiles);
            move |_, index| battery_profiles.set(Some(index))
        });
        battery
            .imp()
            .profiles
            .emit_by_name::<()>("activated", &[&0u32]);
        assert_eq!(battery_profiles.get(), Some(0));
        let battery_limit = Rc::new(Cell::new(None));
        battery.connect_charge_limit_toggled({
            let battery_limit = Rc::clone(&battery_limit);
            move |_, on| battery_limit.set(Some(on))
        });
        battery
            .imp()
            .charge_limit
            .emit_by_name::<()>("toggled", &[&true]);
        assert_eq!(battery_limit.get(), Some(true));
    }

    /// Separate from `widgets()` so an unrelated failure earlier in that test cannot stop these
    /// from running — which is exactly what happened while they were written.
    #[test]
    #[ignore = "needs a display"]
    fn clipboard_widgets() {
        if gtk4::init().is_err() {
            return;
        }
        register_resources().expect("resources");
        let _styles = Styles::install(adw::ColorScheme::Default);

        let clipboard = ClipboardPopover::new();
        clipboard.set_actions(ClipActions {
            pin: "Pin".to_owned(),
            unpin: "Unpin".to_owned(),
            forget: "Forget".to_owned(),
        });
        let clip = |id: u64, pinned: bool| Clip {
            id,
            title: "Марта🙂 a rather long clipboard entry that keeps going".to_owned(),
            icon: "text-x-generic-symbolic".to_owned(),
            image: None,
            pinned,
        };
        clipboard.set_pinned(&[clip(1, true)]);
        clipboard.set_recent(&[clip(2, false), clip(3, false)]);

        let rows = |list: &ClipboardList| {
            let mut found = Vec::new();
            let mut child = list.first_child();
            while let Some(holder) = child {
                child = holder.next_sibling();
                if let Some(split) = holder
                    .downcast_ref::<gtk4::Box>()
                    .and_then(drawer::head::<SplitRow>)
                {
                    found.push(split);
                }
            }
            found
        };
        assert_eq!(rows(&clipboard.imp().recent).len(), 2);
        assert!(
            clipboard.imp().pinned_section.get_visible(),
            "a pinned entry gives the section something to show"
        );

        let width =
            |popover: &ClipboardPopover| popover.measure(gtk4::Orientation::Horizontal, -1).1;
        let floor = width(&clipboard);

        clipboard.set_open(Some(2));
        assert!(
            clipboard.imp().hero.has_css_class("receded"),
            "the hero is read against the open panel and must recede with everything else"
        );
        assert!(
            clipboard.imp().clear.has_css_class("receded")
                && clipboard.imp().footer.has_css_class("receded"),
            "a footer row that stays lit still looks pressable while a card asks a question"
        );
        let opened = &rows(&clipboard.imp().recent)[0];
        assert!(
            opened.has_css_class("open") && !opened.has_css_class("receded"),
            "the row that was opened is the one thing that must not recede"
        );
        assert!(
            rows(&clipboard.imp().recent)[1].has_css_class("receded"),
            "its neighbour recedes"
        );
        assert_eq!(
            width(&clipboard),
            floor,
            "opening a detail must not resize the card under the pointer that opened it"
        );

        clipboard.set_open(None);
        assert!(!clipboard.imp().hero.has_css_class("receded"));

        // A press lands on the panel's own rows, so rebuilding them under a gesture swallows it.
        clipboard.set_open(Some(2));
        let panel_of = |list: &ClipboardList, index: usize| {
            let holder = list.observe_children().item(index as u32)?;
            drawer::panel(holder.downcast_ref::<gtk4::Box>()?)?.child()
        };
        let before = panel_of(&clipboard.imp().recent, 0).expect("an open panel");
        clipboard.set_recent(&[clip(2, false), clip(3, false)]);
        assert_eq!(
            panel_of(&clipboard.imp().recent, 0).as_ref(),
            Some(&before),
            "an unrelated republish must leave the open panel's buttons alone"
        );

        let cleared = Rc::new(Cell::new(false));
        clipboard.connect_cleared({
            let cleared = Rc::clone(&cleared);
            move |_| cleared.set(true)
        });
        clipboard.imp().clear.emit_by_name::<()>("clicked", &[]);
        assert!(cleared.get());

        let restored = Rc::new(Cell::new(None));
        clipboard.connect_restored({
            let restored = Rc::clone(&restored);
            move |_, id| restored.set(Some(id))
        });
        rows(&clipboard.imp().recent)[1].emit_by_name::<()>("activated", &[]);
        assert_eq!(
            restored.get(),
            Some(3),
            "a row emits the entry it currently holds, not the one it was built for"
        );

        // An empty Recent beside a populated Pinned would otherwise read "Nothing copied yet".
        clipboard.set_recent(&[]);
        assert!(
            !clipboard.imp().recent_section.get_visible(),
            "an empty Recent hides rather than contradicting the Pinned list above it"
        );
    }

    /// Separate from `widgets()` for the same reason as `clipboard_widgets`: one failure earlier in
    /// that function must not hide every assertion written after it.
    #[test]
    #[ignore = "needs a display"]
    fn places_popover_widgets() {
        if gtk4::init().is_err() {
            return;
        }
        register_resources().expect("resources");
        let _styles = Styles::install(adw::ColorScheme::Default);

        let popover = PlacesPopover::new();
        let imp = popover.imp();

        assert!(
            !imp.places.get_visible()
                && !imp.bookmarks.get_visible()
                && !imp.network.get_visible()
                && !imp.trash.get_visible(),
            "an untouched popover shows no section"
        );

        let entry = |id: &str, title: &str| PlacesEntry {
            id: id.to_owned(),
            title: title.to_owned(),
            ..Default::default()
        };

        popover.set_places(&[]);
        popover.set_bookmarks(&[]);
        popover.set_network(&[]);
        popover.set_trash(None);
        assert!(
            !imp.places.get_visible()
                && !imp.bookmarks.get_visible()
                && !imp.network.get_visible()
                && !imp.trash.get_visible(),
            "an empty section and one given nothing at all are the same: no section renders"
        );

        // Places and Bookmarks: hidden when empty, reused by key rather than position.
        popover.set_places(&[entry("home", "Home"), entry("docs", "Documents")]);
        assert!(imp.places.get_visible());
        let home_widget = imp.places_rows.first_child();
        popover.set_places(&[entry("docs", "Documents"), entry("home", "Home")]);
        assert_eq!(
            imp.places_rows.last_child(),
            home_widget,
            "a position reuses its widget rather than rebuilding it"
        );

        let activated = Rc::new(RefCell::new(None));
        popover.connect_activated({
            let activated = Rc::clone(&activated);
            move |_, id| {
                activated.replace(Some(id.to_owned()));
            }
        });
        home_widget
            .and_downcast::<Row>()
            .expect("a row")
            .emit_by_name::<()>("clicked", &[]);
        assert_eq!(
            activated.borrow().as_deref(),
            Some("home"),
            "identity follows the key a row was built for, not the position it sits at now"
        );

        popover.set_bookmarks(&[entry("dl", "Downloads")]);
        assert!(imp.bookmarks.get_visible());
        popover.set_bookmarks(&[]);
        assert!(!imp.bookmarks.get_visible());

        popover.set_network(&[entry("files", "files")]);
        assert!(imp.network.get_visible());
        popover.set_network(&[]);
        assert!(
            !imp.network.get_visible(),
            "an empty Network hides like every other section rather than standing empty"
        );

        // Trash: a known count of zero is "Empty", not absence either.
        popover.set_trash(Some(PlacesTrash { items: 0 }));
        assert!(imp.trash.get_visible());
        assert_eq!(imp.trash_row.value().as_deref(), Some("Empty"));
        assert_eq!(
            imp.trash_row.lead_icon().as_deref(),
            Some("user-trash-symbolic")
        );
        popover.set_trash(Some(PlacesTrash { items: 363 }));
        assert_eq!(imp.trash_row.value().as_deref(), Some("363 items"));
        assert_eq!(
            imp.trash_row.lead_icon().as_deref(),
            Some("user-trash-full-symbolic")
        );
        popover.set_trash(None);
        assert!(!imp.trash.get_visible());

        // A hostile multibyte label is capped by characters, not bytes, and the card holds its
        // width. Both measurements keep the same sections visible, so the comparison isolates
        // label length rather than a section appearing for the first time.
        let width = |popover: &PlacesPopover| popover.measure(gtk4::Orientation::Horizontal, -1).1;
        popover.set_places(&[entry("a", "Home")]);
        popover.set_bookmarks(&[entry("b", "Downloads")]);
        popover.set_network(&[]);
        popover.set_trash(None);
        let floor = width(&popover);

        let hostile = "\u{1f642}".repeat(400);
        popover.set_bookmarks(&[entry("h", &hostile)]);
        assert_eq!(
            width(&popover),
            floor,
            "a 400-character bookmark label must not widen the card"
        );

        let more = Rc::new(Cell::new(false));
        popover.connect_more({
            let more = Rc::clone(&more);
            move |_| more.set(true)
        });
        popover.set_overflow(Some("3 more"));
        imp.bookmarks_more.emit_by_name::<()>("clicked", &[]);
        assert!(more.get());

        let footer_activated = Rc::new(Cell::new(false));
        popover.connect_footer_activated({
            let footer_activated = Rc::clone(&footer_activated);
            move |_| footer_activated.set(true)
        });
        popover.set_footer(Some("Open file manager"));
        assert!(imp.footer.get_visible());
        imp.footer.emit_by_name::<()>("clicked", &[]);
        assert!(footer_activated.get());
    }

    /// Separate from `widgets()` for the same reason as `places_popover_widgets`: one failure
    /// earlier in that function must not hide every assertion written after it.
    #[test]
    #[ignore = "needs a display"]
    fn removable_popover_widgets() {
        if gtk4::init().is_err() {
            return;
        }
        register_resources().expect("resources");
        let _styles = Styles::install(adw::ColorScheme::Default);

        let popover = RemovablePopover::new();
        let imp = popover.imp();

        assert!(
            !imp.devices.get_visible(),
            "an untouched popover lists no device"
        );

        // AC-1, a drive with two volumes carries a real eject control on the drive row alone; AC-2,
        // its mounted volume carries a real unmount control; AC-3, that mounted volume's body stays
        // clickable.
        popover.set_devices(&[RemovableDrive {
            id: "cruzer".to_owned(),
            title: "SanDisk Cruzer".to_owned(),
            subtitle: "USB drive \u{b7} 2 volumes".to_owned(),
            ejectable: true,
            activatable: true,
            volumes: vec![
                RemovableVolume {
                    id: "photos".to_owned(),
                    title: "Photos".to_owned(),
                    subtitle: "24 GB free of 64 GB \u{b7} exfat".to_owned(),
                    activatable: true,
                    fraction: Some(0.625),
                    mounted: true,
                    ..Default::default()
                },
                RemovableVolume {
                    id: "backup".to_owned(),
                    title: "Backup".to_owned(),
                    subtitle: "Not mounted".to_owned(),
                    activatable: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }]);
        assert!(imp.devices.get_visible());
        let cells = children_of::<gtk4::Box>(&*imp.devices_rows);
        assert_eq!(cells.len(), 3, "one drive header plus its two volumes");

        let split_of = |cell: &gtk4::Box| cell.first_child().and_downcast::<SplitRow>();
        let row_of = |cell: &gtk4::Box| -> Row {
            match split_of(cell) {
                Some(split) => split.row(),
                None => cell.first_child().and_downcast::<Row>().expect("a row"),
            }
        };

        let header_split = split_of(&cells[0]).expect("AC-1: the drive row is a real $SplitRow");
        assert_eq!(
            header_split.row().title().as_deref(),
            Some("SanDisk Cruzer")
        );
        assert_eq!(cells[0].margin_start(), 0);
        assert_eq!(
            header_split.detail_icon(),
            "media-eject-symbolic",
            "the drive row's trailing control ejects"
        );

        let ejected = Rc::new(RefCell::new(None));
        popover.connect_eject({
            let ejected = Rc::clone(&ejected);
            move |_, id| {
                ejected.replace(Some(id.to_owned()));
            }
        });
        header_split.detail().emit_by_name::<()>("clicked", &[]);
        assert_eq!(
            ejected.borrow().as_deref(),
            Some("cruzer"),
            "AC-1: the trailing button emits a typed signal carrying the drive id"
        );

        assert!(
            split_of(&cells[1]).is_some(),
            "AC-2: a mounted volume carries a real trailing control"
        );
        assert_eq!(
            cells[1].margin_start(),
            24,
            "a volume nests under its drive"
        );

        let unmounted = Rc::new(RefCell::new(None));
        popover.connect_unmount({
            let unmounted = Rc::clone(&unmounted);
            move |_, id| {
                unmounted.replace(Some(id.to_owned()));
            }
        });
        split_of(&cells[1])
            .expect("a split row")
            .detail()
            .emit_by_name::<()>("clicked", &[]);
        assert_eq!(
            unmounted.borrow().as_deref(),
            Some("photos"),
            "AC-2: the trailing button emits a typed signal carrying the volume id"
        );

        // AC-6: an unmounted volume offers no trailing control at all.
        assert!(
            split_of(&cells[2]).is_none(),
            "AC-6: an unmounted volume is a plain row with no trailing control"
        );

        // AC-4: a capacity bar renders under the row, full width, never in the trail slot, and no
        // percentage text is set anywhere.
        let photos_bar = cells[1]
            .last_child()
            .and_downcast::<gtk4::ProgressBar>()
            .expect("a progress bar under the mounted volume");
        assert!((photos_bar.fraction() - 0.625).abs() < f64::EPSILON);
        assert!(
            row_of(&cells[1]).activatable(),
            "AC-3: a mounted volume's body stays a click target even under a capacity bar"
        );
        assert!(
            cells[2]
                .last_child()
                .and_downcast::<gtk4::ProgressBar>()
                .is_none(),
            "an unmounted volume with no known usage carries no bar"
        );

        // AC-6 continued: an unchanged list reuses the same widget instances rather than tearing
        // down and rebuilding the trailing control on every dress.
        let header_widget = cells[0].first_child();
        let photos_widget = cells[1].first_child();
        popover.set_devices(&[RemovableDrive {
            id: "cruzer".to_owned(),
            title: "SanDisk Cruzer".to_owned(),
            subtitle: "USB drive \u{b7} 2 volumes".to_owned(),
            ejectable: true,
            activatable: true,
            volumes: vec![
                RemovableVolume {
                    id: "photos".to_owned(),
                    title: "Photos".to_owned(),
                    subtitle: "24 GB free of 64 GB \u{b7} exfat".to_owned(),
                    activatable: true,
                    fraction: Some(0.625),
                    mounted: true,
                    ..Default::default()
                },
                RemovableVolume {
                    id: "backup".to_owned(),
                    title: "Backup".to_owned(),
                    subtitle: "Not mounted".to_owned(),
                    activatable: true,
                    busy: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }]);
        let cells_after = children_of::<gtk4::Box>(&*imp.devices_rows);
        assert_eq!(
            cells_after[0].first_child(),
            header_widget,
            "an unrelated update reuses the same eject control rather than rebuilding it"
        );
        assert_eq!(
            cells_after[1].first_child(),
            photos_widget,
            "an unrelated update reuses the same unmount control rather than rebuilding it"
        );

        // AC-5: work in flight is a spinner, never a word, and it suppresses the trailing control.
        popover.set_devices(&[RemovableDrive {
            id: "busy".to_owned(),
            title: "External SSD".to_owned(),
            busy: true,
            activatable: true,
            volumes: vec![RemovableVolume {
                id: "vol".to_owned(),
                title: "External SSD".to_owned(),
                subtitle: "210 GB free of 500 GB \u{b7} ext4".to_owned(),
                activatable: true,
                busy: true,
                mounted: true,
                ..Default::default()
            }],
            ..Default::default()
        }]);
        let busy_cells = children_of::<gtk4::Box>(&*imp.devices_rows);
        let busy_row = row_of(&busy_cells[0]);
        assert!(busy_row.busy());
        assert_eq!(
            busy_row.subtitle().as_deref(),
            Some("210 GB free of 500 GB \u{b7} ext4")
        );
        assert!(
            split_of(&busy_cells[0]).is_none(),
            "busy suppresses the trailing control rather than layering a word over it"
        );

        // AC-7: a hostile multibyte label is capped by characters, not bytes, and the card holds
        // its width.
        let width =
            |popover: &RemovablePopover| popover.measure(gtk4::Orientation::Horizontal, -1).1;
        let drive = |id: &str, label: &str| RemovableDrive {
            id: id.to_owned(),
            title: label.to_owned(),
            activatable: true,
            volumes: vec![RemovableVolume {
                id: "v".to_owned(),
                title: label.to_owned(),
                subtitle: label.to_owned(),
                activatable: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        popover.set_devices(&[drive("d", "Backup")]);
        let floor = width(&popover);

        let hostile = "\u{1f642}".repeat(400);
        popover.set_devices(&[drive("d", &hostile)]);
        assert_eq!(
            width(&popover),
            floor,
            "a 400-character volume label must not widen the card"
        );

        let more = Rc::new(Cell::new(false));
        popover.connect_more({
            let more = Rc::clone(&more);
            move |_| more.set(true)
        });
        popover.set_overflow(Some("3 more"));
        imp.devices_more.emit_by_name::<()>("clicked", &[]);
        assert!(more.get());

        let footer_activated = Rc::new(Cell::new(false));
        popover.connect_footer_activated({
            let footer_activated = Rc::clone(&footer_activated);
            move |_| footer_activated.set(true)
        });
        popover.set_footer(Some("Open file manager"));
        assert!(imp.footer.get_visible());
        imp.footer.emit_by_name::<()>("clicked", &[]);
        assert!(footer_activated.get());
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

    #[test]
    #[ignore = "needs a display"]
    fn workspace_name_popover_widgets() {
        if gtk4::init().is_err() {
            return;
        }
        register_resources().expect("resources");

        let popover = WorkspaceNamePopover::new();
        let imp = popover.imp();

        popover.set_workspace("Workspace 3", "DP-2 · 4 windows");
        assert_eq!(imp.hero.title().as_deref(), Some("Workspace 3"));
        assert_eq!(imp.hero.subtitle().as_deref(), Some("DP-2 · 4 windows"));

        popover.set_name("dev");
        assert_eq!(imp.name.text(), "dev", "an untouched entry takes the name");

        let writes = Rc::new(Cell::new(0));
        let counter = writes.clone();
        imp.name
            .connect_notify_local(Some("text"), move |_, _| counter.set(counter.get() + 1));
        popover.set_name("dev");
        assert_eq!(writes.get(), 0, "an unchanged name writes nothing");

        imp.name.set_text("chat");
        popover.set_name("mail");
        assert_eq!(
            imp.name.text(),
            "chat",
            "a reconcile must not clobber what the user is typing"
        );

        let submitted = Rc::new(RefCell::new(None));
        let sink = submitted.clone();
        popover.connect_submitted(move |_, name| {
            sink.replace(Some(name));
        });
        imp.name.emit_activate();
        assert_eq!(submitted.borrow().as_deref(), Some("chat"));

        imp.name
            .emit_by_name::<()>("icon-press", &[&gtk4::EntryIconPosition::Secondary]);
        assert_eq!(imp.name.text(), "", "the clear icon empties the entry");

        let cancelled = Rc::new(Cell::new(false));
        let sink = cancelled.clone();
        popover.connect_cancelled(move |_| sink.set(true));
        popover.emit_by_name::<()>("cancelled", &[]);
        assert!(cancelled.get());

        let footer = Rc::new(Cell::new(false));
        let sink = footer.clone();
        popover.connect_footer_activated(move |_| sink.set(true));
        assert!(!imp.footer.get_visible(), "no footer until one is set");
        popover.set_footer(Some("Workspace settings"));
        assert!(imp.footer.get_visible());
        imp.footer.emit_clicked();
        assert!(footer.get());
    }
}
