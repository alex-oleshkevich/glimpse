use super::*;

use crate::bluetooth_pairing_dialog::PIN_MAX;

use adw::prelude::AlertDialogExt;

use adw::prelude::{ActionRowExt, ComboRowExt, PreferencesRowExt};

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

    indicator_checks::check();
    hero_checks::check();
    let row = Row::new();
    row_checks::slots(row.clone());
    inhibitor_list_checks::check();
    calendar_checks::check();
    notice_checks::check();
    notification_card_checks::check();
    let four = notification_list_checks::check();
    notifications_popover_checks::check();
    readout_checks::check();
    range_bar_checks::check();
    fact_list_checks::check();
    forecast_checks::check();
    placeholder_checks::check();
    row_checks::activation(row.clone());
    switch_row_checks::check();
    section_checks::check();
    event_list_checks::check();
    world_clock_checks::check();
    scrubber_checks::check();
    fader_checks::check();
    transport_checks::check();
    now_playing_checks::check();
    choice_list_checks::check();
    keyboard_popover_checks::check();
    player_list_checks::check();
    split_row_checks::check();
    pager_checks::check();
    workspaces_popover_checks::check();
    row_checks::kinds();
    calendar_popover_checks::check();
    mpris_popover_checks::check();
    weather_popover_checks::check();
    tray_strip_checks::check();
    tooltip_card_checks::check();
    bluetooth_popover_checks::check();
    pairing_dialog_checks::check();
    network_popover_checks::check();
    audio_popover_checks::check();
    source_list_checks::check();
    display_checks::check();
    idle_popover_checks::check();
    session_popover_checks::check();
    battery_popover_checks::check();
    swatch_checks::check();
    color_list_checks::check();
    expandable_checks::check();
    popover_shell_checks::check();
    system_monitor_popover_checks::check();
    ruler_popover_checks::check();
    notification_stack_checks::peeking(four);
    hero_checks::heights();
    chart_checks::check();
    row_checks::beside();
}

fn children(parent: &impl IsA<gtk4::Widget>) -> usize {
    let mut count = 0;
    let mut child = parent.upcast_ref::<gtk4::Widget>().first_child();
    while let Some(widget) = child {
        count += 1;
        child = widget.next_sibling();
    }
    count
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

fn group_sections(groups: &gtk4::Box) -> Vec<Section> {
    children_of::<gtk4::Revealer>(groups)
        .iter()
        .filter_map(|revealer| revealer.child().and_downcast())
        .collect()
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

fn shot_display() -> bool {
    if gtk4::init().is_err() {
        return false;
    }
    register_resources().expect("resources");
    let _styles = Styles::install(adw::ColorScheme::Default);
    true
}

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

mod audio_popover_checks;
mod battery_popover_checks;
mod bluetooth_popover_checks;
mod calendar_checks;
mod calendar_popover_checks;
mod chart_checks;
mod choice_list_checks;
mod clipboard_checks;
mod color_list_checks;
mod display_checks;
mod event_list_checks;
mod expandable_checks;
mod fact_list_checks;
mod fader_checks;
mod forecast_checks;
mod hero_checks;
mod idle_popover_checks;
mod indicator_checks;
mod inhibitor_list_checks;
mod kdeconnect_popover_checks;
mod keyboard_popover_checks;
mod mpris_popover_checks;
mod network_popover_checks;
mod none_if_empty_checks;
mod notice_checks;
mod notification_card_checks;
mod notification_list_checks;
mod notification_stack_checks;
mod notifications_popover_checks;
mod now_playing_checks;
mod pager_checks;
mod pairing_dialog_checks;
mod placeholder_checks;
mod places_popover_checks;
mod player_list_checks;
mod popover_shell_checks;
mod range_bar_checks;
mod readout_checks;
mod removable_popover_checks;
mod row_checks;
mod ruler_popover_checks;
mod scrubber_checks;
mod section_checks;
mod session_popover_checks;
mod shot_banner_checks;
mod shot_capture_page_checks;
mod shot_countdown_checks;
mod shot_icons_checks;
mod shot_region_checks;
mod shot_result_page_checks;
mod shot_toolbar_checks;
mod shot_window_list_checks;
mod source_list_checks;
mod split_row_checks;
mod swatch_checks;
mod switch_row_checks;
mod system_monitor_popover_checks;
mod tooltip_card_checks;
mod transport_checks;
mod tray_strip_checks;
mod weather_popover_checks;
mod workspace_name_popover_checks;
mod workspaces_popover_checks;
mod world_clock_checks;
