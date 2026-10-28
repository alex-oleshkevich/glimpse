use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::{gdk, glib};

use zeroize::Zeroizing;

use super::{
    FOCUSED, Look, Surfaces, Timer, accepts, carried_awake, next_minute, prompt_output,
    prompt_stage, session_result_target, submitted,
};
use crate::lifecycle::Input;
use crate::status::Status;

#[test]
fn the_prompt_goes_to_the_focused_output_else_the_first() {
    let connectors = ["eDP-1".to_owned(), "DP-2".to_owned()];
    assert_eq!(
        prompt_stage(&connectors, Some("DP-2"), None, None, true),
        Some(1)
    );
    assert_eq!(
        prompt_stage(&connectors, Some("HDMI-A-1"), None, None, true),
        Some(0)
    );
    assert_eq!(prompt_stage(&connectors, None, None, None, true), Some(0));
    assert_eq!(prompt_stage(&[], Some("DP-2"), None, None, true), None);
}

#[test]
fn only_the_prompt_that_submitted_and_is_still_interactive_reaches_pam() {
    let (edp, dp) = ("eDP-1", "DP-2");
    assert!(accepts(Some(&edp), Some(&edp)));
    assert!(
        !accepts(Some(&edp), Some(&dp)),
        "interactivity moved between the submit and the attempt"
    );
    assert!(
        !accepts(Some(&edp), None),
        "the submitting output left and nothing replaced it"
    );
    assert!(
        !accepts(None, Some(&edp)),
        "no prompt submitted this attempt"
    );
    assert!(!accepts::<&str>(None, None));

    assert_eq!(
        submitted(Zeroizing::new(String::new())),
        None,
        "an entry the move cleared never reaches PAM as an empty password"
    );
    assert_eq!(
        submitted(Zeroizing::new("hunter2".to_owned()))
            .as_deref()
            .map(|text| text.as_str()),
        Some("hunter2")
    );
}

#[test]
fn the_clock_timer_is_only_ever_armed_once() {
    let context = glib::MainContext::default();
    let _guard = context
        .acquire()
        .expect("the default context is free in this test");
    let fired = Rc::new(Cell::new(0));
    let timer = Timer::default();
    let count = |fired: &Rc<Cell<u32>>| {
        let fired = fired.clone();
        move || fired.set(fired.get() + 1)
    };

    timer.arm(Duration::ZERO, count(&fired));
    timer.arm(Duration::ZERO, count(&fired));
    while context.pending() {
        context.iteration(false);
    }
    assert_eq!(fired.get(), 1, "re-arming replaces the pending timer");

    timer.fired();
    timer.arm(Duration::ZERO, count(&fired));
    timer.cancel();
    while context.pending() {
        context.iteration(false);
    }
    assert_eq!(fired.get(), 1, "a cancelled timer never fires");
}

#[test]
fn focus_moves_the_prompt_only_onto_an_output_with_a_stage() {
    let stages = ["eDP-1".to_owned(), "DP-2".to_owned()];
    assert_eq!(
        prompt_stage(&stages, Some("DP-2"), None, Some(0), false),
        Some(1)
    );
    assert_eq!(
        prompt_stage(&stages, Some("DP-2"), None, Some(1), false),
        None
    );
    assert_eq!(
        prompt_stage(&stages, Some("HDMI-A-1"), None, Some(1), false),
        None,
        "a focused output with no stage yet leaves the prompt where it is"
    );
    assert_eq!(
        prompt_stage(&stages, Some("HDMI-A-1"), None, None, false),
        None
    );
    assert_eq!(prompt_stage(&stages, None, None, Some(1), true), None);
}

