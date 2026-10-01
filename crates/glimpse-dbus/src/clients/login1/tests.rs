use super::select_seat_session_candidate;
use super::{SessionCandidate, select_session_candidate};
use zbus::zvariant::OwnedObjectPath;

#[test]
fn select_session_candidate_prefers_active_user_wayland_session() {
    let candidates = vec![
        candidate("1", 1000, false, Some("user"), Some("wayland"), "seat0"),
        candidate("2", 1000, true, Some("user"), Some("wayland"), "seat0"),
        candidate("3", 1001, true, Some("user"), Some("wayland"), "seat0"),
    ];

    assert_eq!(
        select_session_candidate(&candidates, 1000).map(|candidate| candidate.id.as_str()),
        Some("2")
    );
}

#[test]
fn select_session_candidate_prefers_graphical_session_over_active_tty() {
    let candidates = vec![
        candidate("1", 1000, true, Some("user"), Some("tty"), "seat0"),
        candidate("2", 1000, false, Some("user"), Some("wayland"), "seat0"),
    ];

    assert_eq!(
        select_session_candidate(&candidates, 1000).map(|candidate| candidate.id.as_str()),
        Some("2")
    );
}

#[test]
fn select_session_candidate_ignores_other_users() {
    let candidates = vec![candidate(
        "1",
        1001,
        true,
        Some("user"),
        Some("wayland"),
        "seat0",
    )];

    assert!(select_session_candidate(&candidates, 1000).is_none());
}

#[test]
fn select_seat_session_candidate_skips_no_seat_sessions() {
    let candidates = vec![
        candidate("1", 1000, true, Some("user"), Some("wayland"), ""),
        candidate("2", 1000, false, Some("user"), Some("tty"), "seat0"),
    ];

    assert_eq!(
        select_seat_session_candidate(&candidates, 1000).map(|candidate| candidate.id.as_str()),
        Some("2")
    );
}

#[test]
fn select_seat_session_candidate_returns_none_without_a_seat() {
    let candidates = vec![candidate(
        "1",
        1000,
        true,
        Some("manager"),
        Some("unspecified"),
        "",
    )];

    assert!(select_seat_session_candidate(&candidates, 1000).is_none());
}

fn candidate(
    id: &str,
    uid: u32,
    active: bool,
    class: Option<&str>,
    kind: Option<&str>,
    seat: &str,
) -> SessionCandidate {
    SessionCandidate {
        id: id.into(),
        uid,
        seat: seat.into(),
        path: OwnedObjectPath::try_from(format!("/org/freedesktop/login1/session/_{id}")).unwrap(),
        active,
        class: class.map(str::to_owned),
        kind: kind.map(str::to_owned),
    }
}

#[test]
fn login1_inhibitor_entry_tuple_shape() {
    let e: super::Login1InhibitorEntry = (
        "idle".into(),
        "test".into(),
        "smoke".into(),
        "block".into(),
        1000,
        1234,
    );
    assert_eq!(e.0, "idle");
    assert_eq!(e.5, 1234);
}
