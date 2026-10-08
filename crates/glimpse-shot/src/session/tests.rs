use std::time::Duration;

use glimpse_wayland::capture::Frame;

use super::*;
use crate::doc::units::Scale;

const GUI: Face = Face {
    headless: false,
    notify: false,
};
const HEADLESS: Face = Face {
    headless: true,
    notify: false,
};

fn source() -> Source {
    Source::Output {
        name: "eDP-1".to_owned(),
        scale: Scale(1.25),
        frame: Frame {
            connector: "eDP-1".to_owned(),
            width: 2880,
            height: 1800,
            pixels: glib::Bytes::from_owned(vec![0u8; 16]),
            alpha: false,
            layout: glimpse_wayland::capture::Layout::Rgbx,
        },
    }
}

fn editing(face: Face) -> Phase {
    Phase::Editing {
        saved: None,
        copied: false,
        face,
    }
}

fn drive(phase: Phase, events: Vec<Event>) -> Vec<(Phase, Vec<Effect>)> {
    let mut phase = phase;
    let mut trace = Vec::new();
    for event in events {
        let (next, effects) = step(phase, event);
        trace.push((next.clone(), effects));
        phase = next;
    }
    trace
}

#[test]
fn a_screen_capture_saved_to_a_file_follows_the_machine() {
    let path = PathBuf::from("/run/user/1000/shot.png");
    let saved_as = PathBuf::from("/run/user/1000/shot (2).png");
    let (phase, effects) = start(Target::CurrentScreen, None, HEADLESS);
    assert_eq!(effects, vec![Effect::Capture(Target::CurrentScreen)]);
    let trace = drive(
        phase,
        vec![
            Event::Captured(Ok(source())),
            Event::Finish(FinishKey::SaveAsClose, Some(path.clone())),
            Event::Effected(Effect::Encode, Ok(())),
            Event::Effected(Effect::Write(saved_as.clone()), Ok(())),
        ],
    );

    let expected = vec![
        (editing(HEADLESS), vec![]),
        (
            Phase::Finishing {
                pending: vec![Effect::Encode, Effect::Write(path.clone())],
                done: vec![],
                close: true,
                gone: false,
                face: HEADLESS,
            },
            vec![Effect::Encode],
        ),
        (
            Phase::Finishing {
                pending: vec![Effect::Write(path.clone())],
                done: vec![Effect::Encode],
                close: true,
                gone: false,
                face: HEADLESS,
            },
            vec![Effect::Write(path)],
        ),
        (
            Phase::Done(Outcome::Saved {
                path: saved_as,
                copied: false,
            }),
            vec![],
        ),
    ];
    assert_eq!(trace, expected);
}

#[test]
fn a_delayed_start_captures_only_on_a_tick_at_or_past_the_deadline() {
    let now = Instant::now();
    let until = now + Duration::from_secs(3);
    let (phase, effects) = start(Target::Screen("DP-2".to_owned()), Some(until), GUI);
    assert!(effects.is_empty());

    let (phase, effects) = step(phase, Event::Tick(now + Duration::from_secs(1)));
    assert!(matches!(phase, Phase::Countdown { .. }));
    assert!(effects.is_empty());

    let (phase, effects) = step(phase, Event::Tick(until));
    assert_eq!(phase, Phase::Capturing { face: GUI });
    assert_eq!(
        effects,
        vec![Effect::Capture(Target::Screen("DP-2".to_owned()))]
    );
}

#[test]
fn share_after_a_collision_shares_the_path_actually_written() {
    let asked = PathBuf::from("/p/shot.png");
    let actual = PathBuf::from("/p/shot (2).png");
    let trace = drive(
        editing(GUI),
        vec![
            Event::Finish(FinishKey::Share, Some(asked.clone())),
            Event::Effected(Effect::Encode, Ok(())),
            Event::Effected(Effect::Write(actual.clone()), Ok(())),
            Event::Effected(Effect::Share(actual.clone()), Ok(())),
        ],
    );

    assert_eq!(trace[2].1, vec![Effect::Share(actual.clone())]);
    assert_eq!(trace[3].0, Phase::Done(Outcome::Shared(actual)));
}

