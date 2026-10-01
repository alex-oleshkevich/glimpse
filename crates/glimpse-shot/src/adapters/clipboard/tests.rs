use super::*;

#[test]
fn a_notify_only_hold_asks_for_no_clipboard_and_carries_the_notice() {
    let notice = Notice {
        summary: "Screenshot failed".to_owned(),
        body: "no such output".to_owned(),
        image: None,
        actions: false,
    };
    let mut line = serde_json::to_vec(&notice_header(notice.clone())).expect("json");
    line.push(b'\n');

    let (header, entries) =
        crate::hold::read_request(&mut io::Cursor::new(line)).expect("a valid request");

    assert!(!header.duties.clipboard);
    assert_eq!(header.duties.notify, Some(notice));
    assert!(entries.is_empty());
}

#[test]
fn ok_on_the_pipe_is_success() {
    let (mut reader, mut writer) = io::pipe().expect("a pipe");
    writer.write_all(b"ok\n").expect("writes");

    assert_eq!(wait_reply(&mut reader, Duration::from_secs(2)), Ok(()));
}

#[test]
fn an_error_on_the_pipe_is_that_error() {
    let (mut reader, mut writer) = io::pipe().expect("a pipe");
    writer
        .write_all(b"error: no data-control\n")
        .expect("writes");

    assert_eq!(
        wait_reply(&mut reader, Duration::from_secs(2)),
        Err("no data-control".to_owned())
    );
}

#[test]
fn silence_until_the_deadline_is_a_failure() {
    let (mut reader, _writer) = io::pipe().expect("a pipe");
    let started = Instant::now();

    let answer = wait_reply(&mut reader, Duration::from_millis(100));

    assert!(answer.is_err_and(|reason| reason.contains("did not answer")));
    assert!(started.elapsed() >= Duration::from_millis(100));
}

fn gone(pid: u32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn a_hold_that_failed_is_killed_and_reaped() {
    let child = Command::new("sleep").arg("30").spawn().expect("spawns");
    let pid = child.id();

    assert!(settle(child, Err(anyhow::anyhow!("silent"))).is_err());

    assert!(gone(pid), "hold {pid} is still there");
}

#[test]
fn a_hold_that_took_the_clipboard_is_reaped_when_it_exits() {
    let child = Command::new("true").spawn().expect("spawns");
    let pid = child.id();

    assert!(settle(child, Ok(())).is_ok());

    assert!(gone(pid), "hold {pid} was left a zombie");
}

#[test]
fn a_hold_that_exits_without_answering_is_a_failure() {
    let (mut reader, writer) = io::pipe().expect("a pipe");
    drop(writer);

    assert!(wait_reply(&mut reader, Duration::from_secs(2)).is_err());
}