#[test]
fn a_connector_pins_the_prompt_and_focused_follows_the_compositor() {
    assert_eq!(prompt_output(FOCUSED, Some("DP-2")), Some("DP-2"));
    assert_eq!(prompt_output(FOCUSED, None), None);
    assert_eq!(prompt_output("eDP-1", Some("DP-2")), Some("eDP-1"));
    assert_eq!(prompt_output("eDP-1", None), Some("eDP-1"));

    let stages = ["eDP-1".to_owned(), "DP-2".to_owned()];
    let pinned = prompt_output("eDP-1", Some("DP-2"));
    assert_eq!(
        prompt_stage(&stages, pinned, None, Some(0), false),
        None,
        "focus moving elsewhere does not move a pinned prompt"
    );
    assert_eq!(
        prompt_stage(
            &stages,
            prompt_output("HDMI-A-1", Some("DP-2")),
            None,
            None,
            true
        ),
        Some(0),
        "a pinned connector that is absent falls back to the first"
    );
}

#[test]
fn the_window_holding_keyboard_focus_takes_the_prompt() {
    let stages = ["eDP-1".to_owned(), "DP-2".to_owned()];
    assert_eq!(
        prompt_stage(&stages, Some("eDP-1"), Some(1), Some(0), false),
        Some(1),
        "an active window wins over a pinned or compositor-focused output"
    );
    assert_eq!(
        prompt_stage(&stages, Some("eDP-1"), Some(1), None, true),
        Some(1),
        "an active window wins over the fallback too"
    );
    assert_eq!(
        prompt_stage(&stages, Some("eDP-1"), Some(1), Some(1), false),
        None,
        "the active window's stage is already interactive"
    );
    assert_eq!(
        prompt_stage(&stages, Some("DP-2"), None, Some(0), false),
        Some(1),
        "with no active window yet, the configured output decides"
    );
    assert_eq!(
        prompt_stage(&stages, Some("eDP-1"), Some(5), Some(1), false),
        Some(0),
        "an active index with no stage behind it is ignored"
    );
}

#[test]
fn a_session_result_falls_back_to_the_interactive_stage_when_its_own_is_gone() {
    let connectors = ["eDP-1".to_owned(), "DP-2".to_owned()];
    assert_eq!(
        session_result_target(Some("DP-2"), Some("eDP-1"), &connectors),
        Some("DP-2"),
        "the requesting stage still exists"
    );
    assert_eq!(
        session_result_target(Some("HDMI-A-1"), Some("eDP-1"), &connectors),
        Some("eDP-1"),
        "the requesting stage is gone; falls back to the interactive one"
    );
    assert_eq!(
        session_result_target(Some("HDMI-A-1"), Some("HDMI-A-2"), &connectors),
        None,
        "both the requester and the interactive stage are gone"
    );
    assert_eq!(
        session_result_target(None, Some("eDP-1"), &connectors),
        Some("eDP-1"),
        "no requester monitor at all falls back to the interactive one"
    );
    assert_eq!(session_result_target(None, None, &connectors), None);
}

#[test]
fn the_clock_wakes_on_the_next_minute_boundary() {
    assert_eq!(next_minute(0.0), Duration::from_secs(60));
    assert_eq!(next_minute(30.25), Duration::from_millis(29_750));
    assert_eq!(next_minute(59.9995), Duration::from_millis(1));
    assert_eq!(next_minute(59.0), Duration::from_secs(1));
    assert_eq!(
        next_minute(30.0001),
        Duration::from_secs(30),
        "a fraction of a millisecond rounds up, so the tick never lands before :00"
    );
    assert_eq!(next_minute(61.0), Duration::from_secs(59));
    assert_eq!(next_minute(f64::NAN), Duration::from_secs(60));
    assert_eq!(next_minute(f64::INFINITY), Duration::from_secs(60));
}

#[test]
fn an_action_or_a_wake_is_carried_only_from_the_stage_that_was_interactive() {
    let (edp, dp) = ("eDP-1".to_owned(), "DP-2".to_owned());
    assert!(
        accepts(Some(&edp), Some(&edp)),
        "the interactive stage acts"
    );
    assert!(
        !accepts(Some(&dp), Some(&edp)),
        "a mirror's session or transport action is ignored"
    );

    assert!(
        carried_awake(Some(true)),
        "the prompt moves while awake and stays awake"
    );
    assert!(
        !carried_awake(Some(false)),
        "and while covered stays covered"
    );
    assert!(
        !carried_awake(None),
        "a stage with no predecessor starts on the cover"
    );
}

