use super::*;

use crate::bluetooth_pairing_dialog::PIN_MAX;

use crate::forecast::ForecastDay;

use crate::pager::PagerItem;

use crate::player_list::PlayerRow;

use adw::prelude::AlertDialogExt;

use adw::prelude::{ActionRowExt, ComboRowExt, PreferencesRowExt};

use gtk4::gdk;

use gtk4::prelude::*;

use gtk4::subclass::prelude::*;

use std::cell::{Cell, RefCell};

use std::rc::Rc;

const MAX_DEPTH: usize = 2;

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
    gtk4::init().expect("display");
    register_resources().expect("resources");
    let _styles = Styles::install(adw::ColorScheme::Default);

    indicator_checks::check();
    hero_checks::check();
    inhibitor_list_checks::check();
    calendar_checks::check();
    notice_checks::check();
    notification_card_checks::check();
    let four = notification_stack_checks::check();
    notifications_popover_checks::check();
    readout_checks::check();
    range_bar_checks::check();
    fact_list_checks::check();
    forecast_checks::check();
    placeholder_checks::check();
    section_checks::check();
    event_list_checks::check();
    world_clock_checks::check();
    scrubber_checks::check();
    transport_checks::check();
    now_playing_checks::check();
    choice_list_checks::check();
    keyboard_popover_checks::check();
    player_list_checks::check();
    pager_checks::check();
    workspaces_popover_checks::check();
    calendar_popover_checks::check();
    mpris_popover_checks::check();
    weather_popover_checks::check();
    tray_strip_checks::check();
    bluetooth_popover_checks::check();
    pairing_dialog_checks::check();
    network_popover_checks::check();
    audio_popover_checks::check();
    source_list_checks::check();
    display_checks::check();
    idle_popover_checks::check();
    session_popover_checks::check();
    swatch_checks::check();
    popover_arrow_checks::check();
    expandable_checks::check();
    popover_shell_checks::check();
    notification_stack_checks::peeking(four);
    hero_checks::heights();
    chart_checks::check();
    widget_demo_popover_shows_the_header_check();
    popover_shell_title_setter_short_circuits_check();
    applet_popover_size_sets_the_wide_class_and_short_circuits_check();
    container_paint_and_width_check();
    row_checks::checks();
    expander_row_checks::check();
    slider_row_checks::check();
    entry_row_checks::check();
    exit_presence_checks::check(&_styles);
    power_popover_checks::check();
    held_display_readout_check();
    toggle_disabled_accessibility_check();
    expander_row_checks::geometry();
    blur_checks::check();
    display_checks::pause();
}

#[test]
#[ignore = "needs a display"]
fn ellipsized_meter_row_title_shows_its_text_as_tooltip() {
    gtk4::init().expect("display");
    register_resources().expect("resources");
    let long = "Battery of the wireless keyboard in the living room";
    let meter = MeterRow::new();
    let row = meter.upcast_ref::<Row>();
    row.set_title(Some(long));
    let window = gtk4::Window::new();
    window.set_child(Some(&meter));
    window.present();
    let context = glib::MainContext::default();
    while context.iteration(false) {}
    let title = row.imp().title.get();
    assert!(title.has_tooltip());

    meter.allocate(
        1000,
        meter.measure(gtk4::Orientation::Vertical, 1000).1,
        -1,
        None,
    );
    assert_eq!(ellipsis_tooltip(&title), None);

    meter.allocate(
        120,
        meter.measure(gtk4::Orientation::Vertical, 120).1,
        -1,
        None,
    );
    assert_eq!(ellipsis_tooltip(&title).as_deref(), Some(long));

    row.set_tooltip(Some("Explicit"));
    assert_eq!(ellipsis_tooltip(&title), None);
    row.set_tooltip(None::<&str>);
    assert_eq!(ellipsis_tooltip(&title).as_deref(), Some(long));
    window.destroy();
}

fn held_display_readout_check() {
    let popover = DisplayPopover::new();
    let mut source = Source {
        key: "built-in".to_owned(),
        name: "Built-in".to_owned(),
        icon: String::new(),
        value: 42.0,
        maximum: 100.0,
        floor: 0.0,
    };
    popover.set_sources(&[source.clone()]);
    let imp = popover.imp();
    imp.primary.imp().scale.set_value(63.0);
    imp.primary.imp().held.set(true);
    source.value = 20.0;
    popover.set_sources(&[source.clone()]);
    assert_eq!(imp.primary.value(), 63.0);
    assert_eq!(imp.readout.value().as_deref(), Some("63"));
    imp.primary.imp().held.set(false);
    source.value = 21.0;
    popover.set_sources(&[source]);
    assert_eq!(imp.readout.value().as_deref(), Some("21"));
}

fn toggle_disabled_accessibility_check() {
    let toggle = ToggleRow::new();
    for (sensitive, locked) in [
        (true, false),
        (false, false),
        (false, true),
        (false, false),
        (true, false),
        (true, true),
        (true, false),
    ] {
        toggle.set_sensitive(sensitive);
        toggle.set_locked(locked);
        let mismatch: Option<glib::GString> = unsafe {
            glib::translate::from_glib_full(gtk4::ffi::gtk_test_accessible_check_state(
                toggle.as_ptr().cast(),
                gtk4::ffi::GTK_ACCESSIBLE_STATE_DISABLED,
                i32::from(locked || !sensitive),
            ))
        };
        assert!(mismatch.is_none(), "{mismatch:?}");
        let active = toggle.active();
        toggle.activate();
        assert_eq!(
            toggle.active(),
            if locked || !sensitive {
                active
            } else {
                !active
            }
        );
    }
}

