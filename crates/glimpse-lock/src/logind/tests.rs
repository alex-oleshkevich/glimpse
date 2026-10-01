use super::*;

#[test]
fn only_a_release_drops_the_inhibitor() {
    let mut held = Some(7);
    assert_eq!(
        step(&mut held, Request::SetLockedHint(true)),
        Some(Call::SetLockedHint(true))
    );
    assert_eq!(held, Some(7));
    assert_eq!(
        step(&mut held, Request::TakeInhibitor),
        None,
        "one is held already"
    );
    assert_eq!(held, Some(7));
    assert_eq!(step(&mut held, Request::ReleaseInhibitor), None);
    assert_eq!(held, None);
    assert_eq!(step(&mut held, Request::ReleaseInhibitor), None);
    assert_eq!(step(&mut held, Request::TakeInhibitor), Some(Call::Inhibit));
}

#[test]
fn session_requests_pass_straight_through() {
    let mut held: Option<i32> = None;
    assert_eq!(
        step(&mut held, Request::FetchSessionActions),
        Some(Call::FetchSessionActions)
    );
    assert_eq!(
        step(&mut held, Request::PerformSessionAction(Action::Suspend, 7)),
        Some(Call::PerformSessionAction(Action::Suspend, 7))
    );
    assert_eq!(held, None, "a session request never touches the inhibitor");
}
