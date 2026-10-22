use std::io::{self, Read, Write};
use std::os::fd::{AsFd, AsRawFd as _};
use std::os::unix::process::CommandExt as _;
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
pub struct HoldClipboard;

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
        if let Some(dir) = glimpse_config::config_dir() {
            command.arg("--config-dir").arg(dir);
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
mod tests;
