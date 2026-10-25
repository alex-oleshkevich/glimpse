use super::*;

#[test]
fn a_refusal_names_the_check() {
    let (summary, body) = text(Notice::NotLocked(Reason::CantVerify));
    assert_eq!(summary, "Screen not locked");
    assert!(body.contains("glimpse-lock check"));
    let (summary, _) = text(Notice::SuspendingUnlocked(Reason::CantVerify));
    assert_eq!(summary, "Suspending unlocked");
}

#[test]
fn a_safety_notice_is_critical_and_an_expired_password_is_not() {
    assert_eq!(urgency(Notice::NotLocked(Reason::LockFailed)), CRITICAL);
    assert_eq!(
        urgency(Notice::SuspendingUnlocked(Reason::CantVerify)),
        CRITICAL
    );
    assert_eq!(urgency(Notice::PasswordExpired), NORMAL);
    let (_, body) = text(Notice::PasswordExpired);
    assert!(body.contains("passwd"));
}

#[test]
fn a_disabled_restore_posts_once_when_the_provider_first_says_so() {
    let mut last = None;
    assert_eq!(
        restore_notice(&mut last, Some(false)),
        Some(Notice::RestoreDisabled)
    );
    let (_, body) = text(Notice::RestoreDisabled);
    assert!(body.contains("misc:allow_session_lock_restore"));
}

#[test]
fn a_repeated_false_posts_nothing_and_true_then_false_posts_again() {
    let mut last = Some(false);
    assert_eq!(restore_notice(&mut last, None), None);
    last = Some(false);
    assert_eq!(restore_notice(&mut last, Some(false)), None);
    last = Some(false);
    assert_eq!(restore_notice(&mut last, Some(true)), None);
    last = Some(true);
    assert_eq!(
        restore_notice(&mut last, Some(false)),
        Some(Notice::RestoreDisabled)
    );
}

#[test]
fn niri_and_an_enabled_restore_post_nothing() {
    let mut last = None;
    assert_eq!(restore_notice(&mut last, None), None);
    last = None;
    assert_eq!(restore_notice(&mut last, Some(true)), None);
    last = Some(true);
    assert_eq!(restore_notice(&mut last, Some(true)), None);
}

#[test]
fn a_provider_gap_keeps_the_last_false_and_posts_exactly_twice() {
    let mut last = None;
    let posted: Vec<Option<Notice>> = [
        None,
        Some(false),
        None,
        Some(false),
        Some(true),
        Some(false),
    ]
    .into_iter()
    .map(|now| restore_notice(&mut last, now))
    .collect();

    assert_eq!(
        posted,
        [
            None,
            Some(Notice::RestoreDisabled),
            None,
            None,
            None,
            Some(Notice::RestoreDisabled),
        ]
    );
}
