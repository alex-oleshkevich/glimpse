use super::*;

fn texture() -> gdk::Texture {
    let pixels = glib::Bytes::from_owned(vec![0u8; 64]);
    gdk::MemoryTexture::new(4, 4, gdk::MemoryFormat::R8g8b8a8, &pixels, 16).upcast()
}

#[test]
#[ignore = "needs a display; run alone with just test-one"]
fn a_crossfade_keeps_the_outgoing_image_at_its_own_fit_and_a_cut_empties_over() {
    gtk4::init().expect("GTK display");
    let under = gtk::Picture::new();
    let over = gtk::Picture::new();
    let old = texture();
    under.set_paintable(Some(&old));
    under.set_content_fit(gtk::ContentFit::ScaleDown);

    let new = texture();
    hand_over(&under, &over, &new, gtk::ContentFit::Fill, true);
    assert_eq!(over.paintable(), Some(old.upcast()));
    assert_eq!(over.content_fit(), gtk::ContentFit::ScaleDown);
    assert_eq!(over.opacity(), 1.0);
    assert_eq!(under.paintable(), Some(new.clone().upcast()));
    assert_eq!(under.content_fit(), gtk::ContentFit::Fill);

    hand_over(&under, &over, &texture(), gtk::ContentFit::Contain, false);
    assert_eq!(over.paintable(), None);
    assert_eq!(over.opacity(), 0.0);
    assert_eq!(under.content_fit(), gtk::ContentFit::Contain);
}

#[test]
#[ignore = "needs a display; run alone with just test-one"]
fn a_skipped_fade_done_keeps_the_next_fade_outgoing_image_and_its_own_done_clears_it() {
    gtk4::init().expect("GTK display");
    let over = gtk::Picture::new();
    let outgoing = texture();
    over.set_paintable(Some(&outgoing));
    let fade = || {
        adw::TimedAnimation::new(
            &over,
            1.0,
            0.0,
            100,
            adw::PropertyAnimationTarget::new(&over, "opacity"),
        )
    };
    let skipped = fade();
    let running = fade();
    let mut animation = Some(running.clone());

    finish(&over, &mut animation, &skipped);
    assert_eq!(over.paintable(), Some(outgoing.upcast()));
    assert_eq!(animation.as_ref(), Some(&running));

    finish(&over, &mut animation, &running);
    assert_eq!(over.paintable(), None);
    assert!(animation.is_none());
}

#[test]
fn touched_matches_the_primary_file() {
    let target = &std::path::absolute("fixtures/live.jpg").unwrap();
    let file = gio::File::for_path(target);
    assert!(touched(target, &file, None));
}

#[test]
fn touched_matches_the_renamed_onto_file_even_when_the_primary_is_the_temp_path() {
    let target = &std::path::absolute("fixtures/live.jpg").unwrap();
    let temp = gio::File::for_path("fixtures/live.jpg.tmp");
    let renamed_onto = gio::File::for_path(target);
    assert!(touched(target, &temp, Some(&renamed_onto)));
}

#[test]
fn touched_ignores_an_unrelated_file_in_the_same_directory() {
    let target = &std::path::absolute("fixtures/live.jpg").unwrap();
    let other = gio::File::for_path("fixtures/unrelated.jpg");
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
