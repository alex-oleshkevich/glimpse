use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use glimpse_config::ExecAppletConfig;
use glimpse_services::{
    Edge, ExecEntry, FromApplet, MAX_LINE, Orientation, Outgoing, Placement, Tree, Zone,
};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::signal::unix::{SignalKind, signal};

use super::dev::Scratch;
use crate::cli::CheckArgs;

pub(super) const GROUP: &str = "Desktop Entry";
pub(super) const MARKER: &str = "X-Glimpse-Installed-By";

struct SmokeChild(Child);

impl Drop for SmokeChild {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-(self.0.id() as libc::pid_t), libc::SIGKILL);
        }
        let _ = self.0.wait();
    }
}

async fn message(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<FromApplet> {
    let mut line = Vec::new();
    loop {
        let bytes = reader.fill_buf().await?;
        ensure!(
            !bytes.is_empty(),
            "applet disconnected before its first valid commit"
        );

        let length = bytes
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| index + 1);
        let used = length.unwrap_or(bytes.len());
        ensure!(
            line.len() + used <= MAX_LINE + 1,
            "applet protocol line exceeds {MAX_LINE} bytes"
        );
        line.extend_from_slice(&bytes[..used]);

        reader.consume(used);

        if length.is_some() {
            return Ok(serde_json::from_slice(&line)?);
        }
    }
}

async fn smoke(id: &str, entry: &ExecEntry, config: &ExecAppletConfig) -> Result<()> {
    let scratch = Scratch::new("checks")?;
    let state = scratch.path.join("state");
    fs::create_dir(&state)?;
    let socket = scratch.path.join("applet.sock");
    let listener = UnixListener::bind(&socket)?;
    let mut command = Command::new(entry.argv.first().context("empty applet command")?);
    command
        .args(&entry.argv[1..])
        .envs(&config.env)
        .env("GLIMPSE_APPLET_SOCKET", &socket)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .process_group(0);
    if let Some(cwd) = &entry.cwd {
        command.current_dir(cwd);
    }
    let mut child = SmokeChild(command.spawn().context("cannot start applet smoke check")?);
    let (stream, _) = tokio::select! {
        biased;
        stream = listener.accept() => stream?,
        status = async {
            loop {
                if let Some(status) = child.0.try_wait()? { return Ok::<_, std::io::Error>(status); }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        } => anyhow::bail!("applet exited before connecting: {}", status?),
    };
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    ensure!(
        matches!(message(&mut reader).await?, FromApplet::Hello { v: 1 }),
        "expected applet hello with protocol version 1"
    );
    let hello = Outgoing::Hello {
        v: 1,
        name: id.to_owned(),
        state: state
            .to_str()
            .context("smoke state path is not UTF-8")?
            .to_owned(),
        options: config.options.clone(),
        placement: Placement {
            output: None,
            position: Edge::Bottom,
            orientation: Orientation::Horizontal,
            zone: Zone::Center,
            size: 36,
        },
    };
    writer
        .write_all(format!("{}\n", serde_json::to_string(&hello)?).as_bytes())
        .await?;
    loop {
        match message(&mut reader).await? {
            FromApplet::Commit { ops } => {
                Tree::default().apply(ops)?;
                return Ok(());
            }
            FromApplet::Hello { .. } => {
                anyhow::bail!("applet sent another hello before its first commit")
            }
            _ => {}
        }
    }
}

pub async fn applet_check(args: CheckArgs, json: bool) -> Result<()> {
    let dir = args.dir.canonicalize()?;
    let (id, entry) = super::run::source(&dir)?;
    let config = settings(&dir, &id)?;
    if args.run {
        eprintln!("Running an unconfined headless protocol smoke check.");
        let mut interrupt = signal(SignalKind::interrupt())?;
        let mut terminate = signal(SignalKind::terminate())?;
        let mut hangup = signal(SignalKind::hangup())?;
        tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(args.timeout), smoke(&id, &entry, &config)) => result.context("applet protocol smoke check timed out")??,
            _ = interrupt.recv() => anyhow::bail!("applet check interrupted"),
            _ = terminate.recv() => anyhow::bail!("applet check interrupted"),
            _ = hangup.recv() => anyhow::bail!("applet check interrupted"),
        }
    }
    if json {
        super::emit(&serde_json::json!({"id": id, "valid": true, "protocol": args.run}))?;
    } else {
        crate::render::print(&format!(
            "{id}: valid{}",
            if args.run {
                ", protocol smoke passed"
            } else {
                ""
            }
        ))?;
    }
    Ok(())
}

pub(super) fn settings(dir: &Path, id: &str) -> Result<ExecAppletConfig> {
    let path = dir.join("devconfig.toml");
    match fs::symlink_metadata(&path) {
        Ok(_) => super::run::settings(&path, id),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ExecAppletConfig {
            applet: id.to_owned(),
            options: serde_json::Map::new(),
            env: std::collections::BTreeMap::new(),
        }),
        Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
    }
}

pub(super) fn marked(path: &Path) -> Result<(String, fs::Metadata)> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| {
            if error.raw_os_error() == Some(libc::ELOOP) {
                anyhow::Error::new(crate::errors::Refused(
                    "refusing a symlink desktop entry".to_owned(),
                ))
            } else {
                error.into()
            }
        })?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(
            crate::errors::Refused("desktop entry is not a regular file".to_owned()).into(),
        );
    }
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let desktop = glib::KeyFile::new();
    desktop.load_from_data(&contents, glib::KeyFileFlags::NONE)?;
    if !desktop
        .string(GROUP, MARKER)
        .is_ok_and(|value| value == "glimpsectl")
    {
        return Err(crate::errors::Refused(
            "desktop entry was not installed by glimpsectl".to_owned(),
        )
        .into());
    }
    Ok((contents, metadata))
}

#[cfg(test)]
mod tests;
