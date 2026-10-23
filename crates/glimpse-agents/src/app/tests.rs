use tokio::sync::oneshot::{self, error::TryRecvError};
use zeroize::Zeroizing;

use super::{Routed, bluetooth_answer, network_answer, route, view_for};
use crate::ask::{Ask, Input, Prompt, Reply, Source};
use crate::bluetooth::{self, Kind};
use crate::dialog::{Field, Icon, Note, Response};
use crate::network;
use crate::polkit::{self, Conversation, Identity};
use crate::queue::Queue;

fn polkit_request() -> polkit::Request {
    polkit::Request {
        action_id: "org.freedesktop.policykit.exec".to_owned(),
        message: "Authentication is required to run a program as another user".to_owned(),
        details: vec![("program".to_owned(), "/usr/bin/true".to_owned())],
        cookie: "3-c0ffee-1-deadbeef".to_owned(),
        identities: vec![
            Identity {
                uid: 1000,
                user: "alex".to_owned(),
                name: "Alex Oleshkevich".to_owned(),
            },
            Identity {
                uid: 1001,
                user: "maria".to_owned(),
                name: String::new(),
            },
        ],
    }
}

fn wifi(retry: bool) -> network::Request {
    network::Request {
        name: "Café Central".to_owned(),
        path: "/org/freedesktop/NetworkManager/Settings/7".to_owned(),
        setting: "802-11-wireless-security".to_owned(),
        retry,
        hints: Vec::new(),
        autoconnect: false,
    }
}

fn vpn(hints: &[&str]) -> network::Request {
    network::Request {
        name: "Work VPN".to_owned(),
        setting: "vpn".to_owned(),
        hints: hints.iter().map(|hint| (*hint).to_owned()).collect(),
        ..wifi(false)
    }
}

fn pairing(name: &str, kind: Kind) -> Prompt {
    Prompt::Pairing(bluetooth::Request {
        device: "/org/bluez/hci0/dev_00".to_owned(),
        name: name.to_owned(),
        kind,
    })
}

fn confirm(secret: &str) -> Response {
    Response::Confirm {
        account: 0,
        username: "alex".to_owned(),
        secret: Zeroizing::new(secret.to_owned()),
        autoconnect: true,
    }
}

#[test]
fn polkit_asks_for_a_password_with_every_account_and_the_details() {
    let view = view_for(&Prompt::Polkit(polkit_request()), None);
    assert_eq!(view.icon, Icon::Lock);
    assert_eq!(view.title, "Authentication needed");
    assert_eq!(view.accounts, ["Alex Oleshkevich", "maria"]);
    assert_eq!(view.secret, Some(("Password".to_owned(), Field::Password)));
    assert_eq!(
        view.details,
        [
            ("program".to_owned(), "/usr/bin/true".to_owned()),
            (
                "Action".to_owned(),
                "org.freedesktop.policykit.exec".to_owned()
            ),
        ]
    );
    assert_eq!(view.confirm.as_deref(), Some("Authorize"));
    assert_eq!(view.cancel, "Cancel");
    assert!(!view.busy);
}

#[test]
fn a_pam_prompt_titles_the_secret_row_and_echo_on_shows_it() {
    let conversation = Conversation {
        account: 0,
        prompt: Some(("Verification code: ".to_owned(), true)),
        note: Some(Note::Error("Wrong password".to_owned())),
        busy: true,
        attempt: None,
        responder: None,
    };
    let view = view_for(&Prompt::Polkit(polkit_request()), Some(&conversation));
    assert_eq!(
        view.secret,
        Some(("Verification code".to_owned(), Field::Visible))
    );
    assert_eq!(view.note, Some(Note::Error("Wrong password".to_owned())));
    assert!(view.busy);
}

