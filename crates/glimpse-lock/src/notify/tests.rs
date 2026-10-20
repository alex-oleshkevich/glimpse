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

async fn restore_with(value: bool) -> Option<Notice> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let listener = tokio::net::UnixListener::bind(dir.path().join(".socket.sock")).expect("bind");
    tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut command = vec![0_u8; 256];
            let _ = stream.read(&mut command).await;
            let reply = format!(r#"{{"option":"misc:allow_session_lock_restore","bool":{value}}}"#);
            let _ = stream.write_all(reply.as_bytes()).await;
        }
    });
    restore_notice(&glimpse_compositors::Compositor::Hyprland(
        glimpse_compositors::Hyprland::at(dir.path()),
    ))
    .await
}

#[tokio::test]
async fn a_disabled_restore_option_posts_one_warning_and_an_enabled_one_none() {
    assert_eq!(restore_with(false).await, Some(Notice::RestoreDisabled));
    assert_eq!(restore_with(true).await, None);
    let (_, body) = text(Notice::RestoreDisabled);
    assert!(body.contains("misc:allow_session_lock_restore"));
}
