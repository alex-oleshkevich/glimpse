use std::fs::File;
use std::future::Future;
use std::io::{self, BufRead, Read as _, Write};
use std::os::fd::{FromRawFd as _, OwnedFd};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use glimpse_shot::ports::{Notice, Notifier};
use glimpse_utils::init_translations;
use glimpse_wayland::selection::{Entry, Owned};
use serde::{Deserialize, Serialize};

pub const REPLY_FD: i32 = 3;
const VERSION: u32 = 1;
const SIZE_CAP: u64 = 256 * 1024 * 1024;
const HEADER_CAP: u64 = 64 * 1024;
const MAX_ENTRIES: usize = 64;
pub(crate) const NOTIFY_GRACE: Duration =
    Duration::from_millis(crate::notify::EXPIRE_MS as u64 + 10_000);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoldHeader {
    pub version: u32,
    pub duties: Duties,
    pub entries: Vec<EntryHeader>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Duties {
    pub clipboard: bool,
    pub notify: Option<Notice>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryHeader {
    pub mime: String,
    pub len: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HoldReply {
    Ok,
    Error(String),
}

impl HoldReply {
    pub fn line(&self) -> String {
        match self {
            Self::Ok => "ok\n".to_owned(),
            Self::Error(reason) => format!("error: {reason}\n"),
        }
    }

    pub fn parse(text: &str) -> Self {
        match text.strip_suffix('\n') {
            Some("ok") => Self::Ok,
            Some(line) => match line.strip_prefix("error: ") {
                Some(reason) => Self::Error(reason.to_owned()),
                None => Self::Error(format!("hold answered {line:?}")),
            },
            None if text.is_empty() => Self::Error("hold exited without answering".to_owned()),
            None => Self::Error(format!("hold answered {text:?}")),
        }
    }
}

impl HoldHeader {
    pub fn clipboard(entries: &[Entry]) -> Self {
        Self {
            version: VERSION,
            duties: Duties {
                clipboard: true,
                notify: None,
            },
            entries: entries
                .iter()
                .map(|entry| EntryHeader {
                    mime: entry.mime.clone(),
                    len: entry.data.len() as u64,
                })
                .collect(),
        }
    }
}

fn reply_pipe(fd: i32) -> Result<File> {
    if unsafe { libc::fcntl(fd, libc::F_GETFD) } < 0 {
        bail!("hold answers on fd {fd}, which is not open: it is started by glimpse-shot --copy");
    }
    Ok(File::from(unsafe { OwnedFd::from_raw_fd(fd) }))
}

pub fn run(config: Option<&Path>) -> Result<()> {
    let mut reply = reply_pipe(REPLY_FD)?;
    let language = match glimpse_config::load(config) {
        Ok(config) => config.regional.language().map(str::to_owned),
        Err(error) => {
            tracing::warn!("running on defaults: {error:#}");
            None
        }
    };
    init_translations(language.as_deref());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let taken = read_request(&mut io::stdin().lock()).and_then(|(header, entries)| {
            let owned = match header.duties.clipboard {
                true => Some(serve(entries)?),
                false => None,
            };
            Ok((header.duties.notify, owned))
        });
        let (notice, owned) = match taken {
            Ok(taken) => taken,
            Err(error) => {
                let _ = reply.write_all(HoldReply::Error(format!("{error:#}")).line().as_bytes());
                return Err(error);
            }
        };
        reply.write_all(HoldReply::Ok.line().as_bytes())?;
        drop(reply);
        let notifier = crate::notify::BusNotifier::session();
        let notified = notice.map(|notice| notified(&notifier, notice));
        until_done(owned.as_ref().map(Owned::cancelled), notified).await;
        Ok(())
    })
}

fn serve(entries: Vec<Entry>) -> Result<Owned> {
    Owned::serve(entries, || true)
        .context("cannot take the clipboard")?
        .context("the clipboard was not taken")
}

pub(crate) async fn notified(notifier: &impl Notifier, notice: Notice) {
    if let Err(reason) = notifier.notify(notice) {
        tracing::warn!("cannot post the notification: {reason}");
        return;
    }
    notifier.closed().await;
}

pub fn read_request(input: &mut impl BufRead) -> Result<(HoldHeader, Vec<Entry>)> {
    let mut line = String::new();
    input
        .by_ref()
        .take(HEADER_CAP)
        .read_line(&mut line)
        .context("cannot read the hold header")?;
    if !line.ends_with('\n') {
        bail!("the hold header ended without a newline within {HEADER_CAP} bytes");
    }
    let header: HoldHeader = serde_json::from_str(&line).context("the hold header is invalid")?;
    if header.version != VERSION {
        bail!("hold speaks version {VERSION}, not {}", header.version);
    }
    if header.entries.len() > MAX_ENTRIES {
        bail!(
            "the header declares {} entries, more than the {MAX_ENTRIES} a hold takes",
            header.entries.len()
        );
    }
    let total = header
        .entries
        .iter()
        .try_fold(0u64, |total, entry| total.checked_add(entry.len))
        .filter(|total| *total <= SIZE_CAP);
    if total.is_none() {
        bail!("the header declares more than the {SIZE_CAP} bytes a hold takes");
    }
    let mut entries = Vec::new();
    for entry in &header.entries {
        let mut data = Vec::new();
        input
            .by_ref()
            .take(entry.len)
            .read_to_end(&mut data)
            .with_context(|| format!("cannot read {}", entry.mime))?;
        if data.len() as u64 != entry.len {
            bail!(
                "{} ended after {} of {} bytes",
                entry.mime,
                data.len(),
                entry.len
            );
        }
        entries.push(Entry {
            mime: entry.mime.clone(),
            data: Arc::from(data),
        });
    }
    Ok((header, entries))
}

pub(crate) async fn until_done(
    clipboard: Option<impl Future<Output = ()>>,
    notify: Option<impl Future<Output = ()>>,
) {
    tokio::join!(
        async {
            if let Some(clipboard) = clipboard {
                clipboard.await;
            }
        },
        async {
            if let Some(notify) = notify {
                notify.await;
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use tokio::sync::oneshot;

    use super::*;

    fn request(header: &HoldHeader, body: &[u8]) -> Cursor<Vec<u8>> {
        let mut bytes = serde_json::to_vec(header).expect("json");
        bytes.push(b'\n');
        bytes.extend_from_slice(body);
        Cursor::new(bytes)
    }

    fn png(len: u64) -> HoldHeader {
        HoldHeader {
            version: 1,
            duties: Duties {
                clipboard: true,
                notify: None,
            },
            entries: vec![EntryHeader {
                mime: "image/png".to_owned(),
                len,
            }],
        }
    }

    #[test]
    fn a_request_is_a_header_line_then_each_entry_in_order() {
        let entries = vec![
            Entry {
                mime: "text/plain".to_owned(),
                data: Arc::from(&b"hello"[..]),
            },
            Entry {
                mime: "image/png".to_owned(),
                data: Arc::from(&b"\x89PNG"[..]),
            },
        ];
        let header = HoldHeader::clipboard(&entries);

        let (read, got) =
            read_request(&mut request(&header, b"hello\x89PNG")).expect("a valid request");

        assert_eq!(read, header);
        assert_eq!(got, entries);
    }

    #[test]
    fn a_request_is_refused_when_it_lies_or_asks_too_much() {
        assert!(read_request(&mut request(&png(10), b"short")).is_err());
        let greedy = read_request(&mut request(&png(SIZE_CAP + 1), b"")).expect_err("over the cap");
        assert!(format!("{greedy:#}").contains("more than"), "{greedy:#}");
        let mut split = png(SIZE_CAP / 2 + 1);
        split.entries.push(split.entries[0].clone());
        let split = read_request(&mut request(&split, b"")).expect_err("over the total");
        assert!(format!("{split:#}").contains("more than"), "{split:#}");
        let mut many = png(0);
        many.entries = vec![many.entries[0].clone(); MAX_ENTRIES + 1];
        let many = read_request(&mut request(&many, b"")).expect_err("too many entries");
        assert!(format!("{many:#}").contains("entries"), "{many:#}");
        let mut endless = vec![b' '; HEADER_CAP as usize + 10];
        endless.push(b'\n');
        let endless = read_request(&mut Cursor::new(endless)).expect_err("no end of header");
        assert!(format!("{endless:#}").contains("within"), "{endless:#}");
        let mut newer = png(0);
        newer.version = 2;
        assert!(read_request(&mut request(&newer, b"")).is_err());
        assert!(read_request(&mut Cursor::new(b"not json\n".to_vec())).is_err());
    }

    #[test]
    fn a_reply_descriptor_that_is_not_open_is_refused() {
        use std::os::fd::IntoRawFd as _;

        assert!(reply_pipe(4000).is_err());
        let (_reader, writer) = io::pipe().expect("a pipe");
        assert!(reply_pipe(OwnedFd::from(writer).into_raw_fd()).is_ok());
    }

    #[test]
    fn a_reply_reads_back_as_itself() {
        for reply in [
            HoldReply::Ok,
            HoldReply::Error("no data-control".to_owned()),
        ] {
            assert_eq!(HoldReply::parse(&reply.line()), reply);
        }
        assert!(matches!(HoldReply::parse(""), HoldReply::Error(_)));
        assert!(matches!(HoldReply::parse("okay\n"), HoldReply::Error(_)));
    }

    #[tokio::test]
    async fn hold_stays_until_the_clipboard_duty_ends_after_the_notification_closed() {
        let (clipboard_lost, clipboard) = oneshot::channel::<()>();
        let (notification_closed, notification) = oneshot::channel::<()>();
        let held = tokio::spawn(until_done(
            Some(async {
                let _ = clipboard.await;
            }),
            Some(async {
                let _ = notification.await;
            }),
        ));

        notification_closed.send(()).expect("sent");
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!held.is_finished());

        clipboard_lost.send(()).expect("sent");
        tokio::time::timeout(Duration::from_secs(1), held)
            .await
            .expect("ends once every duty is done")
            .expect("joins");
    }

    #[tokio::test]
    async fn a_hold_with_no_duty_left_ends_at_once() {
        tokio::time::timeout(
            Duration::from_secs(1),
            until_done(None::<std::future::Ready<()>>, Some(std::future::ready(()))),
        )
        .await
        .expect("ends");
    }
}
