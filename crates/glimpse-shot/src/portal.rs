use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{Context as _, Result, bail};
use futures_util::future::BoxFuture;
use glimpse_shot::doc::wire::ResultV1;
use serde::Deserialize;
use std::process::Stdio;
use tokio::io::AsyncReadExt as _;
use tokio::process::Command;
use zbus::Connection;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Structure, Value};

const BUS_NAME: &str = "me.aresa.Glimpse.Shot.Portal";
const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
const APP_CAP: usize = 200;
const OVERLAY_TIMEOUT_SECS: u32 = 300;
const APP_ID_VARIABLE: &str = "GLIMPSE_SHOT_APP_ID";
const PORTAL_APP_ID: &str = "me.aresa.Glimpse.Shot.Portal";
static NEXT_APP: AtomicU64 = AtomicU64::new(0);
const SUCCESS: u32 = 0;
const CANCELLED: u32 = 1;
const OTHER: u32 = 2;
const TARGET_SCREEN: u32 = 1;
const TARGET_AREA: u32 = 4;
const TARGET_ACTIVE_WINDOW: u32 = 8;
const AVAILABLE_TARGETS: u32 = TARGET_SCREEN | TARGET_AREA | TARGET_ACTIVE_WINDOW;
const OPEN: u8 = 0;
const CLOSED: u8 = 1;
const REPLIED: u8 = 2;

type Reply = (u32, HashMap<String, OwnedValue>);
type Kill = Arc<dyn Fn() + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureKind {
    Overlay,
    Headless,
}

#[derive(Clone, Copy)]
enum RequestKind {
    Shot,
    Color,
}

struct Finished {
    stdout: Vec<u8>,
    code: Option<i32>,
}

struct Running {
    kill: Kill,
    wait: BoxFuture<'static, Finished>,
}

trait Spawner: Send + Sync + 'static {
    fn launch(&self, argv: Vec<OsString>) -> BoxFuture<'static, std::io::Result<Running>>;
}

pub fn run(config_path: Option<&Path>) -> Result<()> {
    let config = glimpse_config::load(config_path)?;
    glimpse_utils::init_translations(config.regional.language());
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        let connection = zbus::Connection::session().await?;
        glimpse_dbus::own_name(&connection, BUS_NAME)
            .await
            .context("me.aresa.Glimpse.Shot.Portal is already owned")?;
        serve_at(
            &connection,
            Arc::new(ProcessSpawner),
            config_path.map(Path::to_path_buf),
        )
        .await?;
        tracing::info!(bus_name = BUS_NAME, "serving the screenshot portal");
        std::future::pending::<Result<()>>().await
    })
}

async fn serve_at(
    connection: &Connection,
    spawner: Arc<dyn Spawner>,
    config: Option<PathBuf>,
) -> Result<()> {
    let portal = ScreenshotPortal {
        inner: Arc::new(Inner {
            spawner,
            config,
            flight: Mutex::new(None),
        }),
    };
    if !connection.object_server().at(OBJECT_PATH, portal).await? {
        bail!("screenshot portal object path is already taken");
    }
    Ok(())
}

fn decide(interactive: bool, permission_store_checked: bool) -> CaptureKind {
    if interactive || !permission_store_checked {
        CaptureKind::Overlay
    } else {
        CaptureKind::Headless
    }
}

fn option_flag(options: &HashMap<String, OwnedValue>, key: &str) -> bool {
    options
        .get(key)
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(false)
}

fn option_target(options: &HashMap<String, OwnedValue>) -> Option<u32> {
    options
        .get("target")
        .map(|value| u32::try_from(value).unwrap_or(0))
}

fn app_label(app_id: &str) -> String {
    let cleaned = glimpse_utils::text::clean(app_id, APP_CAP);
    if cleaned.is_empty() {
        gettextrs::gettext("An application")
    } else {
        cleaned
    }
}

fn resolve_executable(name: &str, current_exe: Option<&Path>) -> PathBuf {
    if let Some(path) = current_exe
        .and_then(Path::parent)
        .map(|dir| dir.join(name))
        .filter(|path| path.is_file())
    {
        return path;
    }
    PathBuf::from(name)
}

fn executable(name: &str) -> PathBuf {
    resolve_executable(name, std::env::current_exe().ok().as_deref())
}

fn overlay_command(label: &str) -> Vec<OsString> {
    vec![
        executable("glimpse-shot").into(),
        OsString::from("--screen"),
        OsString::from("--json"),
        OsString::from("--portal"),
        OsString::from(label),
        OsString::from("--timeout"),
        OsString::from(OVERLAY_TIMEOUT_SECS.to_string()),
    ]
}

fn headless_command(config: Option<&Path>, window: bool) -> Vec<OsString> {
    let mut argv = vec![executable("glimpse-shot").into(), OsString::from("capture")];
    if let Some(config) = config {
        argv.extend([
            OsString::from("--config"),
            config.as_os_str().to_os_string(),
        ]);
    }
    argv.push(OsString::from(if window { "--window" } else { "--screen" }));
    if window {
        argv.push(OsString::from("focused"));
    }
    argv.push(OsString::from("--json"));
    argv
}

