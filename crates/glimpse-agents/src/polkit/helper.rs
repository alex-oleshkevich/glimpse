use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use crate::ask::{Input, Secret};

pub const SOCKET: &str = "/run/polkit/agent-helper.socket";
pub const SETUID: &[&str] = &[
    "/usr/lib/polkit-1/polkit-agent-helper-1",
    "/usr/libexec/polkit-agent-helper-1",
];

const MAX_LINE: usize = 4096;
const SETUID_BIT: u32 = 0o4000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Prompt { text: String, echo: bool },
    Error(String),
    Info(String),
    Success,
    Failure,
    Lost,
}

pub fn parse(line: &str) -> Option<Line> {
    let line = line.trim_end_matches(['\n', '\r']);
    match line {
        "SUCCESS" => return Some(Line::Success),
        "FAILURE" => return Some(Line::Failure),
        _ => {}
    }
    let (kind, text) = line.split_once(' ').unwrap_or((line, ""));
    let text = decompress(text);
    match kind {
        "PAM_PROMPT_ECHO_OFF" => Some(Line::Prompt { text, echo: false }),
        "PAM_PROMPT_ECHO_ON" => Some(Line::Prompt { text, echo: true }),
        "PAM_ERROR_MSG" => Some(Line::Error(text)),
        "PAM_TEXT_INFO" => Some(Line::Info(text)),
        _ => None,
    }
}

fn decompress(source: &str) -> String {
    let mut bytes = source.bytes().peekable();
    let mut out = Vec::with_capacity(source.len());
    while let Some(byte) = bytes.next() {
        if byte != b'\\' {
            out.push(byte);
            continue;
        }
        let Some(escaped) = bytes.next() else {
            break;
        };
        out.push(match escaped {
            b'0'..=b'7' => {
                let mut value = escaped - b'0';
                for _ in 0..2 {
                    match bytes.next_if(|next| (b'0'..=b'7').contains(next)) {
                        Some(digit) => value = value.wrapping_mul(8).wrapping_add(digit - b'0'),
                        None => break,
                    }
                }
                value
            }
            b'b' => 0x08,
            b'f' => 0x0c,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'v' => 0x0b,
            other => other,
        });
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn tagged_key(cookie: &str, epoch: u64) -> String {
    format!("{epoch}\u{1}{cookie}")
}

pub fn split_tagged_key(key: &str) -> Option<(u64, &str)> {
    let (epoch, cookie) = key.split_once('\u{1}')?;
    Some((epoch.parse().ok()?, cookie))
}

pub struct Attempt {
    read: BufReader<Box<dyn AsyncRead + Unpin + Send>>,
    write: Box<dyn AsyncWrite + Unpin + Send>,
    _child: Option<Child>,
}

impl Attempt {
    pub async fn start(
        user: &str,
        cookie: &str,
        socket: &Path,
        helpers: &[PathBuf],
    ) -> io::Result<Self> {
        token(user)?;
        token(cookie)?;
        if tokio::fs::try_exists(socket).await? {
            let stream = tokio::net::UnixStream::connect(socket).await?;
            return Self::socket(stream, user, cookie).await;
        }
        let mut helper = None;
        for path in helpers {
            if setuid_helper(path).await {
                helper = Some(path);
                break;
            }
        }
        let helper = helper
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no polkit agent helper"))?;
        Self::spawned(helper, user, cookie).await
    }

    pub async fn socket(
        stream: tokio::net::UnixStream,
        user: &str,
        cookie: &str,
    ) -> io::Result<Self> {
        token(user)?;
        token(cookie)?;
        let (read, mut write) = stream.into_split();
        write
            .write_all(format!("{user}\n{cookie}\n").as_bytes())
            .await?;
        write.flush().await?;
        Ok(Self::from_parts(read, write, None))
    }

    pub(crate) async fn spawned(helper: &Path, user: &str, cookie: &str) -> io::Result<Self> {
        let mut child = Command::new(helper)
            .arg(user)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("polkit helper has no stdout"))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("polkit helper has no stdin"))?;
        stdin.write_all(format!("{cookie}\n").as_bytes()).await?;
        stdin.flush().await?;
        Ok(Self::from_parts(stdout, stdin, Some(child)))
    }

    fn from_parts(
        read: impl AsyncRead + Unpin + Send + 'static,
        write: impl AsyncWrite + Unpin + Send + 'static,
        child: Option<Child>,
    ) -> Self {
        Self {
            read: BufReader::new(Box::new(read)),
            write: Box::new(write),
            _child: child,
        }
    }

    pub async fn next(&mut self) -> io::Result<Option<Line>> {
        let Some(line) = read_message(&mut self.read).await? else {
            return Ok(None);
        };
        parse(&line)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "helper line is not a polkit message",
                )
            })
            .map(Some)
    }

    pub async fn respond(&mut self, secret: &Secret) -> io::Result<()> {
        self.write.write_all(secret.as_bytes()).await?;
        self.write.write_all(b"\n").await?;
        self.write.flush().await
    }
}

