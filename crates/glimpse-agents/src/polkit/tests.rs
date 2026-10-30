use std::collections::{HashMap, HashSet};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::oneshot::{self, error::TryRecvError};
use zbus::zvariant::{OwnedValue, Value};
use zeroize::Zeroizing;

use super::agent::{self, PATH};
use super::helper::{self, Line};
use super::{
    Conversation, Driver, Identity, Outcome, Request, ResponseEffect, on_dialog, on_helper_line,
    request, resolve_identities,
};
use crate::app::view_for;
use crate::ask::{Ask, Input, Prompt, Reply, Source};
use crate::dialog::{Field, Icon, Note, Response, selected_account};
use crate::queue::Queue;

#[tokio::test]
async fn the_command_is_the_first_detail_from_the_subject_or_caller() {
    let bytes = tokio::fs::read(format!("/proc/{}/cmdline", std::process::id()))
        .await
        .expect("process command line");
    let expected = glimpse_utils::clean(
        &String::from_utf8_lossy(&bytes)
            .trim_end_matches('\0')
            .split('\0')
            .collect::<Vec<_>>()
            .join(" "),
        2000,
    );
    for key in ["polkit.subject-pid", "polkit.caller-pid"] {
        let mut details = HashMap::from([
            (key.to_owned(), std::process::id().to_string()),
            ("A detail".to_owned(), "value".to_owned()),
        ]);
        if key == "polkit.subject-pid" {
            details.insert("polkit.caller-pid".to_owned(), u32::MAX.to_string());
        }
        let request = request(
            "action",
            "message",
            details,
            "cookie".to_owned(),
            Vec::new(),
        )
        .await;
        assert_eq!(request.details[0], ("Command".to_owned(), expected.clone()));
        let view = view_for(&Prompt::Polkit(request), None);
        assert_eq!(view.details[0], ("Command".to_owned(), expected.clone()));
    }
}

#[tokio::test]
async fn an_unreadable_or_invalid_pid_has_no_command_detail() {
    for pid in [u32::MAX.to_string(), "../self".to_owned()] {
        let details = HashMap::from([("polkit.subject-pid".to_owned(), pid)]);
        let request = request(
            "action",
            "message",
            details,
            "cookie".to_owned(),
            Vec::new(),
        )
        .await;
        assert!(request.details.is_empty());
    }
}

#[test]
fn command_lines_are_joined_cleaned_and_capped() {
    let hostile = b"/usr/bin/tool\0<b>\n\t\x1bvalue\0\0";
    assert_eq!(super::command_line(hostile), "/usr/bin/tool <b> value");
    let mut bytes = hostile.to_vec();
    bytes.extend_from_slice("ж".repeat(3000).as_bytes());
    let command = super::command_line(&bytes);
    assert!(command.starts_with("/usr/bin/tool <b> value "));
    assert_eq!(command.chars().count(), 2001);
    assert!(command.ends_with('…'));
    assert!(!command.chars().any(char::is_control));
}

fn pkexec_short(command: &str) -> String {
    match command.len() > 80 {
        true => format!("{} ... {}", &command[..38], &command[command.len() - 37..]),
        false => command.to_owned(),
    }
}

fn pkexec_message(command: &str) -> String {
    format!(
        "Authentication is needed to run `{}' as the super user",
        pkexec_short(command)
    )
}

fn argv(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn long_argument() -> String {
    format!("X=/very/long/{}", "segment/".repeat(12))
}

fn full_command() -> String {
    format!("/usr/bin/env {} --flag /usr/bin/true", long_argument())
}

#[test]
fn a_shortened_command_in_the_message_is_replaced_by_the_full_one() {
    let command = full_command();
    let message = pkexec_message(&command);
    assert!(message.contains(" ... "));
    let argument = long_argument();
    let argv = argv(&[
        "pkexec",
        "/usr/bin/env",
        &argument,
        "--flag",
        "/usr/bin/true",
    ]);
    let expanded = super::expand_short_command(&message, &argv);
    assert_eq!(
        expanded,
        format!("Authentication is needed to run `{command}' as the super user")
    );
}

#[test]
fn pkexec_options_are_skipped() {
    let command = full_command();
    let message = pkexec_message(&command);
    let argument = long_argument();
    let expected = format!("Authentication is needed to run `{command}' as the super user");
    for options in [
        vec!["--user", "root"],
        vec!["-u", "root"],
        vec!["--user=root"],
        vec!["--keep-cwd"],
        vec!["--disable-internal-agent", "--keep-cwd", "--"],
    ] {
        let mut items = vec!["/usr/bin/pkexec"];
        items.extend(options.iter().copied());
        items.extend(["/usr/bin/env", &argument, "--flag", "/usr/bin/true"]);
        assert_eq!(
            super::expand_short_command(&message, &argv(&items)),
            expected
        );
    }
}

#[test]
fn a_relative_program_takes_the_resolved_path_from_the_message() {
    let command = full_command();
    let message = pkexec_message(&command);
    let argument = long_argument();
    let argv = argv(&["pkexec", "env", &argument, "--flag", "/usr/bin/true"]);
    assert_eq!(
        super::expand_short_command(&message, &argv),
        format!("Authentication is needed to run `{command}' as the super user")
    );
}

#[test]
fn a_mismatch_leaves_the_message_unchanged() {
    let command = full_command();
    let message = pkexec_message(&command);
    let argument = long_argument();
    let other = argv(&["sudo", "/usr/bin/env", &argument, "--flag", "/usr/bin/true"]);
    assert_eq!(super::expand_short_command(&message, &other), message);
    let suffix = argv(&[
        "pkexec",
        "/usr/bin/env",
        &argument,
        "--flag",
        "/usr/bin/false",
    ]);
    assert_eq!(super::expand_short_command(&message, &suffix), message);
    let bare = argv(&["pkexec"]);
    assert_eq!(super::expand_short_command(&message, &bare), message);
    let short = "Authentication is needed to run `/usr/bin/true' as the super user";
    let true_argv = argv(&["pkexec", "/usr/bin/true"]);
    assert_eq!(super::expand_short_command(short, &true_argv), short);
}

#[test]
fn a_hostile_argument_is_cleaned_after_the_expansion() {
    let command = format!(
        "/usr/bin/env <b>\u{1b}[31m\u{202e}evil\n{}",
        "y".repeat(100)
    );
    let message = pkexec_message(&command);
    let argv = argv(&[
        "pkexec",
        "/usr/bin/env",
        &format!("<b>\u{1b}[31m\u{202e}evil\n{}", "y".repeat(100)),
    ]);
    let expanded = super::expand_short_command(&message, &argv);
    let cleaned = glimpse_utils::clean(&expanded, crate::dialog::PROMPT_BODY_MAX);
    assert!(cleaned.contains("<b>"));
    assert!(!cleaned.chars().any(char::is_control));
    assert!(!cleaned.contains('\u{202e}'));
    assert!(!cleaned.contains(" ... "));
    assert!(cleaned.contains(&"y".repeat(100)));
}

#[tokio::test]
async fn a_request_fills_the_dialog_from_the_action() {
    let mut details = HashMap::new();
    details.insert("polkit.subject-pid".to_owned(), u32::MAX.to_string());
    details.insert("user".to_owned(), "ada".to_owned());
    details.insert("program".to_owned(), "/usr/bin/fwupdmgr".to_owned());
    details.insert("command_line".to_owned(), "fwupdmgr update".to_owned());
    let request = request(
        "org.freedesktop.fwupd.update-internal",
        "Authentication is required to update the system firmware",
        details,
        "secret-cookie".to_owned(),
        vec![admin("ada", "Ada")],
    )
    .await;
    assert_eq!(
        request.details,
        vec![
            ("command_line".to_owned(), "fwupdmgr update".to_owned()),
            ("program".to_owned(), "/usr/bin/fwupdmgr".to_owned()),
            ("user".to_owned(), "ada".to_owned()),
        ]
    );
    let rendered = format!("{request:?}");
    assert!(rendered.contains("<redacted>"));
    assert!(!rendered.contains("secret-cookie"));

    let conversation = Conversation {
        account: 0,
        prompt: Some(("Password:".to_owned(), false)),
        note: None,
        busy: false,
        attempt: None,
        responder: None,
    };
    let view = view_for(&Prompt::Polkit(request), Some(&conversation));
    assert_eq!(view.icon, Icon::Lock);
    assert_eq!(view.title, "Authentication needed");
    assert_eq!(
        view.body,
        "Authentication is required to update the system firmware"
    );
    assert_eq!(view.secret, Some(("Password".to_owned(), Field::Password)));
    assert!(view.details.iter().any(|(key, value)| {
        key == "Action" && value == "org.freedesktop.fwupd.update-internal"
    }));
    assert!(view.details.iter().any(|(key, _)| key == "command_line"));
    assert!(
        view.details
            .iter()
            .all(|(key, _)| !key.starts_with("polkit."))
    );
    assert_eq!(view.confirm.as_deref(), Some("Authorize"));
    assert!(!view.busy);
    assert!(!Field::Password.accepts(""));
    assert!(Field::Password.accepts("x"));

    let blueprint = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/blueprints/agent_dialog.blp"
    ))
    .expect("blueprint");
    let details = blueprint
        .split("Adw.ExpanderRow details")
        .nth(1)
        .expect("details row");
    let block = details.split('}').next().expect("details block");
    assert!(!block.contains("expanded:"));
}

