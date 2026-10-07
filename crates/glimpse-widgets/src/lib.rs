pub(crate) mod action_row;
pub mod actions;
mod applet_popover;
mod artwork;
mod audio_popover;
mod bars;
mod bluetooth_pairing_dialog;
mod bluetooth_popover;
pub mod blur;
mod calendar;
mod calendar_popover;
mod choice_list;
mod clipboard_list;
mod clipboard_popover;
mod color_list;
mod color_picker_popover;
mod container;
mod display_list;
mod display_popover;
mod dots;
mod drawer;
mod event_list;
mod expandable;
mod expander_row;
mod fact_list;
mod fader;
mod forecast;
mod hero;
mod idle_popover;
mod indicator;
mod indicator_group;
mod inhibitor_list;
mod kdeconnect_popover;
mod keyboard_popover;
pub(crate) mod legacy_row;
mod lens;
mod lock_clock;
mod lock_stage;
pub(crate) mod meter_row;
mod monitors;
mod mpris_popover;
mod network_popover;
mod network_secret_dialog;
mod next_event_popover;
mod notice;
mod notification_card;
mod notification_chips;
mod notification_header;
mod notification_image_body;
mod notification_list;
mod notification_stack;
mod notification_text_body;
mod notifications_popover;
mod now_playing;
mod pager;
mod panel;
mod password_prompt;
mod placeholder;
mod places_popover;
mod player_list;
mod popover_shell;
mod power_popover;
mod printing_popover;
mod privacy_popover;
mod range_bar;
pub mod raster;
mod readout;
pub mod reconcile;
mod removable_popover;
pub(crate) mod row;
mod ruler_popover;
mod scrubber;
mod section;
mod session_popover;
mod session_sheet;
mod shot_banner;
mod shot_capture_page;
mod shot_countdown;
mod shot_preferences;
mod shot_region;
mod shot_result_page;
mod shot_shortcuts;
mod shot_toolbar;
mod shot_window_list;
mod slider_row;
mod source_list;
mod sparkline;
mod split_row;
mod status_island;
mod swatch;
mod switch_row;
mod system_monitor_popover;
mod theme;
pub(crate) mod toggle_row;
mod tooltip_card;
mod track_card;
mod transport;
mod tray_strip;
mod weather_popover;
mod widget_demo_popover;
mod workspace_list;
mod workspace_name_popover;
mod workspace_section;
mod workspaces_popover;
mod world_clock;