#[test]
fn wifi_asks_for_a_key_and_a_retry_says_it_failed() {
    let view = view_for(&Prompt::Secret(wifi(false)), None);
    assert_eq!(view.icon, Icon::Wifi);
    assert_eq!(view.title, "Join “Café Central”");
    assert_eq!(view.body, "The network needs a password");
    assert_eq!(view.secret, Some(("Password".to_owned(), Field::WifiKey)));
    assert_eq!(view.autoconnect, Some(false));
    assert_eq!(view.confirm.as_deref(), Some("Join"));

    assert_eq!(view.note, None);

    let retry = view_for(&Prompt::Secret(wifi(true)), None);
    assert_eq!(retry.body, "The network needs a password");
    assert_eq!(
        retry.note,
        Some(Note::Error("That password didn't work".to_owned()))
    );
    assert_eq!(retry.confirm.as_deref(), Some("Try again"));
}

#[test]
fn vpn_shows_the_username_row_only_when_a_hint_asks_for_it() {
    let view = view_for(&Prompt::Secret(vpn(&[])), None);
    assert_eq!(view.icon, Icon::Vpn);
    assert_eq!(view.title, "Sign in to “Work VPN”");
    assert_eq!(view.username, None);
    assert_eq!(view.autoconnect, None);
    assert_eq!(view.confirm.as_deref(), Some("Connect"));

    let view = view_for(&Prompt::Secret(vpn(&["username", "password"])), None);
    assert_eq!(view.username.as_deref(), Some(""));
    assert_eq!(view.secret, Some(("Password".to_owned(), Field::Password)));
}

#[test]
fn bluetooth_views_follow_the_request_kind() {
    let compare = view_for(&pairing("Pixel 9", Kind::Confirm { passkey: 4_913 }), None);
    assert_eq!(compare.title, "Pair with “Pixel 9”?");
    assert_eq!(compare.code.as_deref(), Some("004 913"));
    assert_eq!(compare.confirm.as_deref(), Some("Pair"));
    assert_eq!(compare.secret, None);

    let pin = view_for(&pairing("Pixel 9", Kind::Pin), None);
    assert_eq!(pin.secret, Some(("PIN".to_owned(), Field::Pin)));
    let passkey = view_for(&pairing("Pixel 9", Kind::Passkey), None);
    assert_eq!(passkey.secret, Some(("Code".to_owned(), Field::Passkey)));

    let show = view_for(
        &pairing(
            "MX Keys",
            Kind::ShowPasskey {
                passkey: 730_519,
                entered: 2,
            },
        ),
        None,
    );
    assert_eq!(show.title, "Pair with “MX Keys”");
    assert_eq!(show.code.as_deref(), Some("730 519"));
    assert_eq!(show.confirm, None);
    assert_eq!(show.cancel, "Cancel");

    let authorize = view_for(&pairing("", Kind::Authorize), None);
    assert_eq!(authorize.title, "“this device” wants to connect");
    assert_eq!(authorize.confirm.as_deref(), Some("Allow"));
    assert_eq!(authorize.cancel, "Deny");
}

#[test]
fn a_hostile_name_is_capped_to_one_plain_line() {
    let name = format!("<b>\u{202e}é\n{}", "x".repeat(4096));
    let view = view_for(&pairing(&name, Kind::Pin), None);
    assert!(view.title.chars().count() < 100);
    assert!(!view.title.contains('\n'));
    assert!(!view.title.contains('\u{202e}'));
}

#[test]
fn network_answers_carry_the_secret_and_cancel_refuses() {
    let network::Answer::Secret {
        secret,
        username,
        autoconnect,
    } = network_answer(&vpn(&["username"]), confirm("hunter22"))
    else {
        panic!("confirm did not answer with a secret");
    };
    assert_eq!(secret.as_str(), "hunter22");
    assert_eq!(username.as_deref(), Some("alex"));
    assert!(autoconnect);

    let network::Answer::Secret { username, .. } = network_answer(&wifi(false), confirm("x"))
    else {
        panic!("confirm did not answer with a secret");
    };
    assert_eq!(username, None);
    assert!(matches!(
        network_answer(&wifi(false), Response::Cancel),
        network::Answer::Refused
    ));
}