#[tokio::test]
async fn a_successful_attempt_answers_polkit_and_closes() {
    let scratch = Scratch::new();
    let socket = scratch.path().join("helper.sock");
    let listener = UnixListener::bind(&socket).expect("socket");
    let mut desk = Desk::socket(socket);
    let (first, mut first_reply) = queued("cookie-a", vec![admin("ada", "Ada")]);
    let (second, mut second_reply) = queued("cookie-b", vec![admin("ada", "Ada")]);
    desk.queue.push(first);
    desk.queue.push(second);
    desk.start();

    let mut pipe = accept(&listener).await;
    assert_eq!(pipe.line().await, "ada");
    assert_eq!(pipe.line().await, "cookie-a");
    pipe.send("PAM_PROMPT_ECHO_OFF Password:").await;
    desk.wait_prompt().await;
    desk.input
        .send(Input::Line {
            key: helper::tagged_key("cookie-a", 99),
            line: Line::Success,
        })
        .expect("stale line");
    desk.pump().await;
    assert!(matches!(first_reply.try_recv(), Err(TryRecvError::Empty)));

    desk.confirm(0, "secret");
    assert_eq!(pipe.line().await, "secret");
    pipe.send("SUCCESS").await;
    assert_eq!(
        desk.wait_outcome(&mut first_reply).await,
        Outcome::Authorized
    );
    assert!(pipe.lines.next_line().await.expect("eof").is_none());
    assert!(desk.conversation().is_none());
    assert_eq!(desk.queue.shown().expect("next").key, "cookie-b");
    assert!(matches!(second_reply.try_recv(), Err(TryRecvError::Empty)));

    desk.start();
    let mut pipe = accept(&listener).await;
    assert_eq!(pipe.line().await, "ada");
    assert_eq!(pipe.line().await, "cookie-b");
}

#[tokio::test]
async fn a_wrong_password_retries_without_a_limit() {
    let scratch = Scratch::new();
    let socket = scratch.path().join("helper.sock");
    let listener = UnixListener::bind(&socket).expect("socket");
    let mut desk = Desk::socket(socket);
    let (ask, mut reply) = queued("cookie-a", vec![admin("ada", "Ada")]);
    desk.queue.push(ask);
    desk.start();

    let mut passwords = Vec::new();
    for index in 0..5 {
        let mut pipe = accept(&listener).await;
        assert_eq!(pipe.line().await, "ada");
        assert_eq!(pipe.line().await, "cookie-a");
        pipe.send("PAM_PROMPT_ECHO_OFF Password:").await;
        desk.wait_prompt().await;
        let view = view_for(
            &desk.queue.shown().expect("head").prompt,
            desk.conversation(),
        );
        assert_eq!(view.secret, Some(("Password".to_owned(), Field::Password)));
        desk.confirm(0, &format!("try-{index}"));
        passwords.push(pipe.line().await);
        pipe.send("FAILURE").await;
        desk.wait_until(|desk| {
            desk.conversation().is_some_and(|conversation| {
                conversation.note == Some(Note::Error("Wrong password".to_owned()))
                    && !conversation.busy
                    && conversation.prompt.is_none()
            })
        })
        .await;
        assert_eq!(desk.queue.shown().expect("head").key, "cookie-a");
        assert!(matches!(reply.try_recv(), Err(TryRecvError::Empty)));
    }
    assert_eq!(passwords, ["try-0", "try-1", "try-2", "try-3", "try-4"]);
}

#[tokio::test]
async fn cancel_and_escape_refuse_the_request() {
    let scratch = Scratch::new();
    let socket = scratch.path().join("helper.sock");
    let listener = UnixListener::bind(&socket).expect("socket");
    let mut desk = Desk::socket(socket);
    let (first, mut first_reply) = queued("cookie-a", vec![admin("ada", "Ada")]);
    let (second, mut second_reply) = queued("cookie-b", vec![admin("ada", "Ada")]);
    desk.queue.push(first);
    desk.queue.push(second);
    desk.start();
    let mut pipe = accept(&listener).await;
    assert_eq!(pipe.line().await, "ada");
    assert_eq!(pipe.line().await, "cookie-a");
    desk.respond(Response::Cancel);
    assert_eq!(
        desk.wait_outcome(&mut first_reply).await,
        Outcome::Cancelled
    );
    assert!(pipe.lines.next_line().await.expect("eof").is_none());
    assert!(matches!(second_reply.try_recv(), Err(TryRecvError::Empty)));
    assert_eq!(desk.queue.shown().expect("next").key, "cookie-b");

    desk.start();
    let mut pipe = accept(&listener).await;
    assert_eq!(pipe.line().await, "ada");
    assert_eq!(pipe.line().await, "cookie-b");
    desk.respond(Response::Cancel);
    assert_eq!(
        desk.wait_outcome(&mut second_reply).await,
        Outcome::Cancelled
    );
    assert!(pipe.lines.next_line().await.expect("eof").is_none());
}

