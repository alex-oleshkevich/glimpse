use glimpse_core::Config;
use glimpse_core::dbus::glimpse_lock::GlimpseLockProxy;
use glimpse_lock::{
    app::{self, LockAppConfig},
    dbus::LockApiState,
    logind,
    runtime::LockRuntime,
    safety,
};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

const EXPORTED_LOCK_CSS: &str = include_str!("../resources/export-lock.css");
const EXPORTED_LOCK_CONFIG: &str = include_str!("../resources/export-config.toml");

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let command = parse_command(&args)?;
    match command {
        Command::Help => {
            print_help();
            return Ok(());
        }
        Command::Version => {
            println!("glimpse-lock {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Command::ExportCss => {
            let path = export_css()?;
            println!("wrote {}", path.display());
            return Ok(());
        }
        Command::ExportConfig => {
            let path = export_config()?;
            println!("wrote {}", path.display());
            return Ok(());
        }
        Command::Lock => {
            let runtime = tokio::runtime::Runtime::new()?;
            return runtime.block_on(request_resident_lock());
        }
        Command::Status => {
            let runtime = tokio::runtime::Runtime::new()?;
            return runtime.block_on(print_status());
        }
        Command::Check => {
            return run_check();
        }
        Command::Run | Command::Preview => {}
    }
    let preview = command == Command::Preview;
    let gtk_args = gtk_args(&args, command);
    register_resources();

    tracing_subscriber::fmt()
        .with_env_filter(log_filter())
        .init();
    tracing::info!("glimpse-lock {}", env!("CARGO_PKG_VERSION"));
    let config = LockAppConfig::load();
    let runtime = tokio::runtime::Runtime::new()?;
    let instance_guard = if preview {
        None
    } else {
        Some(runtime.block_on(LockRuntime::acquire_single_instance())?)
    };
    let api_state = LockApiState::default();
    let result = if preview {
        tracing::info!("starting glimpse-lock preview; password 'valid' succeeds");
        let _runtime_guard = runtime.enter();
        app::run_preview(config, gtk_args)
    } else {
        let _runtime_guard = runtime.enter();
        app::run(
            config,
            gtk_args,
            instance_guard.as_ref().map(|guard| guard.connection()),
            api_state.clone(),
        )
    };
    if !preview && api_state.was_ever_active() {
        if let Err(error) = runtime.block_on(logind::set_current_session_locked_hint(false)) {
            tracing::debug!(%error, "failed to set logind LockedHint=false");
        }
    }
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Run,
    Lock,
    Preview,
    ExportCss,
    ExportConfig,
    Status,
    Check,
    Version,
    Help,
}

fn parse_command<I, S>(args: I) -> anyhow::Result<Command>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = args.into_iter().skip(1);
    let Some(command) = args.next() else {
        return Ok(Command::Run);
    };
    let command = command.as_ref();
    let parsed = match command {
        "lock" => Command::Lock,
        "preview" => Command::Preview,
        "export-css" => Command::ExportCss,
        "export-config" => Command::ExportConfig,
        "status" => Command::Status,
        "check" => Command::Check,
        "version" | "--version" | "-V" => Command::Version,
        "--help" | "-h" => Command::Help,
        _ => anyhow::bail!("unknown glimpse-lock command: {command}"),
    };
    if !matches!(parsed, Command::Run | Command::Preview | Command::Help) && args.next().is_some() {
        anyhow::bail!("command {command} does not accept extra arguments");
    }
    Ok(parsed)
}

fn register_resources() {
    gio::resources_register_include!("glimpse-lock.gresource")
        .expect("failed to register embedded lock resources");
}

fn gtk_args(args: &[String], command: Command) -> Vec<String> {
    match command {
        Command::Preview => args
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != 1)
            .map(|(_, arg)| arg.clone())
            .collect(),
        _ => args.to_vec(),
    }
}

fn print_help() {
    println!("glimpse-lock {}", env!("CARGO_PKG_VERSION"));
    println!("Glimpse Wayland lock screen");
    println!();
    println!("USAGE:");
    println!("    glimpse-lock [COMMAND]");
    println!();
    println!("COMMANDS:");
    println!("    lock           Request the resident daemon to lock the session");
    println!("    status         Show whether the lock screen is currently active");
    println!("    check          Verify PAM and service configuration");
    println!("    export-css     Write the default lock CSS to the config dir");
    println!("    export-config  Write a starter config.toml to the config dir");
    println!("    preview        Open the lock screen UI without locking the session");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help      Print help");
    println!("    -V, --version   Print version");
    println!();
    println!("Without a command, glimpse-lock starts the resident daemon.");
}

