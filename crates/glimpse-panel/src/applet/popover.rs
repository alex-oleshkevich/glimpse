use super::Opener;
use futures_util::StreamExt;
use glimpse_dbus::systemd1::Systemd1ManagerProxy;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::OnceCell;
use zbus::zvariant::Value;

static MANAGER: OnceCell<Systemd1ManagerProxy<'static>> = OnceCell::const_new();
static LAUNCH_SERIAL: AtomicU64 = AtomicU64::new(0);
const JOB_TIMEOUT: Duration = Duration::from_secs(10);

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
        if let Err(error) = launch(&command, true, &BTreeMap::new()).await {
            tracing::warn!(program = command.first(), %error, "command did not start");
        }
    });
}

pub async fn launch(
    command: &[String],
    inherit: bool,
    vars: &BTreeMap<String, String>,
) -> Result<(), gtk4::glib::Error> {
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
    let environment = environment(panel_environment(), inherit, token.as_deref(), vars);
    let clean_env = if inherit {
        None
    } else {
        let env = gtk4::glib::find_program_in_path("env")
            .and_then(|env| env.into_os_string().into_string().ok())
            .ok_or_else(|| {
                gtk4::glib::Error::new(gtk4::glib::FileError::Noent, "program not found")
            })?;
        Some(env)
    };
    let (unit, properties) =
        transient_unit(command, path, &suffix, clean_env.as_deref(), &environment);
    let started = relm4::spawn(start(unit, properties))
        .await
        .unwrap_or_else(|error| Err(zbus::Error::Failure(error.to_string())));
    match started {
        Ok(result) => return finished(result.as_deref()),
        Err(error) => {
            tracing::debug!(program, %error, "cannot start transient service, spawning directly")
        }
    }
    let launcher = gtk4::gio::SubprocessLauncher::new(gtk4::gio::SubprocessFlags::NONE);
    let variables: Vec<std::ffi::OsString> = environment
        .iter()
        .map(|(name, value)| format!("{name}={value}").into())
        .collect();
    launcher.set_environ(&variables);
    let argv: Vec<&std::ffi::OsStr> = command.iter().map(|argument| argument.as_ref()).collect();
    launcher.spawn(&argv).map(drop)
}

async fn manager() -> zbus::Result<Systemd1ManagerProxy<'static>> {
    let bus = zbus::Connection::session().await?;
    let manager = Systemd1ManagerProxy::new(&bus).await?;
    manager.subscribe().await?;
    Ok(manager)
}

async fn start(
    unit: String,
    properties: Vec<(&'static str, Value<'static>)>,
) -> zbus::Result<Option<String>> {
    let manager = MANAGER.get_or_try_init(manager).await?;
    let mut removed = manager
        .receive_job_removed()
        .await
        .map(glimpse_dbus::events)?;
    let job = manager
        .start_transient_unit(&unit, "fail", &properties, &[])
        .await?;
    let result = async {
        while let Some(delivery) = removed.next().await {
            let glimpse_dbus::Delivery::Item(signal) = delivery else {
                return None;
            };
            if let Ok(args) = signal.args()
                && args.job == job
            {
                return Some(args.result);
            }
        }
        None
    };
    Ok(tokio::time::timeout(JOB_TIMEOUT, result)
        .await
        .ok()
        .flatten())
}

fn finished(result: Option<&str>) -> Result<(), gtk4::glib::Error> {
    match result {
        None | Some("done") => Ok(()),
        Some(result) => Err(gtk4::glib::Error::new(
            gtk4::glib::FileError::Failed,
            &format!("the service ended with {result}"),
        )),
    }
}

fn panel_environment() -> Vec<(String, String)> {
    std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .collect()
}

fn set_variable(environment: &mut Vec<(String, String)>, name: &str, value: &str) {
    if value.contains('\0') {
        return;
    }
    if let Some(entry) = environment.iter_mut().find(|(key, _)| key == name) {
        entry.1 = value.to_owned();
    } else {
        environment.push((name.to_owned(), value.to_owned()));
    }
}

fn environment(
    panel: Vec<(String, String)>,
    inherit: bool,
    token: Option<&str>,
    vars: &BTreeMap<String, String>,
) -> Vec<(String, String)> {
    let mut environment = Vec::new();
    for (name, value) in &panel {
        let kept = if inherit {
            glimpse_config::exec_env_name_allowed(name)
        } else {
            matches!(
                name.as_str(),
                "XDG_RUNTIME_DIR" | "WAYLAND_DISPLAY" | "DISPLAY"
            )
        };
        if kept {
            set_variable(&mut environment, name, value);
        }
    }
    if let Some(token) = token {
        set_variable(&mut environment, "XDG_ACTIVATION_TOKEN", token);
    }
    for (name, value) in vars {
        set_variable(&mut environment, name, value);
    }
    environment
}

fn transient_unit(
    command: &[String],
    path: &str,
    suffix: &str,
    clean_env: Option<&str>,
    environment: &[(String, String)],
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
    let variables: Vec<String> = environment
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    let (exec, variables) = match clean_env {
        None => ((path.to_owned(), command.to_vec(), false), variables),
        Some(env) => {
            let mut argv = vec!["env".to_owned(), "-i".to_owned()];
            argv.extend(variables);
            argv.push(path.to_owned());
            argv.extend(command[1..].iter().cloned());
            ((env.to_owned(), argv, false), Vec::new())
        }
    };
    (
        format!("app-glimpse-{escaped}-{suffix}.service"),
        vec![
            ("ExecStart", Value::from(vec![exec])),
            ("Type", Value::from("exec")),
            ("ExitType", Value::from("cgroup")),
            ("Slice", Value::from("app.slice")),
            ("CollectMode", Value::from("inactive-or-failed")),
            ("Environment", Value::from(variables)),
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