#[tokio::test]
async fn a_polkit_cancel_drops_the_request_it_names() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let server = bus.connection().await;
    let (input, mut receiver) = relm4::channel();
    server
        .object_server()
        .at(
            PATH,
            agent::Agent::new(
                server.clone(),
                None,
                input,
                Arc::new(Mutex::new(HashSet::new())),
            ),
        )
        .await
        .expect("export");
    let unique = server.unique_name().expect("unique name").clone();
    let client = bus.connection().await;
    let proxy = AuthenticationAgentProxy::builder(&client)
        .destination(unique)
        .expect("destination")
        .path(PATH)
        .expect("path")
        .build()
        .await
        .expect("proxy");
    let begin_a = {
        let proxy = proxy.clone();
        tokio::spawn(async move { call_begin(&proxy, "cookie-a", HashMap::new()).await })
    };
    let ask_a = recv_ask(&mut receiver).await;
    let begin_b = {
        let proxy = proxy.clone();
        tokio::spawn(async move { call_begin(&proxy, "cookie-b", HashMap::new()).await })
    };
    let ask_b = recv_ask(&mut receiver).await;
    let mut queue = Queue::default();
    queue.push(ask_a);
    queue.push(ask_b);
    assert_eq!(queue.shown().expect("head").key, "cookie-a");

    proxy
        .cancel_authentication("cookie-b")
        .await
        .expect("cancel");
    withdraw(&mut queue, &mut receiver).await;
    let error = begin_b.await.expect("task").expect_err("cancelled");
    assert!(is_cancelled(&error));
    assert_eq!(queue.shown().expect("head").key, "cookie-a");

    proxy
        .cancel_authentication("cookie-a")
        .await
        .expect("cancel");
    withdraw(&mut queue, &mut receiver).await;
    let error = begin_a.await.expect("task").expect_err("cancelled");
    assert!(is_cancelled(&error));
    assert!(queue.shown().is_none());
}

#[tokio::test]
async fn a_second_admin_is_offered_and_authenticates_as_that_user() {
    let passwd = "\
ada:x:1000:1000:Ada Lovelace,office:/home/ada:/bin/sh
grace:x:1001:1001:Grace Hopper:/home/grace:/bin/sh
";
    let mut real_names = HashMap::new();
    real_names.insert(1001, "Grace Real".to_owned());
    let identities = resolve_identities(&[1001, 1000, 424_242], passwd, &real_names, 1000);
    assert_eq!(identities.len(), 2);
    assert_eq!(identities[0].user, "ada");
    assert_eq!(identities[0].name, "Ada Lovelace");
    assert_eq!(identities[1].user, "grace");
    assert_eq!(identities[1].name, "Grace Real");
    let view = view_for(&Prompt::Polkit(sample(&identities)), None);
    assert_eq!(view.accounts, ["Ada Lovelace", "Grace Real"]);

    let scratch = Scratch::new();
    let socket = scratch.path().join("helper.sock");
    let listener = UnixListener::bind(&socket).expect("socket");
    let mut desk = Desk::socket(socket);
    let identities = vec![admin("ada", "Ada Lovelace"), admin("grace", "Grace Real")];
    let (ask, mut outcome) = queued("cookie-a", identities);
    desk.queue.push(ask);
    desk.start();
    let mut first = accept(&listener).await;
    assert_eq!(first.line().await, "ada");
    assert_eq!(first.line().await, "cookie-a");
    first.send("PAM_PROMPT_ECHO_OFF Password:").await;
    desk.wait_prompt().await;
    desk.respond(selected_account(1).expect("an account"));
    assert!(first.lines.next_line().await.expect("closed").is_none());
    let mut second = accept(&listener).await;
    assert_eq!(second.line().await, "grace");
    assert_eq!(second.line().await, "cookie-a");
    second.send("PAM_PROMPT_ECHO_OFF Password:").await;
    desk.wait_prompt().await;
    assert_eq!(
        desk.conversation().map(|conversation| conversation.account),
        Some(1)
    );
    desk.confirm(1, "s3cret");
    assert_eq!(second.line().await, "s3cret");
    second.send("SUCCESS").await;
    assert_eq!(desk.wait_outcome(&mut outcome).await, Outcome::Authorized);
}

#[tokio::test]
async fn a_single_admin_gets_no_account_row() {
    let passwd = "ada:x:1000:1000:Ada Lovelace:/home/ada:/bin/sh\n";
    let identities = resolve_identities(&[1000], passwd, &HashMap::new(), 1000);
    assert_eq!(identities.len(), 1);
    let mut details = HashMap::new();
    details.insert("program".to_owned(), "/usr/bin/true".to_owned());
    let request = request(
        "org.freedesktop.policykit.exec",
        "Authentication is required to run a program",
        details,
        "cookie".to_owned(),
        identities,
    )
    .await;
    let view = view_for(&Prompt::Polkit(request), None);
    assert_eq!(view.accounts.len(), 1);
    assert!(!view.title.contains("ada") && !view.title.contains("Ada"));
    assert!(!view.body.contains("ada") && !view.body.contains("Ada"));
    assert!(view.details.iter().all(|(key, value)| {
        !key.contains("ada")
            && !key.contains("Ada")
            && !value.contains("ada")
            && !value.contains("Ada")
    }));
}

#[tokio::test]
async fn pam_info_text_is_shown_beside_a_usable_password_row() {
    let mut conversation = Conversation {
        account: 0,
        prompt: Some(("Password".to_owned(), false)),
        note: None,
        busy: false,
        attempt: None,
        responder: None,
    };
    on_helper_line(
        &mut conversation,
        Line::Prompt {
            text: "Code:".to_owned(),
            echo: true,
        },
    );
    let visible = view_for(
        &Prompt::Polkit(sample(&[admin("ada", "Ada")])),
        Some(&conversation),
    );
    assert_eq!(visible.secret, Some(("Code".to_owned(), Field::Visible)));
    on_helper_line(&mut conversation, Line::Error("reader failed".to_owned()));
    assert_eq!(
        conversation.note,
        Some(Note::Error("reader failed".to_owned()))
    );
    assert!(conversation.prompt.is_some());

    let scratch = Scratch::new();
    let socket = scratch.path().join("helper.sock");
    let listener = UnixListener::bind(&socket).expect("socket");
    let mut desk = Desk::socket(socket);
    let (ask, _reply) = queued("cookie-a", vec![admin("ada", "Ada")]);
    desk.queue.push(ask);
    desk.start();
    let mut pipe = accept(&listener).await;
    assert_eq!(pipe.line().await, "ada");
    assert_eq!(pipe.line().await, "cookie-a");
    pipe.send("PAM_TEXT_INFO Place your finger on the fingerprint reader")
        .await;
    desk.wait_until(|desk| {
        desk.conversation()
            .is_some_and(|conversation| conversation.note.is_some())
    })
    .await;
    let view = view_for(
        &desk.queue.shown().expect("head").prompt,
        desk.conversation(),
    );
    assert_eq!(
        view.note,
        Some(Note::Info(
            "Place your finger on the fingerprint reader".to_owned()
        ))
    );
    assert_eq!(view.secret, Some(("Password".to_owned(), Field::Password)));
    assert!(!view.busy);
    assert!(Field::Password.accepts("x"));
    desk.confirm(0, "secret");
    pipe.send("PAM_PROMPT_ECHO_OFF Password:").await;
    assert_eq!(pipe.line().await, "secret");
}