fn export_css() -> anyhow::Result<PathBuf> {
    let path = Config::config_dir().join("themes").join("lock.css");
    if path.exists() {
        anyhow::bail!(
            "lock CSS already exists at {}; refusing to overwrite",
            path.display()
        );
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, EXPORTED_LOCK_CSS)?;
    Ok(path)
}

fn export_config() -> anyhow::Result<PathBuf> {
    let path = Config::config_file();
    if path.exists() {
        anyhow::bail!(
            "config already exists at {}; refusing to overwrite",
            path.display()
        );
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, EXPORTED_LOCK_CONFIG)?;
    Ok(path)
}

fn log_filter() -> EnvFilter {
    match std::env::var("GLIMPSE_LOG_LEVEL") {
        Ok(value) => normalized_glimpse_log_filter(&value)
            .unwrap_or_else(|| EnvFilter::new("info,relm4=warn")),
        Err(_) => {
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,relm4=warn"))
        }
    }
}

fn normalized_glimpse_log_filter(value: &str) -> Option<EnvFilter> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let filter = if value.contains(',') || value.contains('=') {
        value.to_string()
    } else {
        format!("{value},relm4=warn")
    };
    EnvFilter::try_new(filter).ok()
}

async fn request_resident_lock() -> anyhow::Result<()> {
    let connection = zbus::Connection::session().await?;
    let proxy = GlimpseLockProxy::new(&connection).await?;
    proxy.lock().await?;
    Ok(())
}

async fn print_status() -> anyhow::Result<()> {
    let connection = zbus::Connection::session().await?;
    let proxy = GlimpseLockProxy::new(&connection).await?;
    let active = proxy.get_active().await?;
    let active_time = proxy.get_active_time().await?;

    println!("service: available");
    println!("active: {active}");
    println!("active_time: {active_time}");
    Ok(())
}

fn run_check() -> anyhow::Result<()> {
    let mut failed = false;

    let no_new_privs = safety::current_no_new_privs()?.unwrap_or(false);
    print_check("no_new_privs", !no_new_privs);
    failed |= no_new_privs;

    let pam_file_ok = std::fs::read_to_string("/etc/pam.d/glimpse-lock")
        .map(|contents| pam_file_uses_real_auth(&contents))
        .unwrap_or(false);
    print_check("pam_file", pam_file_ok);
    failed |= !pam_file_ok;

    let unix_chkpwd_ok = unix_chkpwd_allows_pam();
    print_check("unix_chkpwd", unix_chkpwd_ok);
    failed |= !unix_chkpwd_ok;

    // Only meaningful when run from the source tree; an installed binary has no
    // repo-relative data/ file, so skip rather than print a misleading "ok".
    if let Ok(service) = std::fs::read_to_string("data/glimpse-lock.service") {
        let packaged_service_ok = service_file_allows_pam_helpers(&service);
        print_check("packaged_service", packaged_service_ok);
        failed |= !packaged_service_ok;
    }

    let effective_unit_ok = effective_unit_allows_pam_helpers();
    print_check("effective_unit", effective_unit_ok);
    failed |= !effective_unit_ok;

    if failed {
        anyhow::bail!("glimpse-lock check failed");
    }
    Ok(())
}

fn print_check(name: &str, ok: bool) {
    println!("{name}: {}", if ok { "ok" } else { "fail" });
}

fn unix_chkpwd_allows_pam() -> bool {
    let Ok(metadata) = std::fs::metadata("/usr/bin/unix_chkpwd") else {
        return false;
    };
    metadata.is_file()
        && metadata.uid() == 0
        && metadata.gid() == 0
        && metadata.mode() & 0o4000 != 0
}

/// Returns true iff the systemd service file does NOT contain any directive
/// known to break PAM's setuid helpers (unix_chkpwd, pam_systemd_home, etc.).
///
/// This is a denylist of directives. Anything not listed slips through, which
/// is the safer trade-off than a positive allowlist: most hardening
/// directives are PAM-safe, and we only want to flag the ones we know break
/// auth. Extend this list when a new directive is discovered to interfere.
fn service_file_allows_pam_helpers(service: &str) -> bool {
    const KNOWN_INCOMPATIBLE: &[&str] = &[
        // Blocks setuid execution of unix_chkpwd.
        "NoNewPrivileges=true",
        "NoNewPrivileges=yes",
        // Strips the setuid bit / SGID set up unix_chkpwd needs.
        "RestrictSUIDSGID=true",
        "RestrictSUIDSGID=yes",
        // Hides /etc/shadow from the service (and from any helper it spawns).
        "ProtectSystem=strict",
        // PrivateUsers maps uid 0 outside the user namespace; setuid breaks.
        "PrivateUsers=true",
        "PrivateUsers=yes",
        // Mount-namespace strictness that may hide /etc/pam.d or PAM modules.
        "ProtectHome=true",
        "ProtectHome=yes",
        "ProtectHome=read-only",
    ];
    !service.lines().any(|line| {
        let line = line.trim();
        KNOWN_INCOMPATIBLE.iter().any(|bad| *bad == line)
    })
}