#[test]
fn a_gui_failure_returns_to_editing_with_a_toast_and_earlier_results_kept() {
    let first = PathBuf::from("/p/shot.png");
    let editing_after_save = Phase::Editing {
        saved: Some(first.clone()),
        copied: false,
        face: GUI,
    };
    let trace = drive(
        editing_after_save.clone(),
        vec![
            Event::Finish(FinishKey::CopyClose, None),
            Event::Effected(Effect::Encode, Ok(())),
            Event::Effected(Effect::Copy, Err("no clipboard".to_owned())),
        ],
    );

    assert_eq!(
        trace[2],
        (
            editing_after_save,
            vec![Effect::ShowToast("no clipboard".to_owned())]
        )
    );
}

#[test]
fn a_headless_failure_ends_failed_with_its_reason() {
    let (phase, _) = step(
        Phase::Capturing { face: HEADLESS },
        Event::Captured(Err(CaptureError::NoSuchOutput("DP-9".to_owned()))),
    );
    assert_eq!(
        phase,
        Phase::Done(Outcome::Failed("there is no output named DP-9".to_owned()))
    );

    let trace = drive(
        editing(HEADLESS),
        vec![
            Event::Finish(FinishKey::SaveAsClose, Some(PathBuf::from("/p/x.png"))),
            Event::Effected(Effect::Encode, Err("disk full".to_owned())),
        ],
    );
    assert_eq!(
        trace[1].0,
        Phase::Done(Outcome::Failed("disk full".to_owned()))
    );
}

#[test]
fn ctrl_s_stays_open_and_esc_afterwards_ends_saved() {
    let path = PathBuf::from("/p/shot.png");
    let trace = drive(
        editing(GUI),
        vec![
            Event::Finish(FinishKey::Save, Some(path.clone())),
            Event::Effected(Effect::Encode, Ok(())),
            Event::Effected(Effect::Write(path.clone()), Ok(())),
            Event::Esc,
        ],
    );

    assert_eq!(
        trace[2],
        (
            Phase::Editing {
                saved: Some(path.clone()),
                copied: false,
                face: GUI,
            },
            vec![Effect::ShowToast("Saved".to_owned())]
        )
    );
    assert_eq!(
        trace[3].0,
        Phase::Done(Outcome::Saved {
            path,
            copied: false
        })
    );
}

#[test]
fn esc_with_nothing_done_cancels() {
    assert_eq!(
        step(editing(GUI), Event::Esc).0,
        Phase::Done(Outcome::Cancelled)
    );
}

#[test]
fn copy_without_a_file_encodes_then_copies_and_notifies_when_asked() {
    let face = Face {
        headless: false,
        notify: true,
    };
    let trace = drive(
        editing(face),
        vec![
            Event::Finish(FinishKey::CopyClose, None),
            Event::Effected(Effect::Encode, Ok(())),
            Event::Effected(Effect::Copy, Ok(())),
        ],
    );

    assert_eq!(trace[0].1, vec![Effect::Encode]);
    assert_eq!(trace[1].1, vec![Effect::Copy]);
    assert_eq!(trace[2].0, Phase::Done(Outcome::Copied));
    assert!(matches!(trace[2].1.as_slice(), [Effect::Notify(_)]));
}

fn finishing_after_encode(face: Face) -> Phase {
    let path = PathBuf::from("/p/shot.png");
    let trace = drive(
        editing(face),
        vec![
            Event::Finish(FinishKey::SaveAsClose, Some(path)),
            Event::Effected(Effect::Encode, Ok(())),
        ],
    );
    trace[1].0.clone()
}

#[test]
fn a_duplicate_effected_is_ignored() {
    let phase = finishing_after_encode(HEADLESS);
    assert_eq!(
        step(phase.clone(), Event::Effected(Effect::Encode, Ok(()))),
        (phase, vec![])
    );
}