#[tokio::test]
async fn socket_mode_writes_the_user_then_the_cookie() {
    let scratch = Scratch::new();
    let socket = scratch.path().join("helper.sock");
    assert_ne!(socket, Path::new(helper::SOCKET));
    let listener = UnixListener::bind(&socket).expect("socket");
    let attempt =
        tokio::spawn(async move { Attempt::start("ada", "cookie-1", &socket, &[]).await });
    let (stream, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
        .await
        .expect("accept timed out")
        .expect("accept");
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    assert_eq!(lines.next_line().await.expect("user").expect("user"), "ada");
    assert_eq!(
        lines.next_line().await.expect("cookie").expect("cookie"),
        "cookie-1"
    );
    write
        .write_all(b"PAM_PROMPT_ECHO_OFF Pass\\040word:\n")
        .await
        .expect("prompt");
    write.flush().await.expect("flush");
    let mut attempt = attempt.await.expect("task").expect("open");
    assert_eq!(
        attempt.next().await.expect("line").expect("prompt"),
        Line::Prompt {
            text: "Pass word:".to_owned(),
            echo: false,
        }
    );
    attempt
        .respond(&Zeroizing::new("secret".to_owned()))
        .await
        .expect("answer");
    assert_eq!(
        lines.next_line().await.expect("answer").expect("answer"),
        "secret"
    );
}

#[tokio::test]
async fn an_unreadable_helper_line_fails_one_attempt_only() {
    let scratch = Scratch::new();
    let socket = scratch.path().join("helper.sock");
    let listener = UnixListener::bind(&socket).expect("socket");
    let mut desk = Desk::socket(socket);
    let (ask, mut reply) = queued("cookie-a", vec![admin("ada", "Ada")]);
    desk.queue.push(ask);
    desk.start();
    let mut pipe = accept(&listener).await;
    assert_eq!(pipe.line().await, "ada");
    assert_eq!(pipe.line().await, "cookie-a");
    pipe.send("PAM_PROMPT_ECHO_OFF Password:").await;
    desk.wait_prompt().await;
    desk.confirm(0, "secret");
    assert_eq!(pipe.line().await, "secret");
    pipe.send("NOT A LINE").await;
    desk.wait_until(|desk| {
        desk.conversation().is_some_and(|conversation| {
            conversation.note == Some(Note::Error("Can't verify passwords".to_owned()))
                && !conversation.busy
                && conversation.attempt.is_none()
        })
    })
    .await;
    assert_eq!(desk.queue.shown().expect("head").key, "cookie-a");
    assert!(matches!(reply.try_recv(), Err(TryRecvError::Empty)));
    assert_eq!(
        on_dialog(desk.conversation(), &confirm(0, "again")),
        ResponseEffect::Reconnect
    );
    desk.confirm(0, "again");
    let mut pipe = accept(&listener).await;
    assert_eq!(pipe.line().await, "ada");
    assert_eq!(pipe.line().await, "cookie-a");
}

#[tokio::test]
async fn a_helper_that_cannot_start_cannot_verify_passwords() {
    let scratch = Scratch::new();
    let socket = scratch.path().join("agent-helper.socket");
    let helper = scratch.path().join("polkit-agent-helper-1");
    assert!(!socket.exists());
    assert!(!helper.exists());
    cannot_verify(&socket, vec![helper]).await;

    let plain = scratch.path().join("plain-helper");
    let touched = scratch.path().join("touched");
    script(
        &plain,
        &format!(
            "#!/bin/sh\nprintf x > '{}'\nprintf 'FAILURE\\n'\n",
            touched.display()
        ),
    );
    cannot_verify(&scratch.path().join("no.sock"), vec![plain]).await;
    assert!(
        !touched.exists(),
        "a helper without the setuid bit was started"
    );

    let closed = scratch.path().join("closed.sock");
    let listener = UnixListener::bind(&closed).expect("bind");
    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            drop(stream);
        }
    });
    cannot_verify(&closed, Vec::new()).await;
}

async fn cannot_verify(socket: &Path, helpers: Vec<PathBuf>) {
    let mut desk = Desk::helpers(socket.to_owned(), helpers);
    let (ask, mut outcome) = queued("cookie-a", vec![admin("ada", "Ada")]);
    desk.queue.push(ask);
    desk.start();
    desk.wait_until(|desk| {
        desk.conversation().is_some_and(|conversation| {
            conversation.note == Some(Note::Unavailable("Can't verify passwords".to_owned()))
                && !conversation.busy
        })
    })
    .await;
    let view = view_for(
        &desk.queue.shown().expect("head").prompt,
        desk.conversation(),
    );
    assert_eq!(
        view.note,
        Some(Note::Unavailable("Can't verify passwords".to_owned()))
    );
    assert!(view.secret.is_some());
    assert_ne!(view.note, Some(Note::Error("Wrong password".to_owned())));
    assert_eq!(
        on_dialog(desk.conversation(), &confirm(0, "secret")),
        ResponseEffect::Ignore
    );
    assert!(matches!(outcome.try_recv(), Err(TryRecvError::Empty)));
    assert_eq!(desk.queue.shown().expect("still showing").key, "cookie-a");
}

#[tokio::test]
async fn the_helper_without_a_socket_is_spawned_with_the_user() {
    let scratch = Scratch::new();
    let helper = scratch.path().join("polkit-agent-helper-1");
    let arg = scratch.path().join("arg");
    let cookie = scratch.path().join("cookie");
    script(
        &helper,
        &format!(
            "#!/bin/sh\nread line\nprintf '%s\\n' \"$1\" > '{}'\nprintf '%s\\n' \"$line\" > '{}'\nprintf 'SUCCESS\\n'\n",
            arg.display(),
            cookie.display()
        ),
    );
    let missing = scratch.path().join("agent-helper.socket");
    assert!(!missing.exists());
    assert_ne!(missing, Path::new(helper::SOCKET));
    match Attempt::start("ada", "cookie-9", &missing, std::slice::from_ref(&helper)).await {
        Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::NotFound),
        Ok(_) => panic!("a helper without the setuid bit was started"),
    }
    assert!(!arg.exists(), "a helper without the setuid bit was started");
    let mut attempt = Attempt::spawned(&helper, "ada", "cookie-9")
        .await
        .expect("spawn");
    assert_eq!(attempt.next().await.expect("line"), Some(Line::Success));
    assert_eq!(std::fs::read_to_string(arg).expect("arg").trim(), "ada");
    assert_eq!(
        std::fs::read_to_string(cookie).expect("cookie").trim(),
        "cookie-9"
    );

    let unit = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/systemd/glimpse-agents.service"
    ))
    .expect("unit");
    for line in unit.lines() {
        let line = line.trim();
        assert!(!line.starts_with("NoNewPrivileges"));
        assert!(!line.starts_with("PrivateUsers"));
        assert!(!line.starts_with("RestrictNamespaces"));
    }
}

