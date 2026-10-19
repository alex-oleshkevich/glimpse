use super::*;

fn frame() -> glimpse_wayland::capture::Frame {
    glimpse_wayland::capture::Frame {
        connector: "eDP-1".to_owned(),
        width: 2,
        height: 2,
        pixels: glib::Bytes::from_owned(vec![0u8; 16]),
        alpha: false,
        layout: glimpse_wayland::capture::Layout::Rgbx,
    }
}

#[test]
fn only_a_screen_capture_flashes_and_only_when_asked_and_animated() {
    let screen = Source::Output {
        name: "eDP-1".to_owned(),
        scale: glimpse_shot::doc::units::Scale(1.0),
        frame: frame(),
    };
    let window = Source::Window {
        id: 7,
        app_id: None,
        title: None,
        scale: glimpse_shot::doc::units::Scale(1.0),
        frame: frame(),
    };
    assert!(flashes(true, true, &screen));
    assert!(!flashes(false, true, &screen));
    assert!(!flashes(true, false, &screen));
    assert!(!flashes(true, true, &window));
}

#[test]
fn shift_enter_saves_to_the_default_path_and_copy_names_it_only_for_json() {
    let default = Some(PathBuf::from("/home/me/Pictures/Screenshots/shot.png"));
    assert_eq!(
        finish_path(FinishKey::SaveAsClose, default.clone(), false),
        default,
        "Shift+Enter needs no dialog"
    );
    assert_eq!(
        finish_path(FinishKey::Save, default.clone(), false),
        default
    );
    assert_eq!(
        finish_path(FinishKey::CopyClose, default.clone(), false),
        None
    );
    assert_eq!(
        finish_path(FinishKey::CopyClose, default.clone(), true),
        default
    );
    assert_eq!(finish_path(FinishKey::OpenInWindow, default, false), None);
    let (state, effects) = session::step(
        Phase::Editing {
            saved: None,
            copied: false,
            face: Face {
                headless: false,
                notify: false,
            },
        },
        Event::Finish(
            FinishKey::SaveAsClose,
            Some(PathBuf::from("/home/me/Pictures/Screenshots/shot.png")),
        ),
    );
    assert_eq!(effects, [Effect::Encode], "{state:?}");
}

#[test]
fn the_finish_menu_hides_open_in_window_under_json() {
    assert_eq!(
        finish_actions(false),
        ["save", "save-as", "open-with", "open-window"]
    );
    assert_eq!(finish_actions(true), ["save", "save-as", "open-with"]);
}

#[test]
fn portal_keys_only_share_apply_crop_or_refuse() {
    let none = gdk::ModifierType::empty();
    assert_eq!(
        portal_key(gdk::Key::Return, none, false),
        Some(PortalKey::Share)
    );
    assert_eq!(
        portal_key(gdk::Key::KP_Enter, none, true),
        Some(PortalKey::ApplyCrop)
    );
    assert_eq!(
        portal_key(gdk::Key::Escape, none, false),
        Some(PortalKey::Refuse)
    );
    for (key, state) in [
        (gdk::Key::Return, gdk::ModifierType::SHIFT_MASK),
        (gdk::Key::c, gdk::ModifierType::CONTROL_MASK),
        (gdk::Key::s, gdk::ModifierType::CONTROL_MASK),
        (
            gdk::Key::s,
            gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK,
        ),
        (gdk::Key::e, gdk::ModifierType::CONTROL_MASK),
    ] {
        assert_eq!(portal_key(key, state, false), None);
    }
}

#[test]
fn portal_share_result_names_the_absolute_file_written_from_a_relative_directory() {
    let folder = tempfile::tempdir_in(".").expect("temporary screenshot folder");
    let relative = PathBuf::from(".").join(folder.path().file_name().expect("folder name"));
    let config = ScreenshotConfig {
        directory: Some(relative),
        ..ScreenshotConfig::default()
    };
    let destination = Destination::of(None, false, &config).expect("save destination");
    let requested = destination
        .requested(&config, Local::now())
        .expect("requested screenshot path");
    let png = Png {
        bytes: include_bytes!("../../../tests/golden/every_mark.png").to_vec(),
        width: 2,
        height: 2,
    };
    let actual = finish::write(&destination, &requested, &Disk, &png.bytes, &mut Vec::new())
        .expect("screenshot written");
    let document = Document {
        source: Source::Output {
            name: "eDP-1".to_owned(),
            scale: glimpse_shot::doc::units::Scale(1.0),
            frame: frame(),
        },
        crop: None,
        marks: Vec::new(),
        selected: None,
    };
    let line = result_line(
        &Outcome::Shared(actual.clone()),
        &document,
        &png,
        TargetV1::Screen {
            name: "eDP-1".to_owned(),
        },
    )
    .expect("result object");
    let result: ResultV1 = serde_json::from_str(&line).expect("result json");
    let absolute = std::path::absolute(&actual).expect("absolute result path");
    assert_eq!(result.path.as_deref(), Some(absolute.as_path()));
    assert!(result.path.as_ref().is_some_and(|path| path.is_absolute()));
    assert!(!result.copied);
    assert_eq!(std::fs::read(actual).expect("saved image"), png.bytes);
}

