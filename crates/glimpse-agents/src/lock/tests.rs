use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use zbus::zvariant::OwnedObjectPath;

use super::follow;
use crate::app::App;
use crate::ask::{Ask, Input, Prompt, Reply, Source};
use crate::polkit::{Identity, Request};

const SESSION: &str = "/org/freedesktop/login1/session/_1";
const LOGIN: &str = "org.freedesktop.login1";

struct Session {
    hint: Arc<AtomicBool>,
    fail: bool,
}

#[zbus::interface(name = "org.freedesktop.login1.Session")]
impl Session {
    #[zbus(property)]
    fn active(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn class(&self) -> String {
        "user".to_owned()
    }

    #[zbus(property, name = "Type")]
    fn kind(&self) -> String {
        "wayland".to_owned()
    }

    #[zbus(property, name = "LockedHint")]
    fn locked_hint(&self) -> zbus::fdo::Result<bool> {
        if self.fail {
            Err(zbus::fdo::Error::Failed("unavailable".to_owned()))
        } else {
            Ok(self.hint.load(Ordering::SeqCst))
        }
    }
}

struct Manager {
    uid: u32,
}

#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl Manager {
    fn list_sessions(&self) -> Vec<glimpse_dbus::login1::Login1SessionEntry> {
        vec![(
            "1".to_owned(),
            self.uid,
            "user".to_owned(),
            "seat0".to_owned(),
            OwnedObjectPath::try_from(SESSION).expect("session path"),
        )]
    }
}

struct Hung;

#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl Hung {
    async fn list_sessions(&self) -> Vec<glimpse_dbus::login1::Login1SessionEntry> {
        std::future::pending::<()>().await;
        Vec::new()
    }
}

async fn export(server: &zbus::Connection, hint: bool, fail: bool) -> Arc<AtomicBool> {
    let hint = Arc::new(AtomicBool::new(hint));
    server
        .object_server()
        .at(
            "/org/freedesktop/login1",
            Manager {
                uid: glimpse_dbus::login1::current_uid(),
            },
        )
        .await
        .expect("manager");
    server
        .object_server()
        .at(
            SESSION,
            Session {
                hint: Arc::clone(&hint),
                fail,
            },
        )
        .await
        .expect("session");
    glimpse_dbus::own_name(server, LOGIN)
        .await
        .expect("login1 name");
    hint
}

async fn next_locked(receiver: &mut relm4::Receiver<Input>) -> bool {
    match tokio::time::timeout(Duration::from_secs(4), receiver.recv())
        .await
        .expect("no lock signal")
        .expect("sender dropped")
    {
        Input::Locked(locked) => locked,
        other => panic!("expected a lock signal, got {other:?}"),
    }
}

#[tokio::test]
async fn a_failed_hint_read_publishes_nothing() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let server = bus.connection().await;
    export(&server, false, true).await;
    let (sender, receiver) = relm4::channel();
    let following = tokio::spawn(follow(bus.connection().await, sender));
    let message = tokio::time::timeout(Duration::from_millis(400), receiver.recv()).await;
    assert!(
        message.is_err(),
        "a failed hint read published a lock signal"
    );
    following.abort();
}

#[tokio::test]
async fn a_lost_session_is_read_again() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let server = bus.connection().await;
    let hint = export(&server, false, false).await;
    let (sender, mut receiver) = relm4::channel();
    let following = tokio::spawn(follow(bus.connection().await, sender));
    assert!(!next_locked(&mut receiver).await);
    hint.store(true, Ordering::SeqCst);
    let dbus = zbus::fdo::DBusProxy::new(&server).await.expect("bus");
    dbus.release_name(LOGIN.try_into().expect("name"))
        .await
        .expect("release");
    dbus.request_name(LOGIN.try_into().expect("name"), Default::default())
        .await
        .expect("own login1 again");
    assert!(next_locked(&mut receiver).await);
    following.abort();
}