#[tokio::test]
async fn a_duplicate_agent_leaves_the_other_two_running() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let host = bus.connection().await;
    let duplicate = Arc::new(AtomicBool::new(true));
    let unregisters = Arc::new(AtomicUsize::new(0));
    let notes = Arc::new(Mutex::new(Vec::new()));
    let dbus = zbus::fdo::DBusProxy::new(&host).await.expect("bus");
    dbus.request_name(
        "org.freedesktop.PolicyKit1".try_into().expect("name"),
        Default::default(),
    )
    .await
    .expect("polkit name");
    host.object_server()
        .at(
            "/org/freedesktop/PolicyKit1/Authority",
            Authority {
                duplicate: duplicate.clone(),
                unregisters: unregisters.clone(),
            },
        )
        .await
        .expect("authority");
    dbus.request_name(
        "org.freedesktop.Notifications".try_into().expect("name"),
        Default::default(),
    )
    .await
    .expect("notification name");
    host.object_server()
        .at("/org/freedesktop/Notifications", Notes(notes.clone()))
        .await
        .expect("notifications");

    let agents = bus.connection().await;
    agents
        .object_server()
        .at("/me/aresa/glimpse/network", NetworkStandIn)
        .await
        .expect("network stand-in");
    agents
        .object_server()
        .at("/me/aresa/glimpse/bluetooth", BluetoothStandIn)
        .await
        .expect("bluetooth stand-in");
    let unique = agents.unique_name().expect("unique name").clone();

    let (input, receiver) = relm4::channel();
    agent::serve_with(
        agents.clone(),
        Some(agents.clone()),
        input,
        "1",
        Arc::new(Mutex::new(HashSet::new())),
    )
    .await;
    let posted = notes.lock().expect("notes").clone();
    assert_eq!(posted.len(), 1);
    assert!(posted[0].contains(
        "Another authentication agent is running; glimpse can't ask for admin passwords"
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(200), receiver.recv())
            .await
            .is_err()
    );
    peer_ping(&host, &unique, "/me/aresa/glimpse/network").await;
    peer_ping(&host, &unique, "/me/aresa/glimpse/bluetooth").await;

    assert_eq!(
        agent::probe(&agents, "1").await,
        Err("another agent".to_owned())
    );
    assert_eq!(unregisters.load(Ordering::SeqCst), 0);

    duplicate.store(false, Ordering::SeqCst);
    let fresh = bus.connection().await;
    assert_eq!(agent::probe(&fresh, "1").await, Ok("ok".to_owned()));
    assert_eq!(unregisters.load(Ordering::SeqCst), 1);

    glimpse_dbus::own_name(&agents, "me.aresa.Glimpse.Agents")
        .await
        .expect("agents name");
    assert_eq!(
        agent::registration_report(&agents, &agents).await,
        Ok("running".to_owned())
    );
}

#[tokio::test]
async fn a_cancel_after_the_ask_opens_no_helper() {
    let cancelled = Arc::new(Mutex::new(HashSet::new()));
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let server = bus.connection().await;
    let (input, mut receiver) = relm4::channel();
    server
        .object_server()
        .at(
            PATH,
            agent::Agent::new(server.clone(), None, input, Arc::clone(&cancelled)),
        )
        .await
        .expect("export");
    let unique = server.unique_name().expect("unique name").clone();
    let client = bus.connection().await;
    let proxy = AuthenticationAgentProxy::builder(&client)
        .destination(unique)
        .expect("destination")
        .path(PATH)
        .expect("path")
        .build()
        .await
        .expect("proxy");
    let proxy_begin = proxy.clone();
    let mut begin =
        tokio::spawn(async move { call_begin(&proxy_begin, "cookie-a", HashMap::new()).await });
    let ask = recv_ask(&mut receiver).await;
    proxy
        .cancel_authentication("cookie-a")
        .await
        .expect("cancel");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut begin)
            .await
            .is_err(),
        "authentication returned before the withdraw dropped the ask"
    );

    let scratch = Scratch::new();
    let socket = scratch.path().join("helper.sock");
    let listener = UnixListener::bind(&socket).expect("socket");
    let (driver_input, _driver_receiver) = relm4::channel();
    let mut driver = Driver::with_cancelled(
        tokio::runtime::Handle::current(),
        driver_input,
        socket,
        Vec::new(),
        cancelled,
    );
    let mut queue = Queue::default();
    queue.push(ask);
    driver.start(&queue);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err(),
        "a cancelled cookie opened a helper"
    );
    withdraw(&mut queue, &mut receiver).await;
    let error = begin.await.expect("task").expect_err("cancelled");
    assert!(is_cancelled(&error));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err(),
        "the helper opened after the ask was withdrawn"
    );
}

fn admin(user: &str, name: &str) -> Identity {
    Identity {
        uid: if user == "ada" { 1000 } else { 1001 },
        user: user.to_owned(),
        name: name.to_owned(),
    }
}

fn sample(identities: &[Identity]) -> Request {
    Request {
        action_id: "org.freedesktop.policykit.exec".to_owned(),
        message: "Authentication is required".to_owned(),
        details: vec![("program".to_owned(), "/usr/bin/true".to_owned())],
        cookie: "cookie".to_owned(),
        identities: identities.to_vec(),
    }
}

fn queued(cookie: &str, identities: Vec<Identity>) -> (Ask, oneshot::Receiver<Outcome>) {
    let (reply, outcome) = oneshot::channel();
    let ask = Ask {
        source: Source::Polkit,
        key: cookie.to_owned(),
        prompt: Prompt::Polkit(Request {
            cookie: cookie.to_owned(),
            ..sample(&identities)
        }),
        reply: Reply::Polkit(reply),
    };
    (ask, outcome)
}

fn confirm(account: usize, secret: &str) -> Response {
    Response::Confirm {
        account,
        username: String::new(),
        secret: Zeroizing::new(secret.to_owned()),
        autoconnect: false,
    }
}

struct Desk {
    queue: Queue,
    driver: Driver,
    input: relm4::Sender<Input>,
    receiver: relm4::Receiver<Input>,
}

impl Desk {
    fn socket(socket: PathBuf) -> Self {
        Self::helpers(socket, Vec::new())
    }

    fn helpers(socket: PathBuf, setuid: Vec<PathBuf>) -> Self {
        assert_ne!(socket, Path::new(helper::SOCKET));
        let (input, receiver) = relm4::channel();
        Self {
            queue: Queue::default(),
            driver: Driver::with_helpers(
                tokio::runtime::Handle::current(),
                input.clone(),
                socket,
                setuid,
            ),
            input,
            receiver,
        }
    }