fn picker_command() -> Vec<OsString> {
    vec![
        executable("glimpse-lens").into(),
        OsString::from("--tool"),
        OsString::from("picker"),
        OsString::from("--json"),
    ]
}

fn file_uri(path: &Path) -> Option<String> {
    if !path.is_absolute() {
        return None;
    }
    let text = path.to_str()?;
    let mut uri = String::from("file://");
    for byte in text.bytes() {
        if matches!(
            byte,
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/'
        ) {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    Some(uri)
}

fn other() -> Reply {
    (OTHER, HashMap::new())
}

fn cancelled() -> Reply {
    (CANCELLED, HashMap::new())
}

fn response_of(kind: RequestKind, finished: &Finished) -> Reply {
    match finished.code {
        Some(0) => parse_success(kind, &finished.stdout).unwrap_or_else(other),
        Some(4) => cancelled(),
        _ => other(),
    }
}

fn parse_success(kind: RequestKind, stdout: &[u8]) -> Option<Reply> {
    let text = std::str::from_utf8(stdout).ok()?;
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    match kind {
        RequestKind::Shot => {
            let result: ResultV1 = serde_json::from_str(text).ok()?;
            let path = result.path?;
            uri_reply(file_uri(&std::path::absolute(path).ok()?)?)
        }
        RequestKind::Color => {
            let picked: Picked = serde_json::from_str(text).ok()?;
            color_reply(picked.red, picked.green, picked.blue)
        }
    }
}

fn uri_reply(uri: String) -> Option<Reply> {
    let mut results = HashMap::new();
    results.insert(
        "uri".to_owned(),
        OwnedValue::try_from(Value::from(uri)).ok()?,
    );
    Some((SUCCESS, results))
}

fn color_reply(red: u8, green: u8, blue: u8) -> Option<Reply> {
    let value = Value::from(Structure::from((
        f64::from(red) / 255.0,
        f64::from(green) / 255.0,
        f64::from(blue) / 255.0,
    )));
    let mut results = HashMap::new();
    results.insert("color".to_owned(), OwnedValue::try_from(value).ok()?);
    Some((SUCCESS, results))
}

#[derive(Deserialize)]
struct Picked {
    red: u8,
    green: u8,
    blue: u8,
}

struct ProcessSpawner;

impl Spawner for ProcessSpawner {
    fn launch(&self, argv: Vec<OsString>) -> BoxFuture<'static, std::io::Result<Running>> {
        Box::pin(spawn_child(argv))
    }
}

async fn spawn_child(argv: Vec<OsString>) -> std::io::Result<Running> {
    let mut child = child_command(argv)?.spawn()?;
    let pid = child.id();
    let stdout = child.stdout.take();
    let kill: Kill = Arc::new(move || terminate(pid));
    let wait = Box::pin(async move {
        let read = async {
            let mut bytes = Vec::new();
            if let Some(mut out) = stdout {
                let _ = out.read_to_end(&mut bytes).await;
            }
            bytes
        };
        let (status, stdout) = tokio::join!(child.wait(), read);
        Finished {
            code: status.ok().and_then(|status| status.code()),
            stdout,
        }
    });
    Ok(Running { kill, wait })
}

fn child_command(argv: Vec<OsString>) -> std::io::Result<Command> {
    let app_id = child_app_id(&argv);
    let mut parts = argv.into_iter();
    let Some(program) = parts.next() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "empty command",
        ));
    };
    let mut command = Command::new(program);
    command
        .args(parts)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .kill_on_drop(true);
    if let Some(app_id) = app_id {
        command.env(APP_ID_VARIABLE, app_id);
    }
    Ok(command)
}

fn child_app_id(argv: &[OsString]) -> Option<String> {
    argv.iter().any(|arg| arg == "--portal").then(|| {
        format!(
            "{PORTAL_APP_ID}.p{}r{}",
            std::process::id(),
            NEXT_APP.fetch_add(1, Ordering::Relaxed)
        )
    })
}