async fn setuid_helper(path: &Path) -> bool {
    let Ok(meta) = tokio::fs::metadata(path).await else {
        return false;
    };
    meta.mode() & SETUID_BIT != 0
}

fn token(value: &str) -> io::Result<()> {
    if value.is_empty()
        || value
            .bytes()
            .any(|byte| matches!(byte, b'\n' | b'\r' | b'\0' | 1))
    {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "polkit helper token is not a single line",
        ))
    } else {
        Ok(())
    }
}

async fn read_message(read: &mut (impl AsyncBufRead + Unpin)) -> io::Result<Option<String>> {
    let mut buffer = Vec::new();
    loop {
        let available = read.fill_buf().await?;
        if available.is_empty() {
            if buffer.is_empty() {
                return Ok(None);
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "helper line ended early",
            ));
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |index| index + 1);
        if buffer.len().saturating_add(take) > MAX_LINE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "helper line is too long",
            ));
        }
        buffer.extend_from_slice(&available[..take]);
        read.consume(take);
        if newline.is_some() {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buffer);
    Ok(Some(text.trim_end_matches(['\n', '\r']).to_owned()))
}

pub async fn drive(
    user: String,
    cookie: String,
    socket: PathBuf,
    helpers: Vec<PathBuf>,
    tagged: String,
    input: relm4::Sender<Input>,
    mut secrets: tokio::sync::mpsc::Receiver<Secret>,
) {
    let mut attempt = match Attempt::start(&user, &cookie, &socket, &helpers).await {
        Ok(attempt) => attempt,
        Err(error) => {
            tracing::warn!(%error, "polkit helper did not start");
            let _ = input.send(Input::Line {
                key: tagged,
                line: Line::Lost,
            });
            return;
        }
    };
    loop {
        let line = match attempt.next().await {
            Ok(Some(line)) => line,
            Ok(None) => {
                let _ = input.send(Input::Line {
                    key: tagged,
                    line: Line::Lost,
                });
                return;
            }
            Err(error) => {
                tracing::debug!(%error, "polkit helper stopped");
                let _ = input.send(Input::Line {
                    key: tagged,
                    line: Line::Lost,
                });
                return;
            }
        };
        let prompt = matches!(line, Line::Prompt { .. });
        let done = matches!(line, Line::Success | Line::Failure);
        if input
            .send(Input::Line {
                key: tagged.clone(),
                line,
            })
            .is_err()
        {
            return;
        }
        if done {
            return;
        }
        if prompt {
            let Some(secret) = secrets.recv().await else {
                return;
            };
            if let Err(error) = attempt.respond(&secret).await {
                tracing::debug!(%error, "polkit helper rejected a response");
                let _ = input.send(Input::Line {
                    key: tagged,
                    line: Line::Lost,
                });
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests;
