use super::*;
use glib::translate::IntoGlib;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn shown(visible: bool) -> SessionActionState {
    SessionActionState {
        visible,
        enabled: true,
        subtitle: None,
    }
}

fn press(stage: &LockStage, key: gdk::Key) -> bool {
    press_with(stage, key, gdk::ModifierType::empty())
}

fn key_controller(stage: &LockStage) -> gtk4::EventControllerKey {
    stage
        .observe_controllers()
        .into_iter()
        .filter_map(Result::ok)
        .find_map(|controller| controller.downcast::<gtk4::EventControllerKey>().ok())
        .expect("the stage carries a key controller")
}

fn press_with(stage: &LockStage, key: gdk::Key, state: gdk::ModifierType) -> bool {
    let keys = key_controller(stage);
    assert_eq!(
        keys.propagation_phase(),
        gtk4::PropagationPhase::Capture,
        "the key controller sees Escape before any child"
    );
    keys.emit_by_name::<bool>("key-pressed", &[&key.into_glib(), &0u32, &state])
}

fn prompt_focused(window: &gtk4::Window, stage: &LockStage) -> bool {
    RootExt::focus(window).is_some_and(|focus| focus.is_ancestor(stage.prompt()))
}

#[test]
#[ignore = "needs a display"]
fn lock_stage_states() {
    gtk4::init().expect("display");
    crate::register_resources().expect("resources");

    let stage = LockStage::new();
    let window = gtk4::Window::new();
    window.set_child(Some(&stage));
    let imp = stage.imp();

    assert!(
        stage.has_css_class("lock-stage"),
        "the class every lock rule is scoped under"
    );
    stage.clock().set_formats("%H:%M", "%A");
    stage
        .clock()
        .set_time(&glib::DateTime::from_unix_utc(0).expect("epoch"));
    let (min_height, _, _, _) = stage.measure(gtk4::Orientation::Vertical, -1);
    assert!(min_height > 0, "the stage asks for its layout's minimum");

    let huge = gdk::MemoryTexture::new(
        3840,
        2160,
        gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from_owned(vec![0u8; 3840 * 2160 * 4]),
        3840 * 4,
    );
    let empty = LockStage::new();
    let (_, empty_width, _, _) = empty.measure(gtk4::Orientation::Horizontal, -1);
    let photo = LockStage::new();
    photo.set_background(Some(huge.upcast_ref::<gdk::Texture>()));
    let (_, natural_width, _, _) = photo.measure(gtk4::Orientation::Horizontal, -1);
    let (_, natural_height, _, _) = photo.measure(gtk4::Orientation::Vertical, -1);
    assert_eq!(
        natural_width, empty_width,
        "a 4K texture does not widen the stage"
    );
    assert!(
        natural_width < 3840 && natural_height < 2160,
        "the stage's natural size is its layout's, not the texture's"
    );

    assert!(
        imp.base.paintable().is_some(),
        "a fresh stage paints a colour under an empty image"
    );
    let rebuilt = Rc::new(Cell::new(0));
    imp.base.connect_paintable_notify({
        let rebuilt = Rc::clone(&rebuilt);
        move |_| rebuilt.set(rebuilt.get() + 1)
    });
    stage.set_color(&gdk::RGBA::BLACK);
    assert_eq!(rebuilt.get(), 0, "the default colour again builds nothing");
    stage.set_color(&gdk::RGBA::RED);
    assert_eq!(rebuilt.get(), 1, "a new colour builds one texture");
    stage.set_color(&gdk::RGBA::RED);
    assert_eq!(rebuilt.get(), 1, "the same colour again builds nothing");

    let power = || {
        let mut found = None;
        let mut child = imp.status.first_child();
        while let Some(widget) = child {
            if widget.has_css_class("status-island__power") {
                found = Some(widget.clone());
            }
            child = widget.next_sibling();
        }
        found.expect("the island has a power button")
    };
    assert!(
        !power().get_visible(),
        "a sheet with no actions hides the power button from the start"
    );
    stage.set_session_action(crate::SUSPEND, &shown(true));
    assert!(
        power().get_visible(),
        "one action makes the island offer the sheet"
    );
    stage.set_session_action(crate::SUSPEND, &shown(false));
    assert!(
        !power().get_visible(),
        "the last hidden action hides it again"
    );
    stage.set_session_action(crate::SUSPEND, &shown(true));
    let error = &imp.sheet.imp().error;
    stage.set_session_error(Some("refused"));
    assert!(
        !error.get_visible(),
        "an error for a closed sheet is dropped"
    );
    imp.status.emit_by_name::<()>("session-toggled", &[]);
    stage.set_session_error(Some("refused"));
    assert!(error.get_visible(), "an open sheet shows the error");
    stage.set_session_action(crate::SUSPEND, &shown(false));
    assert!(
        !imp.sheet.is_open(),
        "losing the last action closes an open sheet"
    );
    stage.set_session_action(crate::SUSPEND, &shown(true));
    imp.status.emit_by_name::<()>("session-toggled", &[]);
    assert!(stage.session_open());
    stage.set_session_actions([
        (crate::SUSPEND, &shown(false)),
        (crate::REBOOT, &shown(true)),
    ]);
    assert!(
        imp.sheet.is_open(),
        "the sheet closes only when no action is left after the whole batch"
    );
    imp.status.emit_by_name::<()>("session-toggled", &[]);
    assert!(!stage.session_open());
    stage.set_session_action(crate::SUSPEND, &shown(true));
    stage.set_session_action(crate::REBOOT, &shown(true));

    assert!(!stage.is_awake(), "a new stage starts on the cover");
    stage.set_awake(true);
    assert!(stage.prompt().grab_focus());
    stage.prompt().preempt('x');
    assert!(
        !press(&stage, gdk::Key::Escape),
        "Escape on a closed sheet over typed text propagates"
    );
    assert!(stage.is_awake(), "and leaves the stage awake");
    stage.prompt().clear();

    imp.status.emit_by_name::<()>("session-toggled", &[]);
    assert!(
        imp.sheet.is_open(),
        "the island's power button opens the sheet"
    );
    assert!(
        !prompt_focused(&window, &stage),
        "an open sheet takes the focus"
    );
    assert!(
        !press(&stage, gdk::Key::a),
        "a key other than Escape propagates"
    );
    assert!(imp.sheet.is_open());

    assert!(
        press(&stage, gdk::Key::Escape),
        "Escape on an open sheet stops there"
    );
    assert!(!imp.sheet.is_open(), "Escape closes the sheet");
    assert!(
        prompt_focused(&window, &stage),
        "closing the sheet hands the focus back to the prompt"
    );

    imp.status.emit_by_name::<()>("session-toggled", &[]);
    assert!(imp.sheet.is_open());
    imp.status.emit_by_name::<()>("session-toggled", &[]);
    assert!(
        !imp.sheet.is_open(),
        "the power button toggles the sheet shut"
    );

    let prompt = stage.prompt().upcast_ref::<gtk4::Widget>().clone();
    let sheet_row = imp.sheet.imp().suspend.upcast_ref::<gtk4::Widget>().clone();
    assert!(
        !imp.dismisses(Some(&prompt)),
        "a closed sheet has nothing to dismiss"
    );
    imp.status.emit_by_name::<()>("session-toggled", &[]);
    assert!(
        imp.dismisses(Some(&prompt)),
        "a press elsewhere on the stage closes the sheet"
    );
    assert!(imp.dismisses(None), "a press on nothing closes it too");
    assert!(
        !imp.dismisses(Some(&sheet_row)),
        "a press inside the sheet is the sheet's own"
    );
    assert!(
        !imp.dismisses(Some(&power())),
        "the power button toggles on its own; closing here too would reopen it"
    );
    let outside = stage
        .observe_controllers()
        .into_iter()
        .filter_map(Result::ok)
        .find_map(|controller| controller.downcast::<gtk4::GestureClick>().ok())
        .expect("the stage carries a click gesture");
    assert_eq!(
        outside.propagation_phase(),
        gtk4::PropagationPhase::Capture,
        "the press is seen before the widget under it takes it"
    );
    imp.sheet.close();

    let actions = Rc::new(RefCell::new(Vec::new()));
    stage.connect_session_action({
        let actions = Rc::clone(&actions);
        move |_, action| actions.borrow_mut().push(action.to_owned())
    });
    imp.sheet
        .emit_by_name::<()>("action-requested", &[&crate::SUSPEND]);
    assert_eq!(*actions.borrow(), [crate::SUSPEND]);

    imp.status.emit_by_name::<()>("session-toggled", &[]);
    stage.close_session();
    assert!(!imp.sheet.is_open(), "close_session closes an open sheet");

    stage.set_dim(0.4);
    assert!((imp.scrim.opacity() - 0.4).abs() < 0.01);
    stage.set_dim(f64::NAN);
    assert!(
        (imp.scrim.opacity() - 0.4).abs() < 0.01,
        "a NaN dim is ignored rather than written"
    );
    stage.set_dim(3.0);
    assert!((imp.scrim.opacity() - 1.0).abs() < 0.01, "dim clamps to 1");
    stage.set_dim(-1.0);
    assert!(imp.scrim.opacity().abs() < 0.01, "dim clamps to 0");

    let texture = gdk::MemoryTexture::new(
        2,
        2,
        gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from_static(&[0; 16]),
        8,
    );
    stage.set_background(Some(texture.upcast_ref::<gdk::Texture>()));
    assert!(imp.background.paintable().is_some());
    stage.set_background(None);
    assert!(imp.background.paintable().is_none());
    stage.set_fit(gtk4::ContentFit::Contain);
    assert_eq!(imp.background.content_fit(), gtk4::ContentFit::Contain);

    stage.set_interactive(false);
    assert!(!stage.is_awake(), "a mirror drops back to the cover");
    assert!(
        !stage.prompt().grab_focus(),
        "a non-interactive stage's prompt refuses focus"
    );
    assert!(!press(&stage, gdk::Key::a), "a mirror lets a key through");
    assert!(!stage.is_awake(), "and does not wake for it");
    assert!(stage.prompt().is_empty(), "nor types it");

    stage.set_interactive(true);
    stage.set_awake(true);
    stage.prompt().set_interactive(false);
    imp.status.emit_by_name::<()>("session-toggled", &[]);
    assert!(press(&stage, gdk::Key::Escape));
    assert!(
        RootExt::focus(&window).is_some_and(|focus| focus == power()),
        "with no prompt to return to, the focus goes back to the power button"
    );
    stage.prompt().set_interactive(true);
    assert!(stage.prompt().grab_focus());

    window.destroy();
}

