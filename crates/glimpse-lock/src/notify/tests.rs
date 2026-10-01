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