fn reported(reports: &RefCell<Vec<Input>>, wanted: impl Fn(&Input) -> bool) -> bool {
    reports.borrow().iter().any(wanted)
}

fn pump(limit: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + limit;
    let context = glib::MainContext::default();
    while !done() && Instant::now() < deadline {
        if !context.iteration(false) {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

#[test]
#[ignore = "locks the session it runs in: run alone in the test VM, never on a desktop"]
fn a_lock_window_reports_painted_once_it_draws() {
    gtk4::init().expect("display");
    glimpse_widgets::register_resources().expect("resources");
    let look = Look {
        backgrounds: HashMap::new(),
        color: gdk::RGBA::BLACK,
        fit: gtk4::ContentFit::Cover,
        dim: 0.0,
        clock: None,
        user: None,
        avatar: None,
        prompt_output: FOCUSED.to_owned(),
        session: HashMap::new(),
        track: None,
        notifications: Vec::new(),
        status: Status::default(),
    };
    let reports: Rc<RefCell<Vec<Input>>> = Rc::default();
    let surfaces = Surfaces::lock(
        1,
        look,
        None,
        Rc::default(),
        {
            let reports = reports.clone();
            move |input| reports.borrow_mut().push(input)
        },
        |_, _| {},
        |_| {},
    );
    if std::env::var_os("GLIMPSE_LOCK_TEST_REFUSAL").is_some() {
        pump(Duration::from_secs(5), || {
            reported(&reports, |input| matches!(input, Input::Failed(1)))
        });
        let failed = reported(&reports, |input| matches!(input, Input::Failed(1)));
        let assigned = !surfaces.shared.stages.borrow().is_empty();
        surfaces.unlock();
        assert!(
            failed,
            "the second lock must be refused by the test compositor"
        );
        assert!(!assigned, "a refused lock must never assign a window");
        return;
    }
    let locked = |input: &Input| matches!(input, Input::Locked(1));
    let painted = |input: &Input| matches!(input, Input::Painted(1));
    pump(Duration::from_secs(10), || {
        reported(&reports, locked) && reported(&reports, painted)
    });
    let was_painted = reported(&reports, painted);
    let was_locked = reported(&reports, locked);
    let mut child_result = None;
    let mut duplicate_refused = false;
    if was_locked && was_painted {
        let duplicate = gtk4_session_lock::Instance::new();
        let rejected = Rc::new(Cell::new(false));
        duplicate.connect_failed({
            let rejected = rejected.clone();
            move |_| rejected.set(true)
        });
        duplicate_refused = !duplicate.lock() && rejected.get();
        let child = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--include-ignored",
                "--exact",
                "surface::tests::a_lock_window_reports_painted_once_it_draws",
            ])
            .env("GLIMPSE_LOCK_TEST_REFUSAL", "1")
            .spawn();
        child_result = Some(match child {
            Ok(mut child) => {
                let mut status = None;
                pump(Duration::from_secs(10), || {
                    status = child.try_wait().ok().flatten();
                    status.is_some()
                });
                if status.is_none() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                status.is_some_and(|status| status.success())
            }
            Err(_) => false,
        });
    }
    surfaces.unlock();
    pump(Duration::from_secs(5), || {
        reported(&reports, |input| matches!(input, Input::Unlocked(1)))
    });
    assert!(was_locked, "the compositor never locked");
    assert!(
        duplicate_refused,
        "Instance::lock false must report failure"
    );
    assert_eq!(
        child_result,
        Some(true),
        "a refused lock crashed or assigned a window"
    );
    assert!(was_painted, "the lock window drew without reporting it");
}
