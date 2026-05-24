use glimpse_lock::{
    auth::SecretString,
    runtime::{APP_ID, GTK_APPLICATION_ID, GTK_PREVIEW_APPLICATION_ID, LockRuntime},
};

#[test]
fn app_ids_are_stable_and_distinct() {
    assert_eq!(APP_ID, "me.aresa.GlimpseLock");
    assert_eq!(GTK_APPLICATION_ID, "me.aresa.GlimpseLock.App");
    assert_eq!(GTK_PREVIEW_APPLICATION_ID, "me.aresa.GlimpseLock.Preview");
    assert_ne!(APP_ID, GTK_APPLICATION_ID);
    assert_ne!(APP_ID, GTK_PREVIEW_APPLICATION_ID);
    assert_ne!(GTK_APPLICATION_ID, GTK_PREVIEW_APPLICATION_ID);
}

#[test]
fn secret_debug_does_not_expose_password() {
    let secret = SecretString::new("correct horse battery staple");

    let debug = format!("{secret:?}");

    assert!(debug.contains("SecretString"));
    assert!(!debug.contains("correct"));
    assert!(!debug.contains("horse"));
    assert!(!debug.contains("battery"));
    assert!(!debug.contains("staple"));
}

#[test]
fn runtime_reset_clears_locked_and_auth_state() {
    let mut runtime = LockRuntime::default();

    runtime.mark_locked();
    runtime.mark_auth_success();
    assert!(runtime.can_unlock());

    runtime.reset();

    assert!(!runtime.can_unlock());
}

#[test]
fn unlock_is_allowed_only_after_lock_and_auth_success() {
    let mut runtime = LockRuntime::default();

    assert!(!runtime.can_unlock());
    runtime.mark_auth_success();
    assert!(!runtime.can_unlock());
    runtime.mark_locked();
    assert!(runtime.can_unlock());
}

#[test]
fn failed_auth_clears_pending_success() {
    let mut runtime = LockRuntime::default();

    runtime.mark_locked();
    runtime.mark_auth_success();
    runtime.clear_auth_success();

    assert!(!runtime.can_unlock());
}

/// The unlock invariant: `can_unlock()` requires BOTH the compositor's
/// `Locked` event (via `mark_locked`) AND a successful PAM authentication
/// (via `mark_auth_success`). The Unlocked handler in app.rs uses this to
/// distinguish a legitimate unlock from an unsolicited compositor release.
#[test]
fn can_unlock_requires_both_locked_and_auth_success() {
    let mut runtime = LockRuntime::default();
    assert!(!runtime.can_unlock(), "fresh runtime should not be unlockable");

    runtime.mark_locked();
    assert!(!runtime.can_unlock(), "locked-but-not-authed must not unlock");

    runtime.clear_auth_success();
    runtime.mark_locked();
    runtime.mark_auth_success();
    assert!(runtime.can_unlock(), "locked AND authed should be unlockable");

    runtime.reset();
    runtime.mark_auth_success();
    assert!(!runtime.can_unlock(), "authed-but-not-locked must not unlock");
}

/// `clear_auth_success` (called from the Failure / AccountUnavailable
/// handlers) must NOT clear the `locked` flag: a wrong password attempt
/// should leave the lock surface in place.
#[test]
fn clear_auth_success_preserves_locked_state() {
    let mut runtime = LockRuntime::default();
    runtime.mark_locked();
    runtime.mark_auth_success();

    runtime.clear_auth_success();

    // Still locked from the compositor's perspective: the lock surface
    // should stay up after a failed attempt.
    assert!(!runtime.can_unlock());
    runtime.mark_auth_success();
    assert!(runtime.can_unlock(), "re-auth should restore unlock capability");
}

#[tokio::test]
async fn test_single_instance_guard_rejects_second_owner() {
    let name = format!("me.aresa.GlimpseLock.Test{}", std::process::id());
    let _guard = LockRuntime::acquire_single_instance_for_testing(&name)
        .await
        .expect("first test guard should acquire name");

    let second = LockRuntime::acquire_single_instance_for_testing(&name).await;

    assert!(second.is_err());
}
