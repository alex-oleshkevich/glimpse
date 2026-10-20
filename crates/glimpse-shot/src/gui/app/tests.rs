use std::path::PathBuf;

use super::*;

#[test]
#[ignore = "needs a display"]
fn a_file_timeout_without_json_keeps_the_reply_until_it_exits_seven() {
    gtk4::init().expect("display");
    adw::init().expect("libadwaita");
    glimpse_widgets::register_resources().expect("resources");
    let app = adw::Application::builder()
        .application_id("me.aresa.Glimpse.Shot.TimeoutTest")
        .build();
    app.register(gio::Cancellable::NONE).expect("register");
    let scratch = tempfile::tempdir().expect("scratch");
    let config = scratch.path().join("config.toml");
    std::fs::write(&config, "").expect("config");
    let shell = Shell::new(&app, Some(&config));
    let answered = Rc::new(RefCell::new(None));
    let answer = Rc::clone(&answered);
    shell.open_file(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/every_mark.png"),
        Some(Box::new(move |result| {
            answer.replace(Some(reply(result, false)));
        })),
        Some(&config),
        &GuiArgs {
            timeout: Some(0),
            ..Default::default()
        },
        &[],
    );
    assert!(answered.borrow().is_none(), "waits for the timeout");
    let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while answered.borrow().is_none() && std::time::Instant::now() < until {
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(answered.take().expect("timeout reply").status, 7);
}

fn finished(outcome: Outcome, json: bool) -> Reply {
    let line = json.then(|| "{\"version\":1}".to_owned());
    reply(
        settled(Ended::Finished {
            outcome,
            json: line,
        }),
        json,
    )
}

fn last_error(reply: &Reply) -> serde_json::Value {
    serde_json::from_str(reply.stderr.lines().last().expect("a line")).expect("json")
}

#[test]
fn every_outcome_becomes_the_callers_stdout_stderr_and_status() {
    let saved = Outcome::Saved {
        path: PathBuf::from("/tmp/x.png"),
        copied: true,
    };
    for success in [
        saved,
        Outcome::Copied,
        Outcome::Shared(PathBuf::from("/x.png")),
    ] {
        let plain = finished(success.clone(), false);
        assert_eq!(
            plain,
            Reply {
                stdout: String::new(),
                stderr: String::new(),
                status: 0,
            }
        );
        let json = finished(success, true);
        assert_eq!(json.stdout, "{\"version\":1}\n");
        assert_eq!((json.stderr.as_str(), json.status), ("", 0));
    }

    for (outcome, status, kind) in [
        (Outcome::Cancelled, 4, "cancelled"),
        (Outcome::TimedOut, 7, "timed_out"),
        (Outcome::Failed("disk full".to_owned()), 1, "failed"),
    ] {
        let plain = finished(outcome.clone(), false);
        assert_eq!((plain.stdout.as_str(), plain.status), ("", status));
        assert!(
            plain.stderr.starts_with("glimpse-shot: error: "),
            "{plain:?}"
        );
        assert!(!plain.stderr.contains('{'), "{plain:?}");

        let json = finished(outcome, true);
        assert_eq!((json.stdout.as_str(), json.status), ("", status));
        let error = last_error(&json);
        assert_eq!(error["error"]["code"], status);
        assert_eq!(error["error"]["kind"], kind);
    }
}

#[test]
fn a_typed_failure_keeps_its_own_code() {
    let gone = reply(
        settled(Ended::Failed(
            errors::NotFound {
                what: "no window 7".to_owned(),
                hint: None,
            }
            .into(),
        )),
        true,
    );
    assert_eq!((gone.stdout.as_str(), gone.status), ("", 5));
    assert_eq!(last_error(&gone)["error"]["kind"], "target_not_found");
}

#[test]
#[ignore = "needs a display"]
fn a_hand_off_that_never_settles_is_cancelled() {
    gtk4::init().expect("display");
    use glimpse_config::{PaletteColor, Stroke};
    use glimpse_shot::doc::document::{Document, Source};
    use glimpse_wayland::capture::Frame;

    let doc = Document {
        source: Source::File {
            path: PathBuf::from("/x.png"),
            frame: Frame {
                connector: String::new(),
                width: 1,
                height: 1,
                pixels: glib::Bytes::from_owned(vec![0u8; 4]),
                alpha: true,
                layout: glimpse_wayland::capture::Layout::Rgbx,
            },
        },
        crop: None,
        marks: Vec::new(),
        selected: None,
    };
    let handoff = Handoff {
        editor: canvas::Editor::new(
            doc,
            Style {
                color: PaletteColor::Red,
                stroke: Stroke::Medium,
            },
        ),
        target: None,
        taken: None,
    };
    let handed = reply(settled(Ended::HandedOff(Box::new(handoff))), true);
    assert_eq!((handed.stdout.as_str(), handed.status), ("", 4));
    assert_eq!(last_error(&handed)["error"]["kind"], "cancelled");
}

#[test]
fn a_second_invocation_while_an_overlay_is_up_is_busy() {
    let busy = reply(Err(Busy.into()), true);
    assert_eq!((busy.stdout.as_str(), busy.status), ("", 1));
    assert_eq!(last_error(&busy)["error"]["kind"], "busy");

    let usage = reply(Err(Usage("bad".to_owned()).into()), false);
    assert_eq!(usage.status, 2);
}

#[test]
fn portal_target_rejects_the_window_picker() {
    assert!(portal_target_allowed(&Target::CurrentScreen));
    assert!(portal_target_allowed(&Target::Screen("eDP-1".to_owned())));
    assert!(!portal_target_allowed(&Target::PickWindow));
    assert!(!portal_target_allowed(&Target::Window(7)));
    assert!(!portal_target_allowed(&Target::App));
}

#[test]
fn a_forwarded_config_is_the_callers_and_anchored_at_the_callers_directory() {
    let cwd = Some(Path::new("/home/me"));
    let explicit = Some(PathBuf::from("dev.toml"));
    let environment = Some(PathBuf::from("/etc/env.toml"));
    assert_eq!(
        caller_config(explicit.clone(), environment.clone(), cwd),
        Some(PathBuf::from("/home/me/dev.toml"))
    );
    assert_eq!(
        caller_config(None, environment, cwd),
        Some(PathBuf::from("/etc/env.toml"))
    );
    assert_eq!(
        caller_config(None, Some(PathBuf::from("var/c.toml")), cwd),
        Some(PathBuf::from("/home/me/var/c.toml"))
    );
    assert_eq!(
        caller_config(explicit, None, None),
        Some(PathBuf::from("dev.toml"))
    );
    assert_eq!(caller_config(None, None, cwd), None);
}

#[test]
fn a_forwarded_relative_path_is_anchored_at_the_callers_directory() {
    let gui = GuiArgs {
        output: Some(PathBuf::from("shot.png")),
        annotate: Some(PathBuf::from("-")),
        file: Some(PathBuf::from("/abs/in.png")),
        ..GuiArgs::default()
    };
    let anchored = anchored(gui, Some(Path::new("/home/me")));
    assert_eq!(anchored.output, Some(PathBuf::from("/home/me/shot.png")));
    assert_eq!(anchored.annotate, Some(PathBuf::from("-")));
    assert_eq!(anchored.file, Some(PathBuf::from("/abs/in.png")));
    let gui = GuiArgs {
        file: Some(PathBuf::from("input.png")),
        annotate: Some(PathBuf::from("marks.json")),
        ..Default::default()
    };
    let anchored = super::anchored(gui, Some(Path::new("/other/caller")));
    assert_eq!(
        anchored.file,
        Some(PathBuf::from("/other/caller/input.png"))
    );
    assert_eq!(
        anchored.annotate,
        Some(PathBuf::from("/other/caller/marks.json"))
    );
    let dash = super::anchored(
        GuiArgs {
            file: Some("-".into()),
            ..Default::default()
        },
        Some(Path::new("/other/caller")),
    );
    assert_eq!(dash.file, Some(PathBuf::from("/other/caller/-")));
}