#[test]
fn bluetooth_answers_parse_the_secret_by_kind() {
    let compare = Kind::Confirm { passkey: 1 };
    assert_eq!(
        bluetooth_answer(&compare, confirm("")),
        bluetooth::Answer::Confirm
    );
    assert_eq!(
        bluetooth_answer(&Kind::Authorize, confirm("")),
        bluetooth::Answer::Confirm
    );
    assert_eq!(
        bluetooth_answer(&compare, Response::Cancel),
        bluetooth::Answer::Deny
    );
    assert_eq!(
        bluetooth_answer(&Kind::Pin, confirm("0000")),
        bluetooth::Answer::Pin("0000".to_owned())
    );
    assert_eq!(
        bluetooth_answer(&Kind::Passkey, confirm("123456")),
        bluetooth::Answer::Passkey(123_456)
    );
    assert_eq!(
        bluetooth_answer(&Kind::Passkey, confirm("1000000")),
        bluetooth::Answer::Deny
    );
}

fn queued_wifi(key: &str) -> (Ask, oneshot::Receiver<network::Answer>) {
    let (reply, receiver) = oneshot::channel();
    let ask = Ask {
        source: Source::Network,
        key: key.to_owned(),
        prompt: Prompt::Secret(wifi(false)),
        reply: Reply::Secret(reply),
    };
    (ask, receiver)
}

fn still_waiting(receiver: &mut oneshot::Receiver<network::Answer>) {
    assert_eq!(receiver.try_recv().unwrap_err(), TryRecvError::Empty);
}

#[test]
fn a_response_for_another_request_is_dropped_and_the_shown_one_is_answered_once() {
    let mut queue = Queue::default();
    let (shown, mut shown_answer) = queued_wifi("shown");
    let (next, mut next_answer) = queued_wifi("next");
    queue.push(shown);
    queue.push(next);

    assert!(matches!(
        route(&mut queue, Source::Network, "next", Response::Cancel),
        Routed::Dropped
    ));
    still_waiting(&mut shown_answer);
    still_waiting(&mut next_answer);
    assert_eq!(queue.shown().map(|ask| ask.key.as_str()), Some("shown"));

    assert!(matches!(
        route(&mut queue, Source::Network, "shown", Response::Cancel),
        Routed::Sent
    ));
    assert!(matches!(
        shown_answer.try_recv(),
        Ok(network::Answer::Refused)
    ));
    still_waiting(&mut next_answer);

    assert!(matches!(
        route(&mut queue, Source::Network, "shown", Response::Cancel),
        Routed::Dropped
    ));
    still_waiting(&mut next_answer);
    assert_eq!(queue.shown().map(|ask| ask.key.as_str()), Some("next"));

    assert!(matches!(
        route(&mut queue, Source::Network, "next", confirm("hunter22")),
        Routed::Sent
    ));
    match next_answer.try_recv() {
        Ok(network::Answer::Secret { secret, .. }) => assert_eq!(secret.as_str(), "hunter22"),
        Ok(network::Answer::Refused) => panic!("the matching response refused the network"),
        Err(error) => panic!("the matching response sent nothing: {error}"),
    }
    assert!(matches!(
        route(&mut queue, Source::Network, "next", confirm("again")),
        Routed::Dropped
    ));
    assert!(queue.shown().is_none());
}

