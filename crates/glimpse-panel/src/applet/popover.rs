use super::Opener;
use glimpse_dbus::systemd1::Systemd1ManagerProxy;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::OnceCell;
use zbus::zvariant::Value;

static SESSION: OnceCell<zbus::Connection> = OnceCell::const_new();
static LAUNCH_SERIAL: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct Seat {
    host: relm4::Sender<super::runtime::HostInput>,
}

impl Seat {
    pub(crate) fn new(host: relm4::Sender<super::runtime::HostInput>) -> Self {
        Self { host }
    }

    pub fn opener(&self) -> Opener {
        Opener(self.host.clone())
    }
}

pub fn run(command: &[String]) {
    let command = command.to_vec();
    relm4::spawn_local(async move {
        if let Err(error) = launch(&command).await {
            tracing::warn!(program = command.first(), %error, "command did not start");
        }
    });
}

pub async fn launch(command: &[String]) -> Result<(), gtk4::glib::Error> {
    let Some(program) = command.first() else {
        return Ok(());
    };
    let path = gtk4::glib::find_program_in_path(program)
        .ok_or_else(|| gtk4::glib::Error::new(gtk4::glib::FileError::Noent, "program not found"))?;
    let path = path.to_str().ok_or_else(|| {
        gtk4::glib::Error::new(gtk4::glib::FileError::Inval, "program path is not UTF-8")
    })?;
    let token = activation_token();
    let suffix = format!(
        "{}-{}",
        std::process::id(),
        LAUNCH_SERIAL.fetch_add(1, Ordering::Relaxed)
    );
    let (unit, properties) = transient_unit(
        command,
        path,
        &suffix,
        [
            token.as_ref().map(ToString::to_string),
            std::env::var("WAYLAND_DISPLAY").ok(),
            std::env::var("DISPLAY").ok(),
        ],
    );
    let result = async {
        let bus = SESSION.get_or_try_init(zbus::Connection::session).await?;
        let proxy = Systemd1ManagerProxy::new(bus).await?;
        proxy
            .start_transient_unit(&unit, "fail", &properties, &[])
            .await
    }
    .await;
    match result {
        Ok(_) => return Ok(()),
        Err(error) => {
            tracing::debug!(program, %error, "cannot start transient service, spawning directly")
        }
    }
    let launcher = gtk4::gio::SubprocessLauncher::new(gtk4::gio::SubprocessFlags::NONE);
    if !glimpse_utils::language_was_inherited() {
        launcher.unsetenv("LANGUAGE");
    }
    if let Some(token) = token {
        launcher.setenv("XDG_ACTIVATION_TOKEN", &token, true);
    }
    let argv: Vec<&std::ffi::OsStr> = command.iter().map(|argument| argument.as_ref()).collect();
    launcher.spawn(&argv).map(drop)
}

fn transient_unit(
    command: &[String],
    path: &str,
    suffix: &str,
    environment: [Option<String>; 3],
) -> (String, Vec<(&'static str, Value<'static>)>) {
    let basename = Path::new(&command[0])
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("program");
    let mut escaped = String::new();
    for (index, byte) in basename.bytes().enumerate() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b':') || byte == b'.' && index > 0
        {
            escaped.push(char::from(byte));
        } else {
            escaped.push_str(&format!("\\x{byte:02x}"));
        }
    }
    let environment: Vec<String> = ["XDG_ACTIVATION_TOKEN", "WAYLAND_DISPLAY", "DISPLAY"]
        .into_iter()
        .zip(environment)
        .filter_map(|(name, value)| value.map(|value| format!("{name}={value}")))
        .collect();
    (
        format!("app-glimpse-{escaped}-{suffix}.service"),
        vec![
            (
                "ExecStart",
                Value::from(vec![(path.to_owned(), command.to_vec(), false)]),
            ),
            ("Type", Value::from("exec")),
            ("Slice", Value::from("app.slice")),
            ("CollectMode", Value::from("inactive-or-failed")),
            ("Environment", Value::from(environment)),
        ],
    )
}

fn activation_token() -> Option<gtk4::glib::GString> {
    use gtk4::prelude::{AppLaunchContextExt as _, DisplayExt as _};
    if !gtk4::is_initialized_main_thread() {
        return None;
    }
    let token = gtk4::gdk::Display::default()?
        .app_launch_context()
        .startup_notify_id(gtk4::gio::AppInfo::NONE, &[])?;
    (!token.is_empty()).then_some(token)
}

#[cfg(test)]
mod tests;