fn terminate(pid: Option<u32>) {
    let Some(id) = pid else {
        return;
    };
    let Some(pid) = rustix::process::Pid::from_raw(id as i32) else {
        return;
    };
    let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct Flight {
    state: AtomicU8,
    kill: Mutex<Option<Kill>>,
}

impl Flight {
    fn new() -> Self {
        Self {
            state: AtomicU8::new(OPEN),
            kill: Mutex::new(None),
        }
    }

    fn set_kill(&self, kill: Kill) {
        *lock(&self.kill) = Some(Arc::clone(&kill));
        if self.state.load(Ordering::SeqCst) == CLOSED {
            kill();
        }
    }

    fn invoke_kill(&self) {
        if let Some(kill) = lock(&self.kill).clone() {
            kill();
        }
    }

    fn mark_closed(&self) {
        if self
            .state
            .compare_exchange(OPEN, CLOSED, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.invoke_kill();
        }
    }

    fn claim_reply(&self) -> bool {
        self.state
            .compare_exchange(OPEN, REPLIED, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
}

struct Inner {
    spawner: Arc<dyn Spawner>,
    config: Option<PathBuf>,
    flight: Mutex<Option<Arc<Flight>>>,
}

impl Inner {
    fn reserve(&self) -> Option<Arc<Flight>> {
        let mut slot = lock(&self.flight);
        if slot.is_some() {
            return None;
        }
        let flight = Arc::new(Flight::new());
        *slot = Some(Arc::clone(&flight));
        Some(flight)
    }

    fn release(&self, flight: &Arc<Flight>) {
        let mut slot = lock(&self.flight);
        if slot
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, flight))
        {
            *slot = None;
        }
    }
}

struct ScreenshotPortal {
    inner: Arc<Inner>,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Screenshot")]
impl ScreenshotPortal {
    #[zbus(name = "Screenshot")]
    async fn screenshot(
        &self,
        handle: ObjectPath<'_>,
        app_id: String,
        _parent_window: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<Reply> {
        self.dispatch(connection, handle, &app_id, &options, RequestKind::Shot)
            .await
    }

    #[zbus(name = "PickColor")]
    async fn pick_color(
        &self,
        handle: ObjectPath<'_>,
        _app_id: String,
        _parent_window: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<Reply> {
        self.dispatch(connection, handle, "", &options, RequestKind::Color)
            .await
    }

    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property, name = "AvailableTargets")]
    fn available_targets(&self) -> u32 {
        AVAILABLE_TARGETS
    }
}

impl ScreenshotPortal {
    async fn dispatch(
        &self,
        connection: &Connection,
        handle: ObjectPath<'_>,
        app_id: &str,
        options: &HashMap<String, OwnedValue>,
        kind: RequestKind,
    ) -> zbus::fdo::Result<Reply> {
        let Some(flight) = self.inner.reserve() else {
            tracing::debug!("portal request refused because another is open");
            return Ok(other());
        };
        let owned: OwnedObjectPath = handle.to_owned().into();
        let request = PortalRequest {
            flight: Arc::clone(&flight),
            handle: owned.clone(),
        };
        match connection.object_server().at(owned.clone(), request).await {
            Ok(true) => {}
            taken => {
                tracing::warn!(?taken, handle = %owned, "portal request path is already taken");
                self.inner.release(&flight);
                return finish(&flight, other()).await;
            }
        }

        let argv = match self.command(kind, app_id, options).await {
            Some(argv) => argv,
            None => {
                self.unexport(connection, &owned).await;
                self.inner.release(&flight);
                return finish(&flight, other()).await;
            }
        };
        let running = match self.inner.spawner.launch(argv).await {
            Ok(running) => running,
            Err(error) => {
                tracing::warn!(%error, "portal child failed to start");
                self.unexport(connection, &owned).await;
                self.inner.release(&flight);
                return finish(&flight, other()).await;
            }
        };
        flight.set_kill(running.kill);
        let finished = running.wait.await;
        self.unexport(connection, &owned).await;
        self.inner.release(&flight);
        finish(&flight, response_of(kind, &finished)).await
    }

    async fn command(
        &self,
        kind: RequestKind,
        app_id: &str,
        options: &HashMap<String, OwnedValue>,
    ) -> Option<Vec<OsString>> {
        match kind {
            RequestKind::Color => Some(picker_command()),
            RequestKind::Shot => {
                let target = option_target(options);
                if target.is_some_and(|target| {
                    target.count_ones() != 1 || target & AVAILABLE_TARGETS == 0
                }) {
                    return None;
                }
                let mut decision = decide(
                    option_flag(options, "interactive"),
                    option_flag(options, "permission_store_checked"),
                );
                if target == Some(TARGET_AREA) {
                    decision = CaptureKind::Overlay;
                }
                let window = target == Some(TARGET_ACTIVE_WINDOW);
                if window {
                    if !option_flag(options, "permission_store_checked") {
                        return None;
                    }
                    decision = CaptureKind::Headless;
                }
                let label = app_label(app_id);
                match decision {
                    CaptureKind::Overlay => Some(overlay_command(&label)),
                    CaptureKind::Headless => {
                        Some(headless_command(self.inner.config.as_deref(), window))
                    }
                }
            }
        }
    }

    async fn unexport(&self, connection: &Connection, handle: &OwnedObjectPath) {
        let _ = connection
            .object_server()
            .remove::<PortalRequest, _>(handle.clone())
            .await;
    }
}

async fn finish(flight: &Flight, reply: Reply) -> zbus::fdo::Result<Reply> {
    if flight.claim_reply() {
        Ok(reply)
    } else {
        std::future::pending().await
    }
}

struct PortalRequest {
    flight: Arc<Flight>,
    handle: OwnedObjectPath,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Request")]
impl PortalRequest {
    #[zbus(name = "Close")]
    async fn close(
        &self,
        #[zbus(object_server)] object_server: &zbus::ObjectServer,
    ) -> zbus::fdo::Result<()> {
        self.flight.mark_closed();
        let _ = object_server
            .remove::<PortalRequest, _>(self.handle.clone())
            .await;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