#[test]
fn a_polkit_response_reaches_the_seam_again_without_retiring_the_head() {
    let mut queue = Queue::default();
    let (reply, mut waiting) = oneshot::channel();
    queue.push(Ask {
        source: Source::Polkit,
        key: "cookie".to_owned(),
        prompt: Prompt::Polkit(polkit_request()),
        reply: Reply::Polkit(reply),
    });

    assert!(matches!(
        route(&mut queue, Source::Network, "cookie", Response::Cancel),
        Routed::Dropped
    ));
    assert!(matches!(
        route(&mut queue, Source::Polkit, "cookie", Response::Cancel),
        Routed::Polkit(_)
    ));
    assert!(matches!(
        route(&mut queue, Source::Polkit, "other", Response::Cancel),
        Routed::Dropped
    ));
    assert!(matches!(
        route(&mut queue, Source::Polkit, "cookie", confirm("hunter22")),
        Routed::Polkit(_)
    ));
    assert_eq!(waiting.try_recv().unwrap_err(), TryRecvError::Empty);
    assert_eq!(queue.shown().map(|ask| ask.key.as_str()), Some("cookie"));
}

#[test]
fn a_secret_never_reaches_the_log_or_a_debug_line() {
    let secrets = [
        format!("{:?}", polkit_request()),
        format!(
            "{:?}",
            network::Answer::Secret {
                secret: Zeroizing::new("hunter22".to_owned()),
                username: None,
                autoconnect: true,
            }
        ),
        format!("{:?}", bluetooth::Answer::Pin("8642".to_owned())),
        format!("{:?}", bluetooth::Answer::Passkey(975_310)),
        format!(
            "{:?}",
            Input::Dialog {
                source: Source::Network,
                key: "3-c0ffee-1-deadbeef".to_owned(),
                response: confirm("hunter22"),
            }
        ),
        format!(
            "{:?}",
            Input::Withdraw {
                source: Source::Polkit,
                key: Some("3-c0ffee-1-deadbeef".to_owned()),
            }
        ),
    ]
    .join("\n");
    for secret in ["3-c0ffee-1-deadbeef", "hunter22", "8642", "975310"] {
        assert!(!secrets.contains(secret), "{secret} leaked: {secrets}");
    }
}

#[test]
fn dialog_blur_follows_the_configured_boolean() {
    let mut config = glimpse_config::Config::default();
    assert!(!super::blurred(&config));
    config.appearance.blur = true;
    assert!(super::blurred(&config));
}

fn network_ask(key: &str) -> (Ask, oneshot::Receiver<network::Answer>) {
    let (reply, answer) = oneshot::channel();
    (
        Ask {
            source: Source::Network,
            key: key.to_owned(),
            prompt: Prompt::Secret(wifi(false)),
            reply: Reply::Secret(reply),
        },
        answer,
    )
}

fn session_request(
    action: crate::session::Action,
) -> (Input, oneshot::Receiver<zbus::fdo::Result<()>>) {
    let (reply, answer) = oneshot::channel();
    (
        Input::SessionRequest(crate::session::Request { action, reply }),
        answer,
    )
}