#[test]
#[ignore = "needs a display"]
fn a_cancelled_run_answers_once_as_cancelled_and_stays_off_the_app() {
    gtk4::init().expect("display");
    adw::init().expect("libadwaita");
    assert!(glimpse_widgets::register_resources().is_ok());
    let app = adw::Application::builder()
        .application_id("me.aresa.Glimpse.Shot.Test")
        .build();
    let ended = Rc::new(RefCell::new(Vec::new()));
    let options = Options {
        target: Target::Screen("eDP-1".to_owned()),
        in_window: false,
        delay: 60,
        cursor: false,
        flash: false,
        notify: false,
        style: Style {
            color: glimpse_config::PaletteColor::Red,
            stroke: glimpse_config::Stroke::Medium,
        },
        prompt: None,
        portal: None,
        timeout: None,
        json: false,
        scale: ScaleMode::Buffer,
        marks: Vec::new(),
        output: None,
        config: glimpse_config::ScreenshotConfig::default(),
        config_path: None,
        snapshot: None,
        tool: None,
    };
    let running = start(&app, options, {
        let ended = Rc::clone(&ended);
        move |end| ended.borrow_mut().push(end)
    });
    assert!(running.0.pill.borrow().is_some(), "the countdown is up");
    assert!(
        app.windows().is_empty(),
        "no overlay window belongs to the app"
    );
    running.cancel();
    running.cancel();
    let ended = ended.borrow();
    assert_eq!(ended.len(), 1);
    assert!(matches!(
        ended[0],
        Ended::Finished {
            outcome: Outcome::Cancelled,
            ..
        }
    ));
    assert!(running.0.pill.borrow().is_none(), "the countdown is gone");
}

#[test]
fn a_window_shown_below_its_logical_size_says_how_much() {
    assert_eq!(readout(2551, 2055, Some(62)), "2551 × 2055 · scaled 62%");
    assert_eq!(readout(800, 600, Some(100)), "800 × 600");
    assert_eq!(readout(1440, 612, None), "1440 × 612");
}

#[test]
fn a_countdown_rounds_up_to_the_next_whole_second() {
    assert_eq!(remaining(Instant::now() + Duration::from_millis(2100)), 3);
    assert_eq!(remaining(Instant::now()), 0);
}

#[test]
#[ignore = "needs a display"]
fn overlay_text_entry_is_readable_and_grows() {
    gtk4::init().expect("nested display");
    adw::init().expect("libadwaita");
    let _styles = glimpse_widgets::Styles::install(adw::ColorScheme::Default);
    let canvas = gtk4::DrawingArea::new();
    canvas.set_size_request(800, 600);
    let (scroller, text) = overlay_text_entry();
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&canvas));
    overlay.add_overlay(&scroller);
    let window = gtk4::Window::new();
    window.set_child(Some(&overlay));
    window.present();
    let settle = || {
        let context = glib::MainContext::default();
        for _ in 0..30 {
            while context.pending() {
                context.iteration(false);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    settle();
    scroller.set_visible(true);
    settle();
    let initial = (text.width(), scroller.height());
    text.buffer().set_text("first\nsecond\nthird\nfourth");
    settle();
    for _ in 0..270 {
        if scroller.height() >= scroller.height_request() {
            break;
        }
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let grown = (text.width(), scroller.height());
    assert!(
        initial.0 > 100 && initial.1 > 20,
        "text entry is tiny: {initial:?}"
    );
    assert!(
        grown.1 > initial.1 + 10,
        "text entry did not grow: {initial:?} -> {grown:?}, request={}, end={:?}, lines={}, mapped={}/{}/{}, window={}x{}",
        scroller.height_request(),
        text.iter_location(&text.buffer().end_iter()),
        text.buffer().line_count(),
        scroller.is_mapped(),
        overlay.is_mapped(),
        window.is_mapped(),
        window.width(),
        window.height(),
    );
    text.buffer().set_text(&"line\n".repeat(200));
    settle();
    assert!(scroller.height() <= canvas.height());
    let adjustment = scroller.vadjustment();
    assert!(
        adjustment.upper() > adjustment.page_size(),
        "height={} canvas={} adjustment={}/{} end={:?}",
        scroller.height(),
        canvas.height(),
        adjustment.upper(),
        adjustment.page_size(),
        text.iter_location(&text.buffer().end_iter()),
    );
}