fn widget_demo_popover_shows_the_header_check() {
    let popover = WidgetDemoPopover::new();
    let shell = child_named::<PopoverShell>(&popover, "popover-shell");
    let header = child_named::<Hero>(&shell, "popover-shell__header");
    assert!(header.get_visible());
    assert_eq!(header.title().as_deref(), Some("Widget demo"));
    assert_eq!(header.subtitle().as_deref(), Some("Popover shell slots"));
    assert_eq!(children(&shell.body()), 10);
    let addon = shell.imp().header.imp().slot.first_child();
    let switch = addon
        .and_downcast::<gtk4::Switch>()
        .expect("the addon is the wide switch");
    switch.set_active(true);
    assert_eq!(shell.size(), PopoverSize::Wide);
    switch.set_active(false);
    assert_eq!(shell.size(), PopoverSize::Regular);
    clear_children(&shell.body());
    assert!(shell.body().first_child().is_none());
    assert!(!shell.imp().scroller.get_visible());
    assert!(header.get_visible());
}

fn applet_popover_size_sets_the_wide_class_and_short_circuits_check() {
    let popover = AppletPopover::new();
    assert!(!popover.has_css_class("applet-popover--wide"));
    let notifies = Rc::new(Cell::new(0u32));
    popover.connect_size_notify({
        let notifies = Rc::clone(&notifies);
        move |_| notifies.set(notifies.get() + 1)
    });
    popover.set_size(PopoverSize::Wide);
    assert!(popover.has_css_class("applet-popover--wide"));
    popover.set_size(PopoverSize::Wide);
    assert_eq!(notifies.get(), 1);
    popover.set_size(PopoverSize::Regular);
    assert!(!popover.has_css_class("applet-popover--wide"));
}

fn container_paint_and_width_check() {
    let container = Container::new();
    let notifies = Rc::new(Cell::new(0u32));
    container.connect_background_notify({
        let notifies = Rc::clone(&notifies);
        move |_| notifies.set(notifies.get() + 1)
    });
    container.set_background(ContainerPaint::Red);
    container.set_background(ContainerPaint::Red);
    assert_eq!(notifies.get(), 1);
    container.set_background(ContainerPaint::Blue);
    let classes = container.css_classes();
    let backgrounds: Vec<_> = classes
        .iter()
        .filter(|class| class.as_str().starts_with("container--bg-"))
        .collect();
    assert_eq!(backgrounds.len(), 1);
    assert_eq!(backgrounds[0].as_str(), "container--bg-blue");
    container.set_width(99999);
    assert_eq!(container.width(), 1024);
    assert_eq!(container.size_request(), (1024, -1));
}

fn popover_shell_title_setter_short_circuits_check() {
    let shell = PopoverShell::new();
    let header = child_named::<Hero>(&shell, "popover-shell__header");
    assert!(!header.get_visible());
    let notifies = Rc::new(Cell::new(0u32));
    header.connect_title_notify({
        let notifies = Rc::clone(&notifies);
        move |_| notifies.set(notifies.get() + 1)
    });
    shell.set_title(Some("Widget demo"));
    shell.set_title(Some("Widget demo"));
    assert_eq!(notifies.get(), 1);
    shell.set_title(None::<&str>);
    assert!(!header.get_visible());
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

fn shot_display() {
    gtk4::init().expect("display");
    register_resources().expect("resources");
    let _styles = Styles::install(adw::ColorScheme::Default);
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
mod bluetooth_popover_checks;
mod blur_checks;
mod calendar_checks;
mod calendar_popover_checks;
mod chart_checks;
mod choice_list_checks;
mod clipboard_checks;
mod display_checks;
mod entry_row_checks;
mod event_list_checks;
mod exit_presence_checks;
mod expandable_checks;
mod expander_row_checks;
mod fact_list_checks;
mod forecast_checks;
mod hero_checks;
mod idle_popover_checks;
mod indicator_checks;
mod inhibitor_list_checks;
mod keyboard_popover_checks;
mod mpris_popover_checks;
mod network_popover_checks;
mod none_if_empty_checks;
mod notice_checks;
mod notification_card_checks;
mod notification_stack_checks;
mod notifications_popover_checks;
mod now_playing_checks;
mod pager_checks;
mod pairing_dialog_checks;
mod placeholder_checks;
mod places_popover_checks;
mod player_list_checks;
mod popover_arrow_checks;
mod popover_shell_checks;
mod power_popover_checks;
mod range_bar_checks;
mod readout_checks;
mod removable_popover_checks;
mod row_checks;
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
mod slider_row_checks;
mod source_list_checks;
mod swatch_checks;
mod transport_checks;
mod tray_strip_checks;
mod weather_popover_checks;
mod workspace_name_popover_checks;
mod workspaces_popover_checks;
mod world_clock_checks;
