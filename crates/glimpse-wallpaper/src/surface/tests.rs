use super::*;

#[test]
fn should_clear_image_only_when_the_new_intent_has_no_image_and_one_is_rendered() {
    assert!(should_clear_image(None, true));
    assert!(!should_clear_image(None, false));
    assert!(!should_clear_image(Some(Path::new("/a.jpg")), true));
    assert!(!should_clear_image(Some(Path::new("/a.jpg")), false));
}

#[test]
fn touched_matches_the_primary_file() {
    let target = Path::new("/tmp/glimpse-wallpaper-test/live.jpg");
    let file = gio::File::for_path(target);
    assert!(touched(target, &file, None));
}

#[test]
fn touched_matches_the_renamed_onto_file_even_when_the_primary_is_the_temp_path() {
    let target = Path::new("/tmp/glimpse-wallpaper-test/live.jpg");
    let temp = gio::File::for_path("/tmp/glimpse-wallpaper-test/live.jpg.tmp");
    let renamed_onto = gio::File::for_path(target);
    assert!(touched(target, &temp, Some(&renamed_onto)));
}

#[test]
fn touched_ignores_an_unrelated_file_in_the_same_directory() {
    let target = Path::new("/tmp/glimpse-wallpaper-test/live.jpg");
    let other = gio::File::for_path("/tmp/glimpse-wallpaper-test/unrelated.jpg");
    assert!(!touched(target, &other, None));
}

#[test]
fn should_reconfigure_ignores_a_bare_changed_event() {
    assert!(!should_reconfigure(gio::FileMonitorEvent::Changed));
}

#[test]
fn should_reconfigure_waits_for_the_changes_done_hint() {
    assert!(should_reconfigure(gio::FileMonitorEvent::ChangesDoneHint));
}

#[test]
fn should_reconfigure_acts_on_a_rename_onto_the_target_at_once() {
    assert!(should_reconfigure(gio::FileMonitorEvent::Renamed));
}

#[test]
fn should_reconfigure_acts_on_a_create_or_move_in_at_once() {
    assert!(should_reconfigure(gio::FileMonitorEvent::Created));
    assert!(should_reconfigure(gio::FileMonitorEvent::MovedIn));
}

#[test]
fn should_reconfigure_ignores_events_unrelated_to_a_completed_write() {
    assert!(!should_reconfigure(gio::FileMonitorEvent::Deleted));
    assert!(!should_reconfigure(gio::FileMonitorEvent::AttributeChanged));
    assert!(!should_reconfigure(gio::FileMonitorEvent::Moved));
    assert!(!should_reconfigure(gio::FileMonitorEvent::MovedOut));
}
