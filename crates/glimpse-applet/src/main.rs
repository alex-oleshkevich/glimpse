mod errors;

use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::process::Stdio;
use std::sync::mpsc;
use std::time::Duration;

use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::process::{Child, Command, ExitCode};

use anyhow::{Result, bail};
use clap::Parser;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    allow_net: Option<String>,
    #[arg(long)]
    allow_read: Option<String>,
    #[arg(long)]
    allow_env: Option<String>,
    #[arg(long)]
    watch: bool,
    entry: PathBuf,
}

fn argv(cli: &Cli) -> Vec<OsString> {
    let mut args = vec![
        "run".into(),
        "-q".into(),
        "--no-prompt".into(),
        "--v8-flags=--max-old-space-size=128".into(),
    ];
    let mut allowed_env = OsString::from("--allow-env=NODE_ENV");
    if let Some(vars) = cli.allow_env.as_deref().filter(|vars| !vars.is_empty()) {
        allowed_env.push(",");
        allowed_env.push(vars);
    }
    args.push(allowed_env);
    if let Some(hosts) = &cli.allow_net {
        args.push(format!("--allow-net={hosts}").into());
    }
    if let Some(paths) = &cli.allow_read {
        args.push(format!("--allow-read={paths}").into());
    }
    if cli.entry.as_os_str().as_encoded_bytes().starts_with(b"-") {
        args.push(PathBuf::from(".").join(&cli.entry).into_os_string());
    } else {
        args.push(cli.entry.as_os_str().to_owned());
    }
    args
}

fn run(cli: Cli) -> Result<()> {
    if !matches!(
        cli.entry.extension().and_then(|ext| ext.to_str()),
        Some("js" | "ts" | "tsx")
    ) {
        bail!("unsupported applet entry: {}", cli.entry.display());
    }
    let deno = glimpse_utils::deno().ok_or(errors::DenoNotFound)?;
    if cli.watch {
        return watch(&deno, &cli);
    }
    Err(Command::new(deno).args(argv(&cli)).exec().into())
}

fn watched_child(deno: &std::path::Path, cli: &Cli) -> Result<Child> {
    let mut command = Command::new(deno);
    command
        .args(argv(cli))
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    unsafe {
        command.pre_exec(|| {
            let parent = rustix::process::getppid();
            rustix::process::set_parent_process_death_signal(Some(rustix::process::Signal::KILL))?;
            if rustix::process::getppid() != parent {
                unsafe extern "C" {
                    fn _exit(status: i32) -> !;
                }
                _exit(1);
            }
            Ok(())
        });
    }
    Ok(command.spawn()?)
}

fn watch(deno: &std::path::Path, cli: &Cli) -> Result<()> {
    let entry = cli.entry.canonicalize()?;
    let parent = entry
        .parent()
        .ok_or_else(|| anyhow::anyhow!("applet entry has no parent"))?;
    let (send, receive) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = send.send(event);
    })?;
    watcher.watch(parent, RecursiveMode::Recursive)?;
    let mut child = watched_child(deno, cli)?;
    loop {
        if let Some(status) = child.try_wait()? {
            std::process::exit(status.code().unwrap_or(1));
        }
        match receive.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(Event {
                kind: EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_),
                ..
            })) => {
                while receive.recv_timeout(Duration::from_millis(150)).is_ok() {}
                child.kill()?;
                child.wait()?;
                child = watched_child(deno, cli)?;
            }
            Ok(Err(error)) => return Err(error.into()),
            Ok(Ok(_)) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("applet source watcher stopped"),
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            errors::exit_code(&error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_has_fixed_flags_and_only_requested_grants() {
        let cli = Cli::try_parse_from(["glimpse-applet", "main.ts"]).unwrap();
        assert_eq!(
            argv(&cli),
            [
                "run",
                "-q",
                "--no-prompt",
                "--v8-flags=--max-old-space-size=128",
                "--allow-env=NODE_ENV",
                "main.ts"
            ]
        );

        let cli = Cli::try_parse_from([
            "glimpse-applet",
            "--watch",
            "--allow-env=A,B",
            "--allow-net=api.example",
            "--allow-read=/tmp",
            "main.tsx",
        ])
        .unwrap();
        assert_eq!(
            argv(&cli),
            [
                "run",
                "-q",
                "--no-prompt",
                "--v8-flags=--max-old-space-size=128",
                "--allow-env=NODE_ENV,A,B",
                "--allow-net=api.example",
                "--allow-read=/tmp",
                "main.tsx"
            ]
        );
    }

    #[test]
    fn dangerous_grants_are_unknown_arguments() {
        for grant in [
            "--allow-run",
            "-A",
            "--allow-write",
            "--allow-ffi",
            "--allow-sys",
        ] {
            assert!(Cli::try_parse_from(["glimpse-applet", grant, "main.ts"]).is_err());
        }
    }

    #[test]
    fn an_entry_that_looks_like_a_flag_stays_a_path() {
        let cli = Cli::try_parse_from(["glimpse-applet", "--", "--allow-run=x.ts"]).unwrap();
        assert_eq!(argv(&cli).last().unwrap(), "./--allow-run=x.ts");
    }
}