fn pam_file_uses_real_auth(contents: &str) -> bool {
    !contents
        .lines()
        .map(str::trim)
        .any(|line| !line.starts_with('#') && line.contains("pam_permit.so"))
}

/// Probes the live systemd unit via `systemctl --user show` for the same
/// PAM-incompatible directives as service_file_allows_pam_helpers. Fails
/// closed on any error: the point of `check` is to flag misconfiguration,
/// so a failed probe (systemctl absent, permission denied, no service yet)
/// is treated as "cannot verify -> assume bad" rather than silent pass.
fn effective_unit_allows_pam_helpers() -> bool {
    const PROPERTIES: &[&str] = &[
        "NoNewPrivileges",
        "RestrictSUIDSGID",
        "ProtectSystem",
        "PrivateUsers",
        "ProtectHome",
    ];
    let mut args = vec![
        "--user".to_string(),
        "show".to_string(),
        "glimpse-lock.service".to_string(),
        "--no-pager".to_string(),
    ];
    for prop in PROPERTIES {
        args.push("-p".to_string());
        args.push((*prop).to_string());
    }
    let Ok(output) = std::process::Command::new("systemctl").args(&args).output() else {
        tracing::warn!("could not invoke systemctl to verify glimpse-lock.service hardening");
        return false;
    };
    if !output.status.success() {
        tracing::warn!(
            stderr = %String::from_utf8_lossy(&output.stderr),
            "systemctl show exited non-zero; cannot verify glimpse-lock.service hardening"
        );
        return false;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    !stdout.lines().any(|line| {
        let line = line.trim();
        matches!(
            line,
            "NoNewPrivileges=yes"
                | "RestrictSUIDSGID=yes"
                | "ProtectSystem=strict"
                | "PrivateUsers=yes"
                | "ProtectHome=yes"
                | "ProtectHome=read-only"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{Command, pam_file_uses_real_auth, parse_command, service_file_allows_pam_helpers};

    #[test]
    fn command_parser_defaults_to_resident_daemon() {
        assert_eq!(
            parse_command(["glimpse-lock"]).expect("default command should parse"),
            Command::Run
        );
    }

    #[test]
    fn command_parser_accepts_lock_subcommand() {
        assert_eq!(
            parse_command(["glimpse-lock", "lock"]).expect("lock command should parse"),
            Command::Lock
        );
    }

    #[test]
    fn command_parser_accepts_preview_subcommand() {
        assert_eq!(
            parse_command(["glimpse-lock", "preview"]).expect("preview command should parse"),
            Command::Preview
        );
    }

    #[test]
    fn command_parser_accepts_export_css_subcommand() {
        assert_eq!(
            parse_command(["glimpse-lock", "export-css"]).expect("export-css command should parse"),
            Command::ExportCss
        );
    }

    #[test]
    fn command_parser_accepts_status_check_and_version_subcommands() {
        assert_eq!(
            parse_command(["glimpse-lock", "status"]).expect("status command should parse"),
            Command::Status
        );
        assert_eq!(
            parse_command(["glimpse-lock", "check"]).expect("check command should parse"),
            Command::Check
        );
        for v in ["version", "--version", "-V"] {
            assert_eq!(
                parse_command(["glimpse-lock", v]).expect("version should parse"),
                Command::Version
            );
        }
    }

    #[test]
    fn command_parser_rejects_legacy_flags() {
        assert!(parse_command(["glimpse-lock", "--preview"]).is_err());
        assert!(parse_command(["glimpse-lock", "--export-css"]).is_err());
    }

    #[test]
    fn packaged_service_file_allows_pam_helpers() {
        let service = include_str!("../../data/glimpse-lock.service");

        assert!(service_file_allows_pam_helpers(service));
    }

    #[test]
    fn service_file_regression_detects_pam_breaking_hardening() {
        let service = "NoNewPrivileges=true\nRestrictSUIDSGID=true\n";

        assert!(!service_file_allows_pam_helpers(service));
    }

    #[test]
    fn pam_file_check_rejects_rescue_permit_stack() {
        let contents = "auth required pam_permit.so\n";

        assert!(!pam_file_uses_real_auth(contents));
    }

    #[test]
    fn pam_file_check_accepts_real_stack() {
        let contents = "auth include system-local-login\n";

        assert!(pam_file_uses_real_auth(contents));
    }
}