#[tokio::test]
async fn session_confirmation_cancellation_auth_precedence_and_stale_responses_never_execute() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let server = bus.connection().await;
    let backend = crate::session::tests::backend(&server).await;
    let (sender, _receiver) = relm4::channel();
    let mut app = super::App::headless(
        tokio::runtime::Handle::current(),
        sender.clone(),
        bus.connection().await,
        None,
    );
    for cancellation in 0..5 {
        app.drive(Input::Locked(false), &sender);
        let (request, mut answer) = session_request(crate::session::Action::Reboot);
        app.drive(request, &sender);
        assert!(answer.try_recv().expect("admission").is_ok());
        let id = app.session.pending.as_ref().expect("pending").id;
        app.drive(
            Input::SessionReady {
                id,
                view: Ok(super::view_for(&Prompt::Secret(wifi(false)), None)),
            },
            &sender,
        );
        match cancellation {
            0 => app.drive(
                Input::SessionResponse {
                    id,
                    response: Response::Cancel,
                },
                &sender,
            ),
            1 => app.drive(Input::Locked(true), &sender),
            2 => app.drive(Input::LockUnknown, &sender),
            3 => {
                app.drive(Input::SessionFocus { id, active: true }, &sender);
                app.drive(Input::SessionFocus { id, active: false }, &sender);
            }
            _ => app.drive(Input::SessionFocusTimeout(id), &sender),
        }
        assert!(app.session.pending.is_none());
        app.drive(
            Input::SessionResponse {
                id,
                response: confirm(""),
            },
            &sender,
        );
        assert!(!app.session.executing);
    }
    app.drive(Input::Locked(false), &sender);
    let (request, mut answer) = session_request(crate::session::Action::PowerOff);
    app.drive(request, &sender);
    assert!(answer.try_recv().expect("admission").is_ok());
    let id = app.session.pending.as_ref().expect("pending").id;
    let (ask, _answer) = network_ask("authentication");
    app.drive(Input::Ask(ask), &sender);
    assert!(app.session.pending.is_none());
    assert!(app.showing());
    let (request, mut answer) = session_request(crate::session::Action::PowerOff);
    app.drive(request, &sender);
    assert!(matches!(
        answer.try_recv(),
        Ok(Err(zbus::fdo::Error::LimitsExceeded(_)))
    ));
    app.drive(
        Input::SessionResponse {
            id,
            response: confirm(""),
        },
        &sender,
    );
    app.drive(
        Input::SessionReady {
            id,
            view: Ok(super::view_for(&Prompt::Secret(wifi(false)), None)),
        },
        &sender,
    );
    app.drive(
        Input::Dialog {
            source: Source::Network,
            key: "authentication".into(),
            response: Response::Cancel,
        },
        &sender,
    );
    assert!(app.session.pending.is_none());
    assert!(!app.session.executing);
    assert!(backend.calls.lock().expect("calls").is_empty());
}

#[tokio::test]
async fn session_confirm_consumes_the_slot_and_executes_once_until_completion() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let server = bus.connection().await;
    let backend = crate::session::tests::backend(&server).await;
    let (sender, receiver) = relm4::channel();
    let mut app = super::App::headless(
        tokio::runtime::Handle::current(),
        sender.clone(),
        bus.connection().await,
        None,
    );
    app.drive(Input::Locked(false), &sender);
    let (request, mut answer) = session_request(crate::session::Action::PowerOff);
    app.drive(request, &sender);
    assert!(answer.try_recv().expect("admission").is_ok());
    let id = app.session.pending.as_ref().expect("pending").id;
    let ready = tokio::time::timeout(std::time::Duration::from_secs(3), receiver.recv())
        .await
        .expect("prepare deadline")
        .expect("ready");
    app.drive(ready, &sender);
    assert!(
        app.session
            .pending
            .as_ref()
            .expect("pending")
            .view
            .is_some()
    );
    assert!(backend.calls.lock().expect("calls").is_empty());
    app.drive(
        Input::SessionResponse {
            id,
            response: confirm(""),
        },
        &sender,
    );
    assert!(app.session.pending.is_none());
    assert!(app.session_surface.is_none());
    assert!(app.session.executing);
    app.drive(
        Input::SessionResponse {
            id,
            response: confirm(""),
        },
        &sender,
    );
    let (request, mut answer) = session_request(crate::session::Action::Reboot);
    app.drive(request, &sender);
    assert!(matches!(
        answer.try_recv(),
        Ok(Err(zbus::fdo::Error::LimitsExceeded(_)))
    ));
    let done = tokio::time::timeout(std::time::Duration::from_secs(3), receiver.recv())
        .await
        .expect("execution deadline")
        .expect("done");
    app.drive(done, &sender);
    assert!(!app.session.executing);
    assert_eq!(
        *backend.calls.lock().expect("calls"),
        [("power-off".into(), true)]
    );
    app.drive(Input::SessionDone(Err("failure".into())), &sender);
    assert!(!app.session.executing);
}