    fn start(&mut self) {
        self.driver.start(&self.queue);
    }

    fn respond(&mut self, response: Response) {
        self.driver.on_response(&mut self.queue, response);
    }

    fn confirm(&mut self, account: usize, secret: &str) {
        self.respond(confirm(account, secret));
    }

    fn conversation(&self) -> Option<&Conversation> {
        self.driver.conversation.as_ref()
    }

    async fn pump(&mut self) {
        if let Ok(Some(Input::Line { key, line })) =
            tokio::time::timeout(Duration::from_millis(50), self.receiver.recv()).await
        {
            self.driver.on_line(&mut self.queue, &key, line);
        }
    }

    async fn wait_until(&mut self, ready: impl Fn(&Desk) -> bool) {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(3) {
            if ready(self) {
                return;
            }
            self.pump().await;
        }
        panic!("timed out waiting for the polkit dialog");
    }

    async fn wait_prompt(&mut self) {
        self.wait_until(|desk| {
            desk.conversation()
                .is_some_and(|conversation| conversation.prompt.is_some() && !conversation.busy)
        })
        .await;
    }

    async fn wait_outcome(&mut self, outcome: &mut oneshot::Receiver<Outcome>) -> Outcome {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(3) {
            if let Ok(outcome) = outcome.try_recv() {
                return outcome;
            }
            self.pump().await;
        }
        panic!("the polkit request was not answered");
    }
}

struct Pipe {
    lines: tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    write: tokio::net::unix::OwnedWriteHalf,
}

async fn accept(listener: &UnixListener) -> Pipe {
    let (stream, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
        .await
        .expect("helper accept timed out")
        .expect("accept");
    let (read, write) = stream.into_split();
    Pipe {
        lines: BufReader::new(read).lines(),
        write,
    }
}

impl Pipe {
    async fn line(&mut self) -> String {
        self.lines
            .next_line()
            .await
            .expect("read")
            .expect("a helper line")
    }

    async fn send(&mut self, line: &str) {
        self.write.write_all(line.as_bytes()).await.expect("write");
        self.write.write_all(b"\n").await.expect("newline");
        self.write.flush().await.expect("flush");
    }
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "glimpse-polkit-agent-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).expect("scratch");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn script(path: &Path, body: &str) {
    std::fs::write(path, body).expect("script");
    let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("executable");
}

use super::helper::Attempt;

#[zbus::proxy(
    interface = "org.freedesktop.PolicyKit1.AuthenticationAgent",
    default_path = "/org/freedesktop/PolicyKit1/AuthenticationAgent"
)]
trait AuthenticationAgent {
    fn begin_authentication(
        &self,
        action_id: &str,
        message: &str,
        icon_name: &str,
        details: HashMap<&str, &str>,
        cookie: &str,
        identities: Vec<(&str, HashMap<&str, Value<'_>>)>,
    ) -> zbus::Result<()>;

    fn cancel_authentication(&self, cookie: &str) -> zbus::Result<()>;
}

async fn call_begin(
    proxy: &AuthenticationAgentProxy<'_>,
    cookie: &str,
    details: HashMap<&str, &str>,
) -> zbus::Result<()> {
    proxy
        .begin_authentication(
            "org.freedesktop.fwupd.update-internal",
            "Authentication is required to update the system firmware",
            "",
            details,
            cookie,
            Vec::new(),
        )
        .await
}

fn is_cancelled(error: &zbus::Error) -> bool {
    matches!(
        error,
        zbus::Error::MethodError(name, _, _)
            if name.as_str() == "org.freedesktop.PolicyKit1.Error.Cancelled"
    )
}

async fn recv_ask(receiver: &mut relm4::Receiver<Input>) -> Ask {
    match tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .expect("ask timed out")
        .expect("ask")
    {
        Input::Ask(ask) => ask,
        other => panic!("expected an ask, got {other:?}"),
    }
}

async fn withdraw(queue: &mut Queue, receiver: &mut relm4::Receiver<Input>) {
    match tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .expect("withdraw timed out")
        .expect("withdraw")
    {
        Input::Withdraw { source, key } => queue.withdraw(source, key.as_deref()),
        other => panic!("expected a withdraw, got {other:?}"),
    }
}

struct Authority {
    duplicate: Arc<AtomicBool>,
    unregisters: Arc<AtomicUsize>,
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.PolicyKit1.Error")]
enum AuthorityError {
    Failed(String),
}

#[zbus::interface(name = "org.freedesktop.PolicyKit1.Authority")]
impl Authority {
    async fn register_authentication_agent(
        &self,
        _subject: (String, HashMap<String, OwnedValue>),
        _locale: String,
        _object_path: String,
    ) -> Result<(), AuthorityError> {
        if self.duplicate.load(Ordering::SeqCst) {
            Err(AuthorityError::Failed(
                "An authentication agent already exists for the given subject".to_owned(),
            ))
        } else {
            Ok(())
        }
    }

