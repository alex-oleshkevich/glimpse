use std::io::{self, Read, Write};
use std::os::fd::{AsFd, AsRawFd as _};
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use glimpse_shot::ports::{Clipboard, Notice};
use glimpse_wayland::selection::{self, Entry, Snapshot};
use rustix::event::{PollFd, PollFlags, Timespec, poll};

use crate::hold::{Duties, HoldHeader, HoldReply, REPLY_FD};

const REPLY_TIMEOUT: Duration = Duration::from_secs(2);
const SNAPSHOT_BUDGET: Duration = Duration::from_secs(1);
const REPLY_CAP: usize = 4096;

#[derive(Clone)]
pub struct HoldClipboard {
    pub config: Option<PathBuf>,
}

impl Clipboard for HoldClipboard {
    fn copy(&self, entries: Vec<Entry>) -> Result<(), String> {
        self.hold(entries).map_err(|error| format!("{error:#}"))
    }

    fn snapshot(&self) -> io::Result<Snapshot> {
        selection::snapshot(SNAPSHOT_BUDGET)
    }
}

impl HoldClipboard {
    pub fn notify(&self, notice: Notice) -> Result<()> {
        self.spawn(&notice_header(notice), Vec::new())
            .context("the notification was not posted")
    }

    fn hold(&self, entries: Vec<Entry>) -> Result<()> {
        self.spawn(&HoldHeader::clipboard(&entries), entries)
            .context("the clipboard was not taken")
    }

    fn spawn(&self, header: &HoldHeader, entries: Vec<Entry>) -> Result<()> {
        let header = serde_json::to_string(header)?;
        let (mut reader, writer) = io::pipe().context("cannot open the reply pipe")?;
        let reply = writer.as_raw_fd();
        let mut command = Command::new(std::env::current_exe()?);
        if let Some(config) = &self.config {
            command.arg("--config").arg(config);
        }
        command
            .arg("hold")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        unsafe {
            command.pre_exec(move || {
                if libc::setsid() < 0 {
                    return Err(io::Error::last_os_error());
                }
                let inherited = match reply == REPLY_FD {
                    true => libc::fcntl(REPLY_FD, libc::F_SETFD, 0),
                    false => libc::dup2(reply, REPLY_FD),
                };
                if inherited < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().context("cannot start glimpse-shot hold")?;
        drop(writer);
        let mut stdin = child.stdin.take().context("hold has no stdin")?;
        let feeder = std::thread::spawn(move || -> io::Result<()> {
            writeln!(stdin, "{header}")?;
            for entry in &entries {
                stdin.write_all(&entry.data)?;
            }
            Ok(())
        });
        let answer = wait_reply(&mut reader, REPLY_TIMEOUT)
            .map_err(anyhow::Error::msg)
            .and_then(|()| {
                feeder
                    .join()
                    .map_err(|_| anyhow::anyhow!("the hold writer panicked"))?
                    .context("cannot hand the clipboard to hold")
            });
        settle(child, answer)
    }
}

fn notice_header(notice: Notice) -> HoldHeader {
    HoldHeader {
        duties: Duties {
            clipboard: false,
            notify: Some(notice),
        },
        ..HoldHeader::clipboard(&[])
    }
}

fn settle(mut child: Child, answer: Result<()>) -> Result<()> {
    match answer {
        Ok(()) => {
            std::thread::spawn(move || child.wait());
            Ok(())
        }
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(error)
        }
    }
}

pub fn wait_reply(reader: &mut (impl Read + AsFd), budget: Duration) -> Result<(), String> {
    let deadline = Instant::now() + budget;
    let mut text = Vec::new();
    let mut chunk = [0u8; 256];
    while !text.ends_with(b"\n") && text.len() < REPLY_CAP {
        let left = deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or_else(|| format!("hold did not answer within {budget:?}"))?;
        let timeout = Timespec {
            tv_sec: left.as_secs() as i64,
            tv_nsec: i64::from(left.subsec_nanos()),
        };
        let mut fds = [PollFd::new(&*reader, PollFlags::IN)];
        match poll(&mut fds, Some(&timeout)) {
            Ok(0) => return Err(format!("hold did not answer within {budget:?}")),
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(format!("cannot wait for hold: {error}")),
        }
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => text.extend_from_slice(&chunk[..read]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(format!("cannot read the answer of hold: {error}")),
        }
    }
    match HoldReply::parse(&String::from_utf8_lossy(&text)) {
        HoldReply::Ok => Ok(()),
        HoldReply::Error(reason) => Err(reason),
    }
}

#[cfg(test)]
mod tests {
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
}
