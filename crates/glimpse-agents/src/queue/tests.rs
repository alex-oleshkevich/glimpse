use tokio::sync::oneshot;

use super::Queue;
use crate::ask::{Ask, Prompt, Reply, Source};
use crate::{bluetooth, network, polkit};

fn polkit_ask(cookie: &str) -> (Ask, oneshot::Receiver<polkit::Outcome>) {
    let (reply, receiver) = oneshot::channel();
    let ask = Ask {
        source: Source::Polkit,
        key: cookie.to_owned(),
        prompt: Prompt::Polkit(polkit::Request {
            action_id: "org.freedesktop.policykit.exec".to_owned(),
            message: "Run a program as another user".to_owned(),
            details: Vec::new(),
            cookie: cookie.to_owned(),
            identities: Vec::new(),
        }),
        reply: Reply::Polkit(reply),
    };
    (ask, receiver)
}

fn network_ask(path: &str) -> (Ask, oneshot::Receiver<network::Answer>) {
    let (reply, receiver) = oneshot::channel();
    let ask = Ask {
        source: Source::Network,
        key: format!("{path}:802-11-wireless-security"),
        prompt: Prompt::Secret(network::Request {
            name: "Café".to_owned(),
            path: path.to_owned(),
            setting: "802-11-wireless-security".to_owned(),
            retry: false,
            hints: Vec::new(),
            autoconnect: true,
        }),
        reply: Reply::Secret(reply),
    };
    (ask, receiver)
}

fn pairing(device: &str, kind: bluetooth::Kind) -> bluetooth::Request {
    bluetooth::Request {
        device: device.to_owned(),
        name: "Pixel".to_owned(),
        kind,
    }
}

fn bluetooth_ask(device: &str) -> (Ask, oneshot::Receiver<bluetooth::Answer>) {
    let (reply, receiver) = oneshot::channel();
    let ask = Ask {
        source: Source::Bluetooth,
        key: device.to_owned(),
        prompt: Prompt::Pairing(pairing(device, bluetooth::Kind::Confirm { passkey: 1 })),
        reply: Reply::Pairing(reply),
    };
    (ask, receiver)
}

fn shown_ask(device: &str, entered: u16) -> Ask {
    Ask {
        source: Source::Bluetooth,
        key: device.to_owned(),
        prompt: Prompt::Pairing(pairing(
            device,
            bluetooth::Kind::ShowPasskey {
                passkey: 482_913,
                entered,
            },
        )),
        reply: Reply::Shown,
    }
}

fn head_key(queue: &Queue) -> Option<&str> {
    queue.shown().map(|ask| ask.key.as_str())
}