    async fn unregister_authentication_agent(
        &self,
        _subject: (String, HashMap<String, OwnedValue>),
        _object_path: String,
    ) -> Result<(), AuthorityError> {
        self.unregisters.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct Notes(Arc<Mutex<Vec<String>>>);

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notes {
    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &self,
        _app_name: String,
        _replaces_id: u32,
        _app_icon: String,
        summary: String,
        _body: String,
        _actions: Vec<String>,
        _hints: HashMap<String, OwnedValue>,
        _expire_timeout: i32,
    ) -> u32 {
        self.0.lock().expect("notes").push(summary);
        1
    }
}

struct NetworkStandIn;

#[zbus::interface(name = "org.freedesktop.NetworkManager.SecretAgent")]
impl NetworkStandIn {
    async fn ping(&self) -> &'static str {
        "ok"
    }
}

struct BluetoothStandIn;

#[zbus::interface(name = "org.bluez.Agent1")]
impl BluetoothStandIn {
    async fn ping(&self) -> &'static str {
        "ok"
    }
}

async fn peer_ping(bus: &zbus::Connection, destination: &zbus::names::UniqueName<'_>, path: &str) {
    zbus::fdo::PeerProxy::builder(bus)
        .destination(destination.to_owned())
        .expect("destination")
        .path(path)
        .expect("path")
        .build()
        .await
        .expect("peer")
        .ping()
        .await
        .expect("ping");
}

#[test]
fn descends_walks_the_parent_chain_and_stops() {
    use super::requester::descends;

    let parents = HashMap::from([(30, 20), (20, 10), (10, 1), (50, 60), (60, 50)]);
    let parent = |pid: u32| parents.get(&pid).copied();
    assert!(descends(30, 30, parent));
    assert!(descends(30, 10, parent));
    assert!(!descends(30, 99, parent));
    assert!(!descends(50, 99, parent));
}

#[test]
fn the_single_requester_window_is_found_on_the_process_or_an_ancestor() {
    use super::requester::{Windows, window_of};

    let parents = HashMap::from([(30, 20), (20, 10), (10, 1), (50, 60), (60, 50)]);
    let parent = |pid: u32| parents.get(&pid).copied();
    let windows = |owners: Vec<(u64, u32)>| Windows {
        focused: None,
        owners,
    };
    assert_eq!(
        window_of(&windows(vec![(7, 30), (9, 5)]), 30, parent),
        Some(7)
    );
    assert_eq!(
        window_of(&windows(vec![(8, 10), (9, 5)]), 30, parent),
        Some(8)
    );
    assert_eq!(
        window_of(&windows(vec![(7, 20), (8, 20)]), 30, parent),
        None
    );
    assert_eq!(window_of(&windows(vec![(9, 5)]), 30, parent), None);
    assert_eq!(window_of(&windows(vec![(9, 5)]), 50, parent), None);
}

#[derive(Debug, Clone, Default)]
struct Posted {
    app: String,
    actions: Vec<String>,
    desktop_entry: Option<String>,
}

struct FakeNotifications {
    posted: Arc<Mutex<Vec<Posted>>>,
    closes: Arc<Mutex<Vec<u32>>>,
    next: std::sync::atomic::AtomicU32,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl FakeNotifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        _replaces_id: u32,
        _app_icon: String,
        _summary: String,
        _body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        _expire_timeout: i32,
    ) -> u32 {
        let text = |key: &str| {
            hints
                .get(key)
                .and_then(|value| <&str>::try_from(&**value).ok())
                .map(str::to_owned)
        };
        self.posted.lock().expect("posted").push(Posted {
            app: app_name,
            actions,
            desktop_entry: text("desktop-entry"),
        });
        self.next.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn get_capabilities(&self) -> Vec<String> {
        ["actions", "body", "persistence"]
            .map(str::to_owned)
            .to_vec()
    }

    fn close_notification(&self, id: u32) {
        self.closes.lock().expect("closes").push(id);
    }

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        id: u32,
        action: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;
}

struct FakeCompositor {
    focused: Option<u64>,
    windows: Vec<(u64, i32)>,
    focuses: Arc<Mutex<Vec<u64>>>,
}

#[zbus::interface(name = "me.aresa.Glimpse.Compositor1")]
impl FakeCompositor {
    #[zbus(property)]
    fn focus(&self) -> HashMap<String, OwnedValue> {
        self.focused
            .map(|id| HashMap::from([("window".to_owned(), OwnedValue::from(id))]))
            .unwrap_or_default()
    }

    #[zbus(property)]
    fn windows(&self) -> Vec<HashMap<String, OwnedValue>> {
        self.windows
            .iter()
            .map(|(id, pid)| {
                HashMap::from([
                    ("id".to_owned(), OwnedValue::from(*id)),
                    ("pid".to_owned(), OwnedValue::from(*pid)),
                ])
            })
            .collect()
    }

    fn focus_window(&self, id: u64) {
        self.focuses.lock().expect("focuses").push(id);
    }
}

const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";

struct Deferral {
    _system: glimpse_dbus::testing::PrivateBus,
    _session: glimpse_dbus::testing::PrivateBus,
    service: zbus::Connection,
    proxy: AuthenticationAgentProxy<'static>,
    receiver: relm4::Receiver<Input>,
    posted: Arc<Mutex<Vec<Posted>>>,
    closes: Arc<Mutex<Vec<u32>>>,
    focuses: Arc<Mutex<Vec<u64>>>,
    child: std::process::Child,
}

impl Drop for Deferral {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn requester_process() -> std::process::Child {
    use std::os::unix::process::CommandExt;

    let mut command = std::process::Command::new("sleep");
    command.arg("30");
    unsafe {
        command.pre_exec(|| {
            rustix::process::setsid()
                .map(drop)
                .map_err(std::io::Error::from)
        });
    }
    command.spawn().expect("requester process")
}

impl Deferral {
    async fn start(layout: impl FnOnce(i32) -> (Option<u64>, Vec<(u64, i32)>)) -> Self {
        let child = requester_process();
        let pid = i32::try_from(child.id()).expect("pid");
        let (focused, windows) = layout(pid);
        let system = glimpse_dbus::testing::PrivateBus::start();
        let session = glimpse_dbus::testing::PrivateBus::start();
        let service = session.connection().await;
        let posted = Arc::new(Mutex::new(Vec::new()));
        let closes = Arc::new(Mutex::new(Vec::new()));
        let focuses = Arc::new(Mutex::new(Vec::new()));
        service
            .object_server()
            .at(
                glimpse_compositors::contract::OBJECT_PATH,
                FakeCompositor {
                    focused,
                    windows,
                    focuses: Arc::clone(&focuses),
                },
            )
            .await
            .expect("compositor");
        service
            .object_server()
            .at(
                NOTIFICATIONS_PATH,
                FakeNotifications {
                    posted: Arc::clone(&posted),
                    closes: Arc::clone(&closes),
                    next: std::sync::atomic::AtomicU32::new(0),
                },
            )
            .await
            .expect("notifications");
        for name in [
            glimpse_compositors::contract::BUS_NAME,
            "org.freedesktop.Notifications",
        ] {
            glimpse_dbus::own_name(&service, name)
                .await
                .expect("own the bus name");
        }
        let server = system.connection().await;
        let (input, receiver) = relm4::channel();
        server
            .object_server()
            .at(
                PATH,
                agent::Agent::new(
                    server.clone(),
                    Some(session.connection().await),
                    input,
                    Arc::new(Mutex::new(HashSet::new())),
                ),
            )
            .await
            .expect("export");
        let unique = server.unique_name().expect("unique name").clone();
        let client = system.connection().await;
        let proxy = AuthenticationAgentProxy::builder(&client)
            .destination(unique)
            .expect("destination")
            .path(PATH)
            .expect("path")
            .build()
            .await
            .expect("proxy");
        Self {
            _system: system,
            _session: session,
            service,
            proxy,
            receiver,
            posted,
            closes,
            focuses,
            child,
        }
    }

    fn begin(&self, cookie: &'static str) -> tokio::task::JoinHandle<zbus::Result<()>> {
        let pid = self.child.id().to_string();
        self.begin_with(
            cookie,
            HashMap::from([("polkit.subject-pid", pid.as_str())]),
        )
    }

    fn begin_with(
        &self,
        cookie: &'static str,
        details: HashMap<&str, &str>,
    ) -> tokio::task::JoinHandle<zbus::Result<()>> {
        let proxy = self.proxy.clone();
        let details: HashMap<String, String> = details
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect();
        tokio::spawn(async move {
            let details = details
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()))
                .collect();
            call_begin(&proxy, cookie, details).await
        })
    }

    async fn posted(&self) -> Posted {
        eventually(Duration::from_secs(2), || {
            self.posted.lock().expect("posted").first().cloned()
        })
        .await
        .expect("a notification")
    }

    async fn quiet(&mut self) {
        assert!(
            tokio::time::timeout(Duration::from_millis(300), self.receiver.recv())
                .await
                .is_err(),
            "an input arrived"
        );
    }

