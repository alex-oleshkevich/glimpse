use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;

use super::{Attempt, Line, parse};

#[test]
fn parse_reads_each_helper_line() {
    assert_eq!(parse("SUCCESS"), Some(Line::Success));
    assert_eq!(parse("FAILURE\r"), Some(Line::Failure));
    assert_eq!(
        parse("PAM_PROMPT_ECHO_OFF Password:"),
        Some(Line::Prompt {
            text: "Password:".to_owned(),
            echo: false,
        })
    );
    assert_eq!(
        parse("PAM_PROMPT_ECHO_ON Code:"),
        Some(Line::Prompt {
            text: "Code:".to_owned(),
            echo: true,
        })
    );
    assert_eq!(
        parse("PAM_ERROR_MSG denied"),
        Some(Line::Error("denied".to_owned()))
    );
    assert_eq!(
        parse(r"PAM_TEXT_INFO Place\040your\040finger"),
        Some(Line::Info("Place your finger".to_owned()))
    );
    assert_eq!(
        parse(r"PAM_TEXT_INFO a\\b"),
        Some(Line::Info("a\\b".to_owned()))
    );
    assert_eq!(
        parse(r"PAM_TEXT_INFO a\nb"),
        Some(Line::Info("a\nb".to_owned()))
    );
    assert_eq!(
        parse(r"PAM_TEXT_INFO a\tb"),
        Some(Line::Info("a\tb".to_owned()))
    );
    assert_eq!(
        parse(r"PAM_TEXT_INFO \b"),
        Some(Line::Info("\u{8}".to_owned()))
    );
    assert_eq!(
        parse(r"PAM_TEXT_INFO \f"),
        Some(Line::Info("\u{c}".to_owned()))
    );
    assert_eq!(
        parse(r"PAM_TEXT_INFO \v"),
        Some(Line::Info("\u{b}".to_owned()))
    );
    assert_eq!(
        parse("PAM_TEXT_INFO trail\\"),
        Some(Line::Info("trail".to_owned()))
    );
    assert_eq!(parse("NOPE"), None);
}

#[tokio::test]
async fn an_unreadable_line_is_rejected() {
    let (client, mut server) = UnixStream::pair().expect("a socket pair");
    server.write_all(b"NOT A LINE\n").await.expect("write");
    let mut attempt = Attempt::socket(client, "alice", "cookie")
        .await
        .expect("handshake");
    let error = attempt.next().await.expect_err("unreadable");
    assert_eq!(error.kind(), ErrorKind::InvalidData);
}

#[tokio::test]
async fn a_multiline_token_is_rejected() {
    let error = Attempt::start("a\nb", "cookie", Path::new("/no/such/polkit-socket"), &[]).await;
    let Err(error) = error else {
        panic!("a token with a newline must be rejected");
    };
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
}

#[tokio::test]
async fn dropping_an_attempt_kills_the_spawned_helper() {
    let scratch = Scratch::new();
    let helper = scratch.path().join("helper");
    let pidfile = scratch.path().join("pid");
    script(
        &helper,
        &format!(
            "#!/bin/sh\nread line\nprintf '%s\\n' \"$$\" > '{}'\nexec sleep 30\n",
            pidfile.display()
        ),
    );
    let attempt = Attempt::spawned(&helper, "alice", "cookie")
        .await
        .expect("spawn");
    let pid = wait_text(&pidfile).await;
    drop(attempt);
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        if dead(&pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("helper {pid} survived drop");
}

fn dead(pid: &str) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => true,
        Ok(stat) => stat
            .rsplit(')')
            .next()
            .is_some_and(|rest| rest.trim_start().starts_with('Z')),
    }
}

async fn wait_text(path: &Path) -> String {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        if let Ok(text) = std::fs::read_to_string(path) {
            let text = text.trim();
            if !text.is_empty() {
                return text.to_owned();
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("helper did not write {}", path.display());
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "glimpse-polkit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
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