#[test]
fn a_stray_effected_ok_is_ignored() {
    let phase = finishing_after_encode(HEADLESS);
    assert_eq!(
        step(phase.clone(), Event::Effected(Effect::Copy, Ok(()))),
        (phase, vec![])
    );
}

#[test]
fn a_stray_effected_err_is_ignored() {
    let phase = finishing_after_encode(GUI);
    assert_eq!(
        step(
            phase.clone(),
            Event::Effected(Effect::Copy, Err("late".to_owned()))
        ),
        (phase, vec![])
    );
}

#[test]
fn an_output_lost_while_finishing_opens_a_window_on_the_return_to_editing() {
    let path = PathBuf::from("/p/shot.png");
    let trace = drive(
        editing(GUI),
        vec![
            Event::Finish(FinishKey::Save, Some(path.clone())),
            Event::OutputGone("eDP-1".to_owned()),
            Event::Effected(Effect::Encode, Ok(())),
            Event::Effected(Effect::Write(path.clone()), Ok(())),
        ],
    );
    assert!(trace[1].1.is_empty());
    assert!(matches!(trace[1].0, Phase::Finishing { .. }));
    assert_eq!(
        trace[3],
        (
            Phase::Editing {
                saved: Some(path.clone()),
                copied: false,
                face: GUI,
            },
            vec![Effect::ShowToast("Saved".to_owned()), Effect::OpenInWindow]
        )
    );

    let failed = drive(
        editing(GUI),
        vec![
            Event::Finish(FinishKey::Save, Some(path.clone())),
            Event::OutputGone("eDP-1".to_owned()),
            Event::Effected(Effect::Encode, Err("broken".to_owned())),
        ],
    );
    assert_eq!(
        failed[2].1,
        vec![Effect::ShowToast("broken".to_owned()), Effect::OpenInWindow]
    );

    let closing = drive(
        editing(GUI),
        vec![
            Event::Finish(FinishKey::SaveAsClose, Some(path.clone())),
            Event::OutputGone("eDP-1".to_owned()),
            Event::Effected(Effect::Encode, Ok(())),
            Event::Effected(Effect::Write(path.clone()), Ok(())),
        ],
    );
    assert_eq!(
        closing[3],
        (
            Phase::Done(Outcome::Saved {
                path,
                copied: false
            }),
            vec![]
        )
    );
}

#[test]
fn a_headless_window_copy_captures_encodes_and_copies_then_ends_copied() {
    let (phase, first) = start(Target::Window(42), None, HEADLESS);
    let trace = drive(
        phase,
        vec![
            Event::Captured(Ok(source())),
            Event::Finish(FinishKey::CopyClose, None),
            Event::Effected(Effect::Encode, Ok(())),
            Event::Effected(Effect::Copy, Ok(())),
        ],
    );

    let emitted: Vec<Effect> = first
        .into_iter()
        .chain(trace.iter().flat_map(|(_, effects)| effects.clone()))
        .collect();
    assert_eq!(
        emitted,
        vec![
            Effect::Capture(Target::Window(42)),
            Effect::Encode,
            Effect::Copy
        ]
    );
    assert_eq!(trace[3].0, Phase::Done(Outcome::Copied));
}

#[test]
fn a_countdown_ends_on_esc_or_timeout() {
    let (phase, _) = start(
        Target::CurrentScreen,
        Some(Instant::now() + Duration::from_secs(3)),
        GUI,
    );
    assert_eq!(
        step(phase.clone(), Event::Esc).0,
        Phase::Done(Outcome::Cancelled)
    );
    assert_eq!(
        step(phase, Event::Timeout).0,
        Phase::Done(Outcome::TimedOut)
    );
}

