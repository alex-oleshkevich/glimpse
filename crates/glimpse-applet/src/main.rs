mod errors;

use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use anyhow::{Result, bail};
use clap::Parser;

#[derive(Parser)]
#[command(version)]
struct Cli {
    #[arg(long, value_parser = grant_list)]
    allow_net: Option<String>,
    #[arg(long, value_parser = grant_list)]
    allow_read: Option<String>,
    #[arg(long, value_parser = grant_list)]
    allow_env: Option<String>,
    #[arg(long)]
    watch: bool,
    entry: PathBuf,
}

fn grant_list(value: &str) -> Result<String, String> {
    if value.split(',').any(|item| item.trim().is_empty()) {
        Err("grant list must contain nonempty values".into())
    } else {
        Ok(value.into())
    }
}

fn argv(cli: &Cli) -> Vec<OsString> {
    let mut args = vec![
        "run".into(),
        "-q".into(),
        "--no-prompt".into(),
        "--v8-flags=--max-old-space-size=128".into(),
    ];
    if cli.watch {
        args.push("--watch".into());
    }
    let mut allowed_env = OsString::from("--allow-env=NODE_ENV");
    if let Some(vars) = cli.allow_env.as_deref() {
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
    Err(Command::new(deno).args(argv(&cli)).exec().into())
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
                "--watch",
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
    fn empty_grants_are_rejected() {
        for grant in [
            "--allow-net=",
            "--allow-net=a,",
            "--allow-read=,a",
            "--allow-read=a,,b",
            "--allow-env=",
            "--allow-net=a, ",
        ] {
            assert!(Cli::try_parse_from(["glimpse-applet", grant, "main.ts"]).is_err());
        }
    }

    #[test]
    fn version_flag_reports_workspace_version() {
        let error = Cli::try_parse_from(["glimpse-applet", "--version"])
            .err()
            .unwrap();
        assert_eq!(error.kind(), clap::error::ErrorKind::DisplayVersion);
        assert!(error.to_string().contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn an_entry_that_looks_like_a_flag_stays_a_path() {
        let cli = Cli::try_parse_from(["glimpse-applet", "--", "--allow-run=x.ts"]).unwrap();
        assert_eq!(argv(&cli).last().unwrap(), "./--allow-run=x.ts");
    }
}