#[tokio::test]
async fn logind_out_of_reach_stays_unlocked() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let (sender, receiver) = relm4::channel();
    tokio::time::timeout(
        Duration::from_secs(2),
        follow(bus.connection().await, sender),
    )
    .await
    .expect("an unreachable logind did not give up");
    assert!(
        receiver.recv().await.is_none(),
        "an unreachable logind published a lock signal"
    );
}

#[tokio::test]
async fn a_hung_logind_answers_within_the_deadline() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let server = bus.connection().await;
    server
        .object_server()
        .at("/org/freedesktop/login1", Hung)
        .await
        .expect("manager");
    glimpse_dbus::own_name(&server, LOGIN)
        .await
        .expect("login1 name");
    let (sender, _receiver) = relm4::channel();
    tokio::time::timeout(
        Duration::from_secs(7),
        follow(bus.connection().await, sender),
    )
    .await
    .expect("logind was given longer than its deadline");
}

#[tokio::test]
async fn a_locked_session_holds_polkit_and_reopens_it_fresh() {
    let scratch = std::env::temp_dir().join(format!(
        "glimpse-lock-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&scratch).expect("scratch");
    let socket = scratch.join("helper.sock");
    let listener = UnixListener::bind(&socket).expect("socket");
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let (input, mut receiver) = relm4::channel();
    let mut app = App::headless(
        tokio::runtime::Handle::current(),
        input.clone(),
        bus.connection().await,
        Some(socket),
    );
    let (reply, mut outcome) = tokio::sync::oneshot::channel();
    app.drive(
        Input::Ask(Ask {
            source: Source::Polkit,
            key: "cookie-a".to_owned(),
            prompt: Prompt::Polkit(Request {
                action_id: "org.freedesktop.policykit.exec".to_owned(),
                message: "Authentication is required".to_owned(),
                details: Vec::new(),
                cookie: "cookie-a".to_owned(),
                identities: vec![Identity {
                    uid: 1000,
                    user: "ada".to_owned(),
                    name: "Ada".to_owned(),
                }],
            }),
            reply: Reply::Polkit(reply),
        }),
        &input,
    );
    let mut first = accept(&listener).await;
    assert_eq!(first.line().await, "ada");
    assert_eq!(first.line().await, "cookie-a");
    first.send("PAM_PROMPT_ECHO_ON Verification code:").await;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(3) {
        pump(&mut app, &input, &mut receiver).await;
        if app
            .conversation()
            .is_some_and(|conversation| conversation.prompt.is_some())
        {
            break;
        }
    }
    let prompt = app
        .conversation()
        .and_then(|conversation| conversation.prompt.clone());
    assert!(
        prompt
            .as_ref()
            .is_some_and(|(text, _)| text.contains("Verification")),
        "the helper prompt was not shown"
    );
    app.set_typed("hunter2");
    app.drive(Input::Locked(true), &input);
    assert!(!app.showing());
    assert_eq!(app.typed(), "");
    assert!(app.conversation().is_none());
    assert!(matches!(
        outcome.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ));
    assert!(first.lines.next_line().await.expect("eof").is_none());

    app.drive(Input::Locked(false), &input);
    assert!(app.showing());
    assert_eq!(app.typed(), "");
    assert!(
        app.conversation()
            .is_some_and(|conversation| conversation.prompt.is_none())
    );
    let mut second = accept(&listener).await;
    assert_eq!(second.line().await, "ada");
    assert_eq!(second.line().await, "cookie-a");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), second.lines.next_line())
            .await
            .is_err(),
        "the fresh attempt was sent a secret"
    );
    assert!(matches!(
        outcome.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ));
    let _ = std::fs::remove_dir_all(&scratch);
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

async fn pump(app: &mut App, input: &relm4::Sender<Input>, receiver: &mut relm4::Receiver<Input>) {
    while let Ok(Some(message)) =
        tokio::time::timeout(Duration::from_millis(30), receiver.recv()).await
    {
        app.drive(message, input);
    }
}