#[test]
#[ignore = "needs a display"]
fn session_overlay_is_destroyed_before_a_confirmed_action() {
    use adw::prelude::*;
    gtk4::init().expect("display");
    adw::init().expect("libadwaita");
    crate::dialog::register_resources().expect("resources");
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let (sender, receiver) = relm4::channel();
    let mut app = super::App::headless(
        runtime.handle().clone(),
        sender.clone(),
        runtime.block_on(bus.connection()),
        None,
    );
    app.drive(Input::Locked(false), &sender);
    let (request, mut answer) = session_request(crate::session::Action::Reboot);
    app.drive(request, &sender);
    assert!(answer.try_recv().expect("admission").is_ok());
    let id = app.session.pending.as_ref().expect("pending").id;
    app.drive(
        Input::SessionReady {
            id,
            view: Ok(super::view_for(&Prompt::Secret(wifi(false)), None)),
        },
        &sender,
    );
    let window = gtk4::Window::new();
    let dialog = crate::dialog::AgentDialog::new();
    window.present();
    app.session_surface = Some(super::Surface::new(window.clone(), dialog.clone()));
    app.drive(
        Input::SessionResponse {
            id,
            response: confirm(""),
        },
        &sender,
    );
    assert!(app.session.executing);
    assert!(app.session_surface.is_none());
    assert!(!window.is_visible());
    assert!(!dialog.is_sensitive());
    assert!(app.session.pending.is_none());
    drop(receiver);
}

#[tokio::test]
async fn headless_dismissal_answers_without_a_glib_timer() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let (sender, _receiver) = relm4::channel();
    let mut app = super::App::headless(
        tokio::runtime::Handle::current(),
        sender.clone(),
        bus.connection().await,
        None,
    );
    let (ask, mut answer) = network_ask("headless");
    app.drive(Input::Ask(ask), &sender);
    app.drive(
        Input::Dialog {
            source: Source::Network,
            key: "headless".to_owned(),
            response: confirm("hunter22"),
        },
        &sender,
    );
    assert!(matches!(
        answer.try_recv(),
        Ok(network::Answer::Secret { .. })
    ));
    assert!(!app.showing());
    assert!(app.surface.is_none());
    assert!(app.closing.is_none());
}