#[test]
fn requests_are_answered_one_at_a_time_in_arrival_order() {
    let mut queue = Queue::default();
    let (polkit, mut polkit_answer) = polkit_ask("cookie");
    let (wifi, mut wifi_answer) = network_ask("/nm/1");
    let (pair, mut pair_answer) = bluetooth_ask("/bluez/dev");
    queue.push(polkit);
    queue.push(wifi);
    queue.push(pair);

    assert_eq!(head_key(&queue), Some("cookie"));
    assert!(wifi_answer.try_recv().is_err());
    let Some(Ask {
        reply: Reply::Polkit(reply),
        ..
    }) = queue.take_head()
    else {
        panic!("the polkit request is not the head");
    };
    reply.send(polkit::Outcome::Authorized).unwrap();
    assert_eq!(polkit_answer.try_recv(), Ok(polkit::Outcome::Authorized));

    assert_eq!(head_key(&queue), Some("/nm/1:802-11-wireless-security"));
    assert!(matches!(
        pair_answer.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    drop(queue.take_head());
    assert!(wifi_answer.try_recv().is_err());

    assert_eq!(head_key(&queue), Some("/bluez/dev"));
    drop(queue.take_head());
    assert_eq!(head_key(&queue), None);
}

#[test]
fn a_source_cancel_drops_its_request_and_advances_the_queue() {
    let mut queue = Queue::default();
    let (first, mut first_answer) = polkit_ask("first");
    let (second, mut second_answer) = polkit_ask("second");
    let (third, _third_answer) = polkit_ask("third");
    queue.push(first);
    queue.push(second);
    queue.push(third);

    queue.withdraw(Source::Polkit, Some("second"));
    assert!(matches!(
        second_answer.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    ));
    assert_eq!(head_key(&queue), Some("first"));

    queue.withdraw(Source::Polkit, Some("first"));
    assert!(matches!(
        first_answer.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    ));
    assert_eq!(head_key(&queue), Some("third"));
}

#[test]
fn a_keyless_withdraw_drops_every_request_of_its_source() {
    let mut queue = Queue::default();
    let (pair, mut pair_answer) = bluetooth_ask("/bluez/a");
    let (wifi, _wifi_answer) = network_ask("/nm/1");
    queue.push(pair);
    queue.push(shown_ask("/bluez/b", 0));
    queue.push(wifi);

    queue.withdraw(Source::Bluetooth, None);
    assert!(matches!(
        pair_answer.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    ));
    assert_eq!(head_key(&queue), Some("/nm/1:802-11-wireless-security"));
    drop(queue.take_head());
    assert!(queue.head().is_none());
}

#[test]
fn a_withdrawn_request_can_never_be_answered_late() {
    let mut queue = Queue::default();
    let (ask, mut answer) = polkit_ask("cookie");
    queue.push(ask);
    queue.withdraw(Source::Polkit, Some("cookie"));

    assert!(queue.take_head().is_none());
    assert!(matches!(
        answer.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    ));
}

#[test]
fn a_repeated_show_replaces_its_entry_in_place() {
    let mut queue = Queue::default();
    queue.push(shown_ask("/bluez/dev", 0));
    let (wifi, _wifi_answer) = network_ask("/nm/1");
    queue.push(wifi);
    queue.push(shown_ask("/bluez/dev", 3));

    let Some(Prompt::Pairing(request)) = queue.shown().map(|ask| &ask.prompt) else {
        panic!("the show-view is not the head");
    };
    assert_eq!(
        request.kind,
        bluetooth::Kind::ShowPasskey {
            passkey: 482_913,
            entered: 3
        }
    );
    drop(queue.take_head());
    assert_eq!(head_key(&queue), Some("/nm/1:802-11-wireless-security"));
    drop(queue.take_head());
    assert!(queue.head().is_none());
}

#[test]
fn a_locked_session_refuses_wireless_and_bluetooth_at_once() {
    let mut queue = Queue::default();
    let (queued_wifi, mut queued_wifi_answer) = network_ask("/nm/0");
    queue.push(queued_wifi);
    queue.push(shown_ask("/bluez/shown", 0));

    queue.lock(true);
    assert!(matches!(
        queued_wifi_answer.try_recv(),
        Ok(network::Answer::Refused)
    ));
    assert!(queue.head().is_none());

    let (wifi, mut wifi_answer) = network_ask("/nm/1");
    let (pair, mut pair_answer) = bluetooth_ask("/bluez/dev");
    queue.push(wifi);
    queue.push(pair);
    queue.push(shown_ask("/bluez/other", 0));
    assert!(matches!(
        wifi_answer.try_recv(),
        Ok(network::Answer::Refused)
    ));
    assert_eq!(pair_answer.try_recv(), Ok(bluetooth::Answer::Deny));
    assert!(queue.head().is_none());

    queue.lock(false);
    let (wifi, mut wifi_answer) = network_ask("/nm/2");
    queue.push(wifi);
    assert_eq!(head_key(&queue), Some("/nm/2:802-11-wireless-security"));
    assert!(matches!(
        wifi_answer.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
}

#[test]
fn a_dropped_ask_closes_its_reply() {
    let mut queue = Queue::default();
    let (ask, answer) = bluetooth_ask("/bluez/dev");
    queue.push(ask);
    drop(queue);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    assert!(runtime.block_on(answer).is_err());
}