#[test]
#[ignore = "needs a display"]
fn the_cover_wakes_into_the_prompt() {
    gtk4::init().expect("display");
    crate::register_resources().expect("resources");

    let stage = LockStage::new();
    let window = gtk4::Window::new();
    window.set_child(Some(&stage));

    assert!(!stage.has_css_class(imp::AWAKE));
    assert_eq!(
        stage.imp().hidden.get(),
        Some([true, true, true, false]),
        "on the cover the island, the bottom row and the unlock group are hidden from assistive technology, the cover is not"
    );
    assert!(
        !stage.prompt().forward_key(&key_controller(&stage)),
        "outside a key handler there is no event to forward, so the waking key falls back to typing"
    );
    let imp = stage.imp();
    assert!(
        !imp.status.can_target() && !imp.cover.can_target(),
        "nothing in the corner is clickable on the cover"
    );
    stage.set_notifications(&[ChipGroup {
        key: "chat".to_owned(),
        app: "Chat".to_owned(),
        icon: None,
        count: 2,
    }]);
    let chip = imp.status.imp().chips.borrow()[0].1.clone();
    assert_eq!(
        chip.parent().as_ref(),
        Some(imp.status.upcast_ref()),
        "the chips are direct children of the island"
    );
    assert_eq!(
        imp.status.first_child().as_ref(),
        Some(chip.upcast_ref()),
        "the chips lead the island"
    );
    assert!(
        !press(&stage, gdk::Key::Shift_L),
        "a modifier on its own passes through"
    );
    assert!(!press(&stage, gdk::Key::Caps_Lock));
    assert!(!stage.is_awake(), "and leaves the cover up");
    assert!(!press(&stage, gdk::Key::Escape));
    assert!(!stage.is_awake(), "Escape on the cover wakes nothing");

    assert!(
        press(&stage, gdk::Key::a),
        "the waking key is consumed once"
    );
    assert!(stage.is_awake());
    assert!(stage.has_css_class(imp::AWAKE));
    assert_eq!(
        stage.imp().hidden.get(),
        Some([false, false, false, true]),
        "awake reverses them"
    );
    assert!(imp.status.can_target(), "waking makes the corner clickable");
    assert!(!imp.cover.can_target());
    assert!(
        prompt_focused(&window, &stage),
        "waking hands the focus to the prompt"
    );
    assert!(
        press(&stage, gdk::Key::b),
        "a key right behind the waking one is typed too, before the input method is ready"
    );
    assert_eq!(
        stage.prompt().take_text().as_str(),
        "ab",
        "the key that woke the cover is the first character typed"
    );

    stage.prompt().preempt('b');
    assert!(
        !press(&stage, gdk::Key::Escape),
        "Escape with text typed stays awake"
    );
    assert!(stage.is_awake());
    stage.set_awake(false);
    assert!(
        stage.prompt().is_empty(),
        "returning to the cover clears what was typed"
    );
    assert!(
        RootExt::focus(&window).is_some_and(|focus| focus == *stage.upcast_ref::<gtk4::Widget>()),
        "on the cover the stage holds the focus, so a key reaches its controller"
    );
    assert!(
        press_with(&stage, gdk::Key::a, gdk::ModifierType::CONTROL_MASK),
        "a chord wakes the cover too"
    );
    assert!(stage.is_awake());
    assert!(stage.prompt().is_empty(), "but types nothing under Control");
    stage.set_awake(false);
    assert!(press_with(
        &stage,
        gdk::Key::b,
        gdk::ModifierType::ALT_MASK | gdk::ModifierType::SHIFT_MASK
    ));
    assert!(stage.prompt().is_empty(), "or under Alt");
    stage.set_awake(false);
    assert!(press_with(
        &stage,
        gdk::Key::A,
        gdk::ModifierType::SHIFT_MASK
    ));
    assert_eq!(
        stage.prompt().take_text().as_str(),
        "A",
        "Shift alone still types"
    );
    stage.set_awake(false);
    stage.set_awake(true);
    assert!(
        press(&stage, gdk::Key::Escape),
        "Escape on an empty prompt returns to the cover"
    );
    assert!(!stage.is_awake());

    stage.set_awake(true);
    stage.prompt().set_busy(true);
    assert!(
        !press(&stage, gdk::Key::Escape),
        "Escape does not abandon a check in flight"
    );
    assert!(stage.is_awake());
    stage.prompt().set_busy(false);
    stage.set_awake(false);

    stage.set_message(Some("Authentication failed"), MessageKind::Error);
    assert!(stage.is_awake(), "a failure wakes the cover so it is seen");
    stage.set_awake(false);
    stage.set_message(None, MessageKind::Info);
    assert!(!stage.is_awake(), "clearing a message wakes nothing");
    stage.set_message(Some("Authentication failed"), MessageKind::Error);
    assert!(stage.is_awake(), "the same failure again wakes it again");
    stage.set_awake(false);
    stage.set_message(None, MessageKind::Info);

    stage.set_awake(true);
    let before = stage.imp().active.get();
    std::thread::sleep(std::time::Duration::from_millis(5));
    stage.set_message(Some("Try again"), MessageKind::Error);
    assert!(
        stage.imp().active.get() > before,
        "a new message on an awake stage restarts the idle clock"
    );
    stage.prompt().set_busy(true);
    let busy_at = stage.imp().active.get();
    std::thread::sleep(std::time::Duration::from_millis(5));
    stage.set_busy(false);
    assert!(
        stage.imp().active.get() > busy_at,
        "finishing a check restarts the idle clock"
    );
    stage.set_awake(false);
    stage.set_interactive(false);
    stage.set_message(Some("Authentication failed"), MessageKind::Error);
    assert!(!stage.is_awake(), "a mirror stays on its cover");

    window.destroy();
}