    async fn notifications(&self) -> zbus::object_server::InterfaceRef<FakeNotifications> {
        self.service
            .object_server()
            .interface::<_, FakeNotifications>(NOTIFICATIONS_PATH)
            .await
            .expect("notifications")
    }

    async fn emit_action(&self, id: u32, action: &str) {
        let interface = self.notifications().await;
        FakeNotifications::action_invoked(interface.signal_emitter(), id, action)
            .await
            .expect("action invoked");
    }

    async fn emit_closed(&self, id: u32, reason: u32) {
        let interface = self.notifications().await;
        FakeNotifications::notification_closed(interface.signal_emitter(), id, reason)
            .await
            .expect("notification closed");
    }

    async fn closed(&self, id: u32, within: Duration) -> bool {
        eventually(within, || {
            self.closes
                .lock()
                .expect("closes")
                .contains(&id)
                .then_some(())
        })
        .await
        .is_some()
    }
}

async fn eventually<T>(within: Duration, check: impl Fn() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + within;
    loop {
        if let Some(value) = check() {
            return Some(value);
        }
        if Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn background(pid: i32) -> (Option<u64>, Vec<(u64, i32)>) {
    (Some(99), vec![(5, pid), (6, pid), (99, i32::MAX)])
}

async fn cancelled_within(begin: tokio::task::JoinHandle<zbus::Result<()>>, within: Duration) {
    let error = tokio::time::timeout(within, begin)
        .await
        .expect("begin returned in time")
        .expect("task")
        .expect_err("cancelled");
    assert!(is_cancelled(&error));
}

#[tokio::test]
async fn a_background_request_waits_behind_a_notification_until_authenticate() {
    let mut fixture = Deferral::start(background).await;
    let begin = fixture.begin("cookie-a");
    let posted = fixture.posted().await;
    assert_eq!(posted.app, "Authentication");
    assert_eq!(
        posted.desktop_entry.as_deref(),
        Some("me.aresa.Glimpse.Agents")
    );
    assert_eq!(
        posted.actions,
        ["default", "Authenticate", "authenticate", "Authenticate"]
    );
    fixture.quiet().await;
    fixture.emit_action(1, "authenticate").await;
    let _ask = recv_ask(&mut fixture.receiver).await;
    assert!(fixture.closed(1, Duration::from_secs(1)).await);
    begin.abort();
}

#[tokio::test]
async fn dismissing_the_notification_cancels_the_request() {
    let mut fixture = Deferral::start(background).await;
    let begin = fixture.begin("cookie-a");
    fixture.posted().await;
    fixture.emit_closed(1, 2).await;
    cancelled_within(begin, Duration::from_secs(2)).await;
    fixture.quiet().await;
}

#[tokio::test]
async fn a_polkit_cancel_closes_the_notification_at_once() {
    let fixture = Deferral::start(background).await;
    let begin = fixture.begin("cookie-a");
    fixture.posted().await;
    fixture
        .proxy
        .cancel_authentication("cookie-a")
        .await
        .expect("cancel");
    assert!(fixture.closed(1, Duration::from_secs(1)).await);
    cancelled_within(begin, Duration::from_secs(1)).await;
}

#[tokio::test]
async fn the_requester_exiting_closes_the_notification() {
    let mut fixture = Deferral::start(background).await;
    let begin = fixture.begin("cookie-a");
    fixture.posted().await;
    fixture.child.kill().expect("kill");
    fixture.child.wait().expect("wait");
    assert!(fixture.closed(1, Duration::from_secs(1)).await);
    cancelled_within(begin, Duration::from_secs(1)).await;
}

#[tokio::test]
async fn show_focuses_the_single_requester_window_and_keeps_waiting() {
    let mut fixture = Deferral::start(|pid| (Some(99), vec![(7, pid), (99, i32::MAX)])).await;
    let begin = fixture.begin("cookie-a");
    let posted = fixture.posted().await;
    assert_eq!(posted.actions.len(), 6);
    assert_eq!(posted.actions[4], "focus");
    fixture.emit_action(1, "focus").await;
    let focused = eventually(Duration::from_secs(1), || {
        let focuses = fixture.focuses.lock().expect("focuses").clone();
        (!focuses.is_empty()).then_some(focuses)
    })
    .await;
    assert_eq!(focused, Some(vec![7]));
    assert!(fixture.closes.lock().expect("closes").is_empty());
    fixture.quiet().await;
    fixture.emit_action(1, "authenticate").await;
    let _ask = recv_ask(&mut fixture.receiver).await;
    begin.abort();
}

#[tokio::test]
async fn no_show_button_when_the_requester_has_several_windows() {
    let fixture = Deferral::start(|pid| (Some(99), vec![(7, pid), (8, pid), (99, i32::MAX)])).await;
    let begin = fixture.begin("cookie-a");
    let posted = fixture.posted().await;
    assert!(!posted.actions.iter().any(|action| action == "focus"));
    begin.abort();
}

#[tokio::test]
async fn a_request_from_the_focused_app_skips_the_notification() {
    let mut fixture = Deferral::start(|pid| (Some(7), vec![(7, pid)])).await;
    let begin = fixture.begin("cookie-a");
    let _ask = recv_ask(&mut fixture.receiver).await;
    assert!(fixture.posted.lock().expect("posted").is_empty());
    begin.abort();
}

#[tokio::test]
async fn a_close_the_user_did_not_make_opens_the_dialog() {
    let mut fixture = Deferral::start(background).await;
    let begin = fixture.begin("cookie-a");
    fixture.posted().await;
    fixture.emit_closed(1, 3).await;
    let _ask = recv_ask(&mut fixture.receiver).await;
    begin.abort();
}

#[tokio::test]
async fn the_subject_exiting_does_not_cancel_while_the_caller_waits() {
    let mut subject = requester_process();
    let subject_pid = i32::try_from(subject.id()).expect("pid");
    let mut fixture = Deferral::start(move |_| {
        (
            Some(99),
            vec![(5, subject_pid), (6, subject_pid), (99, i32::MAX)],
        )
    })
    .await;
    let subject_text = subject.id().to_string();
    let caller_text = fixture.child.id().to_string();
    let begin = fixture.begin_with(
        "cookie-a",
        HashMap::from([
            ("polkit.subject-pid", subject_text.as_str()),
            ("polkit.caller-pid", caller_text.as_str()),
        ]),
    );
    fixture.posted().await;
    subject.kill().expect("kill");
    subject.wait().expect("wait");
    assert!(!fixture.closed(1, Duration::from_millis(500)).await);
    fixture.quiet().await;
    fixture.emit_action(1, "authenticate").await;
    let _ask = recv_ask(&mut fixture.receiver).await;
    begin.abort();
}

#[tokio::test]
async fn a_notification_daemon_restart_opens_the_dialog() {
    let mut fixture = Deferral::start(background).await;
    let begin = fixture.begin("cookie-a");
    fixture.posted().await;
    fixture
        .service
        .release_name("org.freedesktop.Notifications")
        .await
        .expect("release");
    let _ask = recv_ask(&mut fixture.receiver).await;
    begin.abort();
}