#[test]
fn a_timeout_while_editing_ends_with_the_results_so_far() {
    assert_eq!(
        step(editing(GUI), Event::Timeout).0,
        Phase::Done(Outcome::TimedOut)
    );
    let path = PathBuf::from("/p/shot.png");
    let saved = Phase::Editing {
        saved: Some(path.clone()),
        copied: true,
        face: GUI,
    };
    assert_eq!(
        step(saved, Event::Timeout).0,
        Phase::Done(Outcome::Saved { path, copied: true })
    );
}

#[test]
fn ctrl_c_stays_open_with_a_toast_and_esc_afterwards_ends_copied() {
    let trace = drive(
        editing(GUI),
        vec![
            Event::Finish(FinishKey::Copy, None),
            Event::Effected(Effect::Encode, Ok(())),
            Event::Effected(Effect::Copy, Ok(())),
            Event::Esc,
        ],
    );

    assert_eq!(
        trace[2],
        (
            Phase::Editing {
                saved: None,
                copied: true,
                face: GUI,
            },
            vec![Effect::ShowToast("Copied".to_owned())]
        )
    );
    assert_eq!(trace[3].0, Phase::Done(Outcome::Copied));
}

#[test]
fn open_in_window_keeps_editing_and_asks_for_the_window() {
    assert_eq!(
        step(editing(GUI), Event::Finish(FinishKey::OpenInWindow, None)),
        (editing(GUI), vec![Effect::OpenInWindow])
    );
}

#[test]
fn save_or_share_with_nowhere_to_go_toasts_or_fails() {
    for key in [FinishKey::Save, FinishKey::SaveAsClose, FinishKey::Share] {
        let (phase, effects) = step(editing(GUI), Event::Finish(key, None));
        assert_eq!(phase, editing(GUI), "{key:?}");
        assert!(
            matches!(effects.as_slice(), [Effect::ShowToast(_)]),
            "{key:?}"
        );

        let (phase, effects) = step(editing(HEADLESS), Event::Finish(key, None));
        assert!(matches!(phase, Phase::Done(Outcome::Failed(_))), "{key:?}");
        assert!(effects.is_empty());
    }
}

#[test]
fn copy_with_a_destination_encodes_writes_then_copies() {
    let path = PathBuf::from("/p/shot.png");
    let trace = drive(
        editing(HEADLESS),
        vec![
            Event::Finish(FinishKey::CopyClose, Some(path.clone())),
            Event::Effected(Effect::Encode, Ok(())),
            Event::Effected(Effect::Write(path.clone()), Ok(())),
            Event::Effected(Effect::Copy, Ok(())),
        ],
    );

    let emitted: Vec<Effect> = trace
        .iter()
        .flat_map(|(_, effects)| effects.clone())
        .collect();
    assert_eq!(
        emitted,
        vec![Effect::Encode, Effect::Write(path.clone()), Effect::Copy]
    );
    assert_eq!(
        trace[3].0,
        Phase::Done(Outcome::Saved { path, copied: true })
    );
}

#[test]
fn a_notice_names_the_saved_file_or_says_copied() {
    let path = PathBuf::from("/p/shot.png");
    assert_eq!(
        notice(&Outcome::Saved {
            path: path.clone(),
            copied: true
        }),
        Notice {
            summary: "Screenshot saved".to_owned(),
            body: "shot.png".to_owned(),
            image: Some(path.clone()),
            actions: true,
        }
    );
    assert_eq!(notice(&Outcome::Shared(path.clone())).image, Some(path));
    assert_eq!(
        notice(&Outcome::Copied),
        Notice {
            summary: "Screenshot copied".to_owned(),
            body: String::new(),
            image: None,
            actions: false,
        }
    );
}

#[test]
fn a_lost_output_hands_the_document_to_a_window_except_headless() {
    let (phase, effects) = step(editing(GUI), Event::OutputGone("eDP-1".to_owned()));
    assert_eq!(phase, editing(GUI));
    assert_eq!(effects, vec![Effect::OpenInWindow]);

    let (_, effects) = step(editing(HEADLESS), Event::OutputGone("eDP-1".to_owned()));
    assert!(effects.is_empty());
}
