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
            agent::Agent::new(server.clone(), input, Arc::new(Mutex::new(HashSet::new()))),
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
        tokio::spawn(async move { call_begin(&proxy, "cookie-a").await })
    };
    let ask_a = recv_ask(&mut receiver).await;
    let begin_b = {
        let proxy = proxy.clone();
        tokio::spawn(async move { call_begin(&proxy, "cookie-b").await })
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
            agent::Agent::new(server.clone(), input, Arc::clone(&cancelled)),
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
    let mut begin = tokio::spawn(async move { call_begin(&proxy_begin, "cookie-a").await });
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

async fn call_begin(proxy: &AuthenticationAgentProxy<'_>, cookie: &str) -> zbus::Result<()> {
    proxy
        .begin_authentication(
            "org.freedesktop.fwupd.update-internal",
            "Authentication is required to update the system firmware",
            "",
            HashMap::new(),
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