#[test]
#[ignore = "needs a display"]
fn dismissal_animates_without_delaying_replies_or_queue_handoffs() {
    use adw::prelude::*;
    use gtk4::glib;

    gtk4::init().expect("display");
    adw::init().expect("libadwaita");
    crate::dialog::register_resources().expect("agent resources");
    let css = gtk4::CssProvider::new();
    css.connect_parsing_error(|_, section, error| {
        panic!("shared stylesheet {}: {error}", section.to_str());
    });
    css.load_from_string(include_str!("../../../glimpse-widgets/styles/glimpse.css"));
    let styles = glimpse_widgets::Styles::install(adw::ColorScheme::Default);
    styles.set_animation_speed(0.5);
    let settings = gtk4::Settings::default().expect("settings");
    settings.set_gtk_enable_animations(true);
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let (sender, receiver) = relm4::channel();
    let mut app = super::App::headless(
        runtime.handle().clone(),
        sender.clone(),
        runtime.block_on(bus.connection()),
        None,
    );
    app.headless = false;
    let window = gtk4::Window::new();
    window.set_decorated(false);
    window.add_css_class("agent-surface");
    window.set_default_size(1000, 800);
    app.surface = Some(super::Surface::new(
        window.clone(),
        crate::dialog::AgentDialog::new(),
    ));
    let (first, mut first_answer) = network_ask("first");
    app.drive(Input::Ask(first), &sender);
    let backdrop = app.surface.as_ref().expect("surface").backdrop.clone();
    assert_eq!(window.child().as_ref(), Some(backdrop.upcast_ref()));
    assert!(backdrop.has_css_class("agent-backdrop"));
    assert!(backdrop.hexpands() && backdrop.vexpands());
    let context = glib::MainContext::default();
    context.block_on(glib::timeout_future(std::time::Duration::from_millis(450)));
    let dialog = &app.surface.as_ref().expect("surface").dialog;
    let bounds = dialog.compute_bounds(&backdrop).expect("dialog bounds");
    assert!((bounds.x() + bounds.width() / 2.0 - backdrop.width() as f32 / 2.0).abs() < 1.0);
    assert!((bounds.y() + bounds.height() / 2.0 - backdrop.height() as f32 / 2.0).abs() < 1.0);
    assert_eq!(backdrop.width(), window.width());
    assert_eq!(backdrop.height(), window.height());

    let (next, mut next_answer) = network_ask("next");
    app.drive(Input::Ask(next), &sender);
    app.drive(
        Input::Dialog {
            source: Source::Network,
            key: "first".to_owned(),
            response: confirm("hunter22"),
        },
        &sender,
    );
    assert!(matches!(
        first_answer.try_recv(),
        Ok(network::Answer::Secret { .. })
    ));
    assert_eq!(app.surface.as_ref().expect("surface").window, window);
    assert_eq!(app.surface.as_ref().expect("surface").backdrop, backdrop);
    assert!(app.closing.is_none());
    assert!(!window.has_css_class("closing"));

    let dialog = app.surface.as_ref().expect("surface").dialog.clone();
    app.drive(
        Input::Dialog {
            source: Source::Network,
            key: "next".to_owned(),
            response: Response::Cancel,
        },
        &sender,
    );
    assert!(matches!(
        next_answer.try_recv(),
        Ok(network::Answer::Refused)
    ));
    assert!(app.surface.is_none());
    assert!(window.has_css_class("closing"));
    assert!(!dialog.is_sensitive());
    assert!(window.is_visible());
    dialog.emit_by_name::<()>("response", &[&false]);
    assert!(
        runtime
            .block_on(async {
                tokio::time::timeout(std::time::Duration::from_millis(10), receiver.recv()).await
            })
            .is_err()
    );
    context.block_on(glib::timeout_future(std::time::Duration::from_millis(180)));
    assert!(window.is_visible());
    context.block_on(glib::timeout_future(std::time::Duration::from_millis(180)));
    assert!(!window.is_visible());

    let replacement = gtk4::Window::new();
    replacement.add_css_class("agent-surface");
    app.surface = Some(super::Surface::new(
        replacement.clone(),
        crate::dialog::AgentDialog::new(),
    ));
    let (ask, _answer) = network_ask("replacement");
    app.drive(Input::Ask(ask), &sender);
    assert!(app.closing.is_none());
    let dialog = app.surface.as_ref().expect("surface").dialog.clone();
    app.drive(
        Input::Withdraw {
            source: Source::Network,
            key: Some("replacement".to_owned()),
        },
        &sender,
    );
    assert!(!dialog.is_sensitive());
    assert!(replacement.is_visible());

    let fresh = gtk4::Window::new();
    fresh.add_css_class("agent-surface");
    app.surface = Some(super::Surface::new(
        fresh.clone(),
        crate::dialog::AgentDialog::new(),
    ));
    let (ask, _answer) = network_ask("fresh");
    app.drive(Input::Ask(ask), &sender);
    assert!(!replacement.is_visible());
    assert!(app.closing.is_none());
    assert!(!fresh.has_css_class("closing"));
    context.block_on(glib::timeout_future(std::time::Duration::from_millis(350)));
    assert!(fresh.is_visible());

    styles.set_animation_speed(0.0);
    app.close();
    assert!(!fresh.is_visible());
    assert!(app.closing.is_none());
    let unanimated = gtk4::Window::new();
    unanimated.present();
    app.surface = Some(super::Surface::new(
        unanimated.clone(),
        crate::dialog::AgentDialog::new(),
    ));
    styles.set_animation_speed(1.0);
    settings.set_gtk_enable_animations(false);
    app.close();
    assert!(!unanimated.is_visible());
    assert!(app.closing.is_none());
}