#[test]
fn initials_are_the_first_letters_of_two_words() {
    assert_eq!(initials("Alex Aresa"), "AA");
    assert_eq!(initials("ålex ørn third"), "ÅØ");
    assert_eq!(initials("  single  "), "S");
    assert_eq!(initials(""), "");
    assert_eq!(initials("ßeta ßigma"), "SS", "expansion never exceeds two");
    assert_eq!(
        initials("\u{202e}a\u{0}b c"),
        "AB",
        "hostile characters separate"
    );
    assert_eq!(initials(&"x ".repeat(500)), "XX");
}

fn settle(span: std::time::Duration) {
    let end = std::time::Instant::now() + span;
    while std::time::Instant::now() < end {
        glib::MainContext::default().iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
#[ignore = "needs a display"]
fn the_stage_returns_to_the_cover_when_idle() {
    use std::time::Duration;

    gtk4::init().expect("display");
    crate::register_resources().expect("resources");

    let stage = LockStage::new();
    let window = gtk4::Window::new();
    window.set_child(Some(&stage));
    stage.imp().timing.set(imp::Timing {
        soft: Duration::from_millis(120),
        cap: Duration::from_millis(450),
    });

    stage.set_awake(true);
    settle(Duration::from_millis(300));
    assert!(
        !stage.is_awake(),
        "an empty prompt covers after the soft limit"
    );

    stage.set_awake(true);
    stage.prompt().preempt('x');
    settle(Duration::from_millis(300));
    assert!(
        stage.is_awake() && !stage.prompt().is_empty(),
        "typed text outlasts the soft limit"
    );
    settle(Duration::from_millis(400));
    assert!(
        !stage.is_awake(),
        "the idle cap covers even with text typed"
    );
    assert!(stage.prompt().is_empty(), "and clears it");

    stage.set_awake(true);
    stage.prompt().set_busy(true);
    settle(Duration::from_millis(700));
    assert!(stage.is_awake(), "a check in flight never covers");
    stage.set_busy(false);
    settle(Duration::from_millis(60));
    assert!(stage.is_awake(), "finishing it restarts the clock");
    settle(Duration::from_millis(300));
    assert!(!stage.is_awake());

    window.destroy();
}

#[test]
#[ignore = "needs a display"]
fn the_cover_wakes_on_a_click_or_a_pointer_that_travels() {
    gtk4::init().expect("display");
    crate::register_resources().expect("resources");

    let stage = LockStage::new();
    let window = gtk4::Window::new();
    window.set_child(Some(&stage));
    let imp = stage.imp();

    imp.moved(0.0, 0.0);
    imp.moved(5.0, 5.0);
    assert!(!stage.is_awake(), "a small movement wakes nothing");
    imp.moved(20.0, 0.0);
    assert!(stage.is_awake(), "a pointer past the threshold wakes it");
    stage.set_awake(false);

    imp.moved(100.0, 100.0);
    assert!(
        !stage.is_awake(),
        "the first position after covering is only the new origin"
    );

    let click = stage
        .observe_controllers()
        .into_iter()
        .filter_map(Result::ok)
        .find_map(|controller| controller.downcast::<gtk4::GestureClick>().ok())
        .expect("the stage carries a click gesture");
    click.emit_by_name::<()>("pressed", &[&1i32, &0.0f64, &0.0f64]);
    assert!(stage.is_awake(), "a press wakes it");
    stage.set_awake(false);

    stage.set_interactive(false);
    imp.moved(0.0, 0.0);
    imp.moved(300.0, 300.0);
    click.emit_by_name::<()>("pressed", &[&1i32, &0.0f64, &0.0f64]);
    assert!(!stage.is_awake(), "a mirror wakes for neither");

    window.destroy();
}

#[test]
#[ignore = "needs a display"]
fn an_error_does_not_outlive_the_cover_but_a_lockout_does() {
    gtk4::init().expect("display");
    crate::register_resources().expect("resources");

    let stage = LockStage::new();
    let window = gtk4::Window::new();
    window.set_child(Some(&stage));
    let shown = || stage.prompt().imp().message.text().to_string();

    stage.set_message(Some("Wrong password"), MessageKind::Error);
    assert_eq!(shown(), "Wrong password");
    stage.set_awake(false);
    assert_eq!(
        shown(),
        "",
        "an ordinary refusal is gone when the cover returns"
    );
    stage.set_message(Some("Wrong password"), MessageKind::Error);
    assert!(
        !stage.is_awake(),
        "the same message again is not a new failure and wakes nothing"
    );

    stage.set_message(Some("(10 minutes left to unlock)"), MessageKind::Lasting);
    assert!(stage.is_awake());
    stage.set_awake(false);
    assert_eq!(
        shown(),
        "(10 minutes left to unlock)",
        "a lockout stays for the next wake"
    );

    stage.set_message(None, MessageKind::Info);
    stage.set_message(Some("Wrong password"), MessageKind::Error);
    stage.prompt().set_available(false);
    stage.set_awake(false);
    assert_eq!(
        shown(),
        "Wrong password",
        "nothing is cleared while the prompt cannot take a password"
    );

    window.destroy();
}

#[test]
#[ignore = "needs a display"]
fn keys_typed_before_the_prompt_is_shown_reach_it() {
    gtk4::init().expect("display");
    crate::register_resources().expect("resources");

    let stage = LockStage::new();
    let window = gtk4::Window::new();
    window.set_child(Some(&stage));
    stage.set_interactive(true);
    let submitted = Rc::new(Cell::new(0));
    stage.prompt().connect_submitted({
        let submitted = submitted.clone();
        move |_| submitted.set(submitted.get() + 1)
    });

    assert!(press(&stage, gdk::Key::a));
    assert!(!stage.prompt().is_mapped(), "the window is never presented");
    std::thread::sleep(std::time::Duration::from_millis(350));
    assert!(
        press(&stage, gdk::Key::b),
        "a key past the settle window is still typed while the prompt is not shown"
    );
    assert!(press(&stage, gdk::Key::Return));
    assert_eq!(
        submitted.get(),
        1,
        "Enter submits even though the hidden entry cannot activate"
    );
    assert_eq!(stage.prompt().take_text().as_str(), "ab");

    window.destroy();
}