pub use action_row::ActionRow;
pub use applet_popover::{AppletPopover, PopoverSize};
pub use artwork::{Thumbnail, artwork, thumbnail};
pub use audio_popover::{
    AudioPopover, Block as AudioBlock, Details as AudioDetails, Entry as AudioEntry,
};
pub use bars::Bars;
pub use bluetooth_pairing_dialog::{
    Entry as PairingEntry, PASSKEY_MAX, PIN_MAX, PairingAnswer, PairingDialog,
};
pub use bluetooth_popover::{
    Ask as BluetoothAsk, BluetoothPopover, Details as BluetoothDetails, Entry as BluetoothEntry,
    Line as BluetoothLine, Place as BluetoothPlace,
};
pub use blur::roundtrip;
pub use calendar::{Calendar, Ymd};
pub use calendar_popover::CalendarPopover;
pub use choice_list::{Choice, ChoiceList};
pub use clipboard_list::{Actions as ClipActions, Clip, ClipAction, ClipboardList};
pub use clipboard_popover::ClipboardPopover;
pub use color_list::{ColorList, Notation, Shade};
pub use color_picker_popover::ColorPickerPopover;
pub use container::{Container, Paint as ContainerPaint};
pub use display_list::{Display, DisplayList, DisplayLogical, DisplayMode};
pub use display_popover::{DisplayPopover, NightLight};
pub use dots::{Dots, SIZE as DOTS_SIZE};
pub use event_list::{Event, EventList, Link as EventLink};
pub use expandable::Expandable;
pub use expander_row::ExpanderRow;
pub use fact_list::{Fact, FactList};
pub use fader::Fader;
pub use forecast::{Day, ForecastDay, ForecastHour, ForecastList, ForecastStrip, Hour};
pub use hero::Hero;
pub use idle_popover::IdlePopover;
pub use indicator::{Indicator, IndicatorSpec};
pub use indicator_group::IndicatorGroup;
pub use inhibitor_list::{InhibitorEntry, InhibitorList, InhibitorSource, InhibitorTargets};
pub use kdeconnect_popover::{
    Action as KdeconnectAction, Device as KdeconnectDevice, KdeconnectPopover,
    Nearby as KdeconnectNearby,
};
pub use keyboard_popover::{KeyboardPopover, Layout as KeyboardLayout};
pub use legacy_row::{LegacyRow, beside};
pub use lens::{Lens, zoomed};
pub use lock_clock::LockClock;
pub use lock_stage::LockStage;
pub use meter_row::MeterRow;
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
pub use notification_chips::{ChipGroup, NotificationChips};
pub use notification_header::{NotificationHeader, notification_icon};
pub use notification_image_body::{NotificationImageBody, notification_image};
pub use notification_list::{Body, Notification, NotificationList};
pub use notification_stack::NotificationStack;
pub use notification_text_body::NotificationTextBody;
pub use notifications_popover::{Group, NotificationsPopover};
pub use now_playing::NowPlaying;
pub use pager::{Focus, Pager, PagerItem, Shape, Slot};
pub use panel::Panel;
pub use password_prompt::{MessageKind, PasswordPrompt};
pub use placeholder::Placeholder;
pub use places_popover::{Entry as PlacesEntry, PlacesPopover, Trash as PlacesTrash};
pub use player_list::{Player, PlayerList, PlayerRow};
pub use popover_shell::PopoverShell;
pub use power_popover::{
    ChargeLimit as PowerChargeLimit, Device as PowerDevice, History as PowerHistory,
    HoldNotice as PowerHoldNotice, PowerPopover,
};
pub use printing_popover::{Job as PrintingJob, Printer as PrintingPrinter, PrintingPopover};
pub use privacy_popover::{PrivacyPopover, Usage as PrivacyUsage};
pub use range_bar::RangeBar;
pub use readout::Readout;
pub use removable_popover::{
    Drive as RemovableDrive, RemovablePopover, Space as RemovableSpace, Volume as RemovableVolume,
};
pub use row::{Row, RowTone, RowTypography};
pub use ruler_popover::{HistoryEntry, RulerPopover};
pub use scrubber::{Scrubber, clock};
pub use section::Section;
pub use session_popover::{
    ActionState as SessionActionState, HIBERNATE, LOCK, LOG_OUT, POWER_OFF, REBOOT, SUSPEND,
    SessionPopover,
};
pub use session_sheet::SessionSheet;
pub use shot_banner::ShotBanner;
pub use shot_capture_page::ShotCapturePage;
pub use shot_countdown::ShotCountdown;
pub use shot_preferences::{ShotPreferences, ShotPreferencesFields};
pub use shot_region::ShotRegion;
pub use shot_result_page::ShotResultPage;
pub use shot_shortcuts::shot_shortcuts;
pub use shot_toolbar::ShotToolbar;
pub use shot_window_list::{ShotWindowEntry, ShotWindowList};
pub use slider_row::SliderRow;
pub use source_list::{Source, SourceList};
pub use sparkline::{Sparkline, Tick};
pub use split_row::SplitRow;
pub use status_island::StatusIsland;
pub use swatch::{Swatch, rgba};
pub use switch_row::SwitchRow;
pub use system_monitor_popover::{
    Meter as SystemMonitorMeter, NamedMeter as SystemMonitorNamedMeter,
    Readings as SystemMonitorReadings, SystemMonitorPopover,
};
pub(crate) use theme::animation_ms;
pub use theme::{Sheets, Styles, duration_ms};
pub use toggle_row::{ToggleControl, ToggleRow};
pub use tooltip_card::TooltipCard;
pub use track_card::{Track, TrackCard};
pub use transport::{Repeat, Transport, TransportAction};
pub use tray_strip::{Edge, TrayChip, TrayStrip};
pub use weather_popover::{Advisory, Page as WeatherPage, WeatherPopover, alert_page, day_page};
pub use widget_demo_popover::WidgetDemoPopover;
pub use workspace_list::{Window as WorkspaceWindow, Workspace, WorkspaceList};
pub use workspace_name_popover::WorkspaceNamePopover;
pub use workspace_section::WorkspaceSection;
pub use workspaces_popover::WorkspacesPopover;
pub use world_clock::{WorldClock, Zone};

#[cfg(test)]
use indicator::{LABEL_MAX_CHARS, TOOLTIP_MAX_CHARS};

pub(crate) const TEXT_MAX_CHARS: usize = 128;

pub(crate) fn replace_children(container: &gtk4::Box, child: Option<&gtk4::Widget>) {
    use gtk4::prelude::*;

    if let Some(child) = child {
        container.append(child);
    }
    while let Some(old) = container.first_child().filter(|old| Some(old) != child) {
        container.remove(&old);
    }
}

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

pub(crate) fn fade_out(
    owner: &impl gtk4::prelude::IsA<gtk4::Widget>,
    leaving: Vec<gtk4::Widget>,
    done: impl FnOnce() + 'static,
) {
    use adw::prelude::*;

    for widget in &leaving {
        widget.add_css_class("leaving");
    }
    let fading = leaving.clone();
    let target = adw::CallbackAnimationTarget::new(move |value| {
        for widget in &fading {
            widget.set_opacity(value);
        }
    });
    let animation = adw::TimedAnimation::new(owner, 1.0, 0.0, duration_ms(), target);
    animation.set_easing(adw::Easing::EaseOutCubic);
    let done = std::cell::Cell::new(Some(done));
    animation.connect_done(move |_| {
        for widget in &leaving {
            widget.remove_css_class("leaving");
            widget.set_opacity(1.0);
        }
        if let Some(done) = done.take() {
            done();
        }
    });
    animation.play();
    if duration_ms() == 0 {
        animation.skip();
    }
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

pub(crate) fn fact_row(fact: &Fact) -> Row {
    let row = Row::new();
    row.set_title(Some(fact.label.as_str()));
    row.set_value(Some(fact.value.as_str()));
    if fact.warning {
        row.set_tone(RowTone::Warning);
    }
    row
}

pub(crate) fn set_action_label(row: &ActionRow, label: Option<&str>) {
    use gtk4::prelude::*;

    row.upcast_ref::<Row>().set_title(label);
    if row.get_visible() != label.is_some() {
        row.set_visible(label.is_some());
    }
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
mod tests;
