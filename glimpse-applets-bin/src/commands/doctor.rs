use anyhow::Result;
use clap::Args as ClapArgs;
use std::path::PathBuf;
use std::process::Command;

use crate::project::Language;

#[derive(ClapArgs)]
pub struct Args {
    /// Only check the toolchain for one language. By default every language
    /// is probed and missing ones are reported (not failed).
    #[arg(long, value_enum)]
    lang: Option<Language>,
    /// Exit non-zero if any check fails. Useful in CI / scripts.
    #[arg(long)]
    strict: bool,
}

pub fn run(args: Args) -> Result<()> {
    let mut report = Report::default();

    println!("Glimpse applet environment check");
    println!();

    // Host: where the applet will run.
    report.add(check_command("glimpse-shell", &["--version"], true, false));

    // Per-language toolchains.
    let mut langs = match args.lang {
        Some(l) => vec![l],
        None => vec![
            Language::Rust,
            Language::Python,
            Language::Typescript,
            Language::Go,
        ],
    };
    langs.sort_by_key(|l| l.name());

    for lang in langs {
        for check in language_checks(lang) {
            report.add(check);
        }
    }

    // Optional dev-mode watchers.
    report.add(check_command("watchexec", &["--version"], false, true));

    println!();
    println!(
        "Summary: {} ok, {} optional missing, {} missing.",
        report.ok, report.optional_missing, report.missing
    );

    if report.missing > 0 {
        println!();
        println!("Install hints:");
        for hint in &report.hints {
            println!("  • {hint}");
        }
    }

    if args.strict && report.missing > 0 {
        anyhow::bail!("{} required checks failed", report.missing);
    }
    Ok(())
}

struct Check {
    name: String,
    status: Status,
    hint: Option<String>,
}

enum Status {
    Ok(String),
    Missing,
    OptionalMissing,
}

#[derive(Default)]
struct Report {
    ok: usize,
    missing: usize,
    optional_missing: usize,
    hints: Vec<String>,
}

impl Report {
    fn add(&mut self, check: Check) {
        let symbol = match &check.status {
            Status::Ok(version) => {
                self.ok += 1;
                format!("✓ {} ({version})", check.name)
            }
            Status::OptionalMissing => {
                self.optional_missing += 1;
                format!("- {} (optional, not found)", check.name)
            }
            Status::Missing => {
                self.missing += 1;
                format!("✗ {} (not found)", check.name)
            }
        };
        println!("  {symbol}");
        if matches!(check.status, Status::Missing) {
            if let Some(h) = check.hint {
                self.hints.push(h);
            }
        }
    }
}

fn check_command(cmd: &str, version_args: &[&str], required: bool, allow_missing_path: bool) -> Check {
    let path: Option<PathBuf> = which::which(cmd).ok();

    match path {
        Some(_p) => {
            let version = run_version(cmd, version_args);
            Check {
                name: cmd.to_string(),
                status: Status::Ok(version),
                hint: None,
            }
        }
        None if !required && allow_missing_path => Check {
            name: cmd.to_string(),
            status: Status::OptionalMissing,
            hint: None,
        },
        None if !required => Check {
            name: cmd.to_string(),
            status: Status::OptionalMissing,
            hint: None,
        },
        None => Check {
            name: cmd.to_string(),
            status: Status::Missing,
            hint: install_hint(cmd),
        },
    }
}

fn run_version(cmd: &str, args: &[&str]) -> String {
    let out = Command::new(cmd).args(args).output();
    match out {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let first = stdout.lines().next().or_else(|| stderr.lines().next());
            first.map(str::trim).unwrap_or("present").to_string()
        }
        Err(_) => "present".to_string(),
    }
}

fn language_checks(lang: Language) -> Vec<Check> {
    match lang {
        Language::Rust => vec![
            check_command("cargo", &["--version"], true, false),
            check_command("rustc", &["--version"], true, false),
        ],
        Language::Python => vec![
            check_command("python", &["--version"], true, false),
            check_command("uv", &["--version"], false, true),
        ],
        Language::Typescript => vec![
            check_command("node", &["--version"], true, false),
            check_command("npm", &["--version"], true, false),
        ],
        Language::Go => vec![check_command("go", &["version"], true, false)],
    }
}

fn install_hint(cmd: &str) -> Option<String> {
    let msg = match cmd {
        "cargo" | "rustc" => "Rust: install via rustup → https://rustup.rs",
        "python" => "Python 3.14+: `pacman -S python` (Arch) / `apt install python3` (Debian)",
        "uv" => "uv (recommended for Python): `curl -LsSf https://astral.sh/uv/install.sh | sh`",
        "node" | "npm" => "Node 20+ and npm: install via your distro or https://nodejs.org",
        "go" => "Go 1.24+: `pacman -S go` (Arch) / https://go.dev/dl",
        "glimpse-shell" => "glimpse-shell: install Glimpse — https://github.com/alex-oleshkevich/glimpse",
        "watchexec" => "watchexec (used by `glimpse-applet dev`): `cargo install watchexec-cli`",
        _ => return None,
    };
    Some(msg.to_string())
}
