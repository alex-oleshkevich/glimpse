use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{Context as _, Result, bail};
use chrono::Local;
use futures_util::future::BoxFuture;
use glimpse_config::{FileTemplate, ScreenshotConfig};
use glimpse_shot::doc::naming::file_name;
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
    let screenshot = config.screenshot;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        let connection = zbus::Connection::session().await?;
        glimpse_dbus::own_name(&connection, BUS_NAME)
            .await
            .context("me.aresa.Glimpse.Shot.Portal is already owned")?;
        serve_at(&connection, Arc::new(ProcessSpawner), screenshot).await?;
        tracing::info!(bus_name = BUS_NAME, "serving the screenshot portal");
        std::future::pending::<Result<()>>().await
    })
}

fn screenshot_directory(config: &ScreenshotConfig) -> Result<PathBuf> {
    match crate::finish::Destination::of(None, false, config)? {
        crate::finish::Destination::Directory(directory) => Ok(directory),
        crate::finish::Destination::File(path) => path
            .parent()
            .map(Path::to_path_buf)
            .context("screenshot destination has no directory"),
        crate::finish::Destination::Stdout | crate::finish::Destination::Nowhere => {
            bail!("screenshot destination is not a directory")
        }
    }
}

async fn serve_at(
    connection: &Connection,
    spawner: Arc<dyn Spawner>,
    screenshot: ScreenshotConfig,
) -> Result<()> {
    let portal = ScreenshotPortal {
        inner: Arc::new(Inner {
            spawner,
            screenshot,
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

fn headless_command(output: &Path) -> Vec<OsString> {
    vec![
        executable("glimpse-shot").into(),
        OsString::from("capture"),
        OsString::from("--screen"),
        OsString::from("--json"),
        OsString::from("-o"),
        output.as_os_str().to_os_string(),
    ]
}

fn picker_command() -> Vec<OsString> {
    vec![
        executable("glimpse-picker").into(),
        OsString::from("--json"),
    ]
}

fn headless_output(directory: &Path, template: &FileTemplate) -> PathBuf {
    let path = directory.join(file_name(template, Local::now()));
    match std::path::absolute(&path) {
        Ok(absolute) => absolute,
        Err(_) => path,
    }
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
            uri_reply(file_uri(&path)?)
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
    screenshot: ScreenshotConfig,
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
        2
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
                let decision = decide(
                    option_flag(options, "interactive"),
                    option_flag(options, "permission_store_checked"),
                );
                let label = app_label(app_id);
                match decision {
                    CaptureKind::Overlay => Some(overlay_command(&label)),
                    CaptureKind::Headless => {
                        let directory = match screenshot_directory(&self.inner.screenshot) {
                            Ok(directory) => directory,
                            Err(error) => {
                                tracing::warn!(%error, "cannot resolve the screenshot directory");
                                return None;
                            }
                        };
                        let output = headless_output(&directory, &self.inner.screenshot.filename);
                        if let Some(parent) = output.parent()
                            && let Err(error) = tokio::fs::create_dir_all(parent).await
                        {
                            tracing::warn!(
                                %error,
                                directory = %parent.display(),
                                "cannot create the screenshot directory"
                            );
                            return None;
                        }
                        Some(headless_command(&output))
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
mod tests {
    use std::collections::VecDeque;
    use std::time::Duration;

    use glimpse_dbus::testing::PrivateBus;
    use glimpse_shot::doc::wire::TargetV1;
    use tokio::sync::{oneshot, watch};
    use zbus::zvariant::Value;

    use super::*;

    #[test]
    fn a_screenshot_is_silent_only_when_the_portal_already_checked_permission() {
        assert_eq!(decide(true, false), CaptureKind::Overlay);
        assert_eq!(decide(true, true), CaptureKind::Overlay);
        assert_eq!(decide(false, false), CaptureKind::Overlay);
        assert_eq!(decide(false, true), CaptureKind::Headless);
    }

    #[test]
    fn a_missing_or_mistyped_option_is_false() {
        let mut options = HashMap::new();
        assert!(!option_flag(&options, "interactive"));
        assert!(!option_flag(&options, "permission_store_checked"));
        options.insert(
            "interactive".to_owned(),
            OwnedValue::try_from(Value::from("true")).expect("a string"),
        );
        assert!(!option_flag(&options, "interactive"));
        options.insert(
            "permission_store_checked".to_owned(),
            OwnedValue::try_from(Value::from(true)).expect("a bool"),
        );
        assert!(option_flag(&options, "permission_store_checked"));
    }

    #[test]
    fn the_app_name_is_capped_and_an_empty_one_reads_an_application() {
        let raw = format!("<b>{}</b>", "x".repeat(300));
        assert_eq!(app_label(&raw), glimpse_utils::text::clean(&raw, APP_CAP));
        assert!(app_label(&raw).chars().count() < raw.chars().count());
        assert_eq!(app_label(""), gettextrs::gettext("An application"));
        assert_eq!(app_label(" \n\t"), gettextrs::gettext("An application"));
        assert_eq!(app_label("<b>GIMP</b>"), "<b>GIMP</b>");
    }

    #[test]
    fn a_file_uri_is_absolute_and_percent_encoded() {
        assert_eq!(file_uri(Path::new("shot.png")), None);
        assert_eq!(
            file_uri(Path::new("/tmp/my shot.png")).as_deref(),
            Some("file:///tmp/my%20shot.png")
        );
        assert_eq!(
            file_uri(Path::new("/tmp/déjà vu.png")).as_deref(),
            Some("file:///tmp/d%C3%A9j%C3%A0%20vu.png")
        );
    }

    #[test]
    fn child_exit_codes_map_onto_portal_responses() {
        let shot = result_stdout("/tmp/shot.png");
        let saved = response_of(
            RequestKind::Shot,
            &Finished {
                stdout: shot.into_bytes(),
                code: Some(0),
            },
        );
        assert_eq!(saved.0, SUCCESS);
        assert_eq!(
            saved
                .1
                .get("uri")
                .and_then(|value| String::try_from(value.clone()).ok()),
            Some("file:///tmp/shot.png".to_owned())
        );

        let refused = response_of(
            RequestKind::Shot,
            &Finished {
                stdout: Vec::new(),
                code: Some(4),
            },
        );
        assert_eq!(refused.0, CANCELLED);
        assert!(refused.1.is_empty());

        for code in [None, Some(1), Some(7), Some(9)] {
            let failed = response_of(
                RequestKind::Shot,
                &Finished {
                    stdout: result_stdout("/tmp/shot.png").into_bytes(),
                    code,
                },
            );
            assert_eq!(failed.0, OTHER, "{code:?}");
            assert!(failed.1.is_empty());
        }

        let garbage = response_of(
            RequestKind::Shot,
            &Finished {
                stdout: b"not json".to_vec(),
                code: Some(0),
            },
        );
        assert_eq!(garbage.0, OTHER);

        let relative = response_of(
            RequestKind::Shot,
            &Finished {
                stdout: result_stdout("shot.png").into_bytes(),
                code: Some(0),
            },
        );
        assert_eq!(relative.0, OTHER);

        let color = response_of(
            RequestKind::Color,
            &Finished {
                stdout:
                    br#"{"format":"rgb","value":"rgb(224 86 63)","red":224,"green":86,"blue":63}"#
                        .to_vec(),
                code: Some(0),
            },
        );
        assert_eq!(color.0, SUCCESS);
        let (red, green, blue) =
            <(f64, f64, f64)>::try_from(color.1.get("color").expect("color").clone())
                .expect("(ddd)");
        assert_eq!(
            (red, green, blue),
            (224.0 / 255.0, 86.0 / 255.0, 63.0 / 255.0)
        );

        let cancelled_color = response_of(
            RequestKind::Color,
            &Finished {
                stdout: Vec::new(),
                code: Some(4),
            },
        );
        assert_eq!(cancelled_color.0, CANCELLED);
    }

    #[test]
    fn an_executable_beside_this_binary_wins_over_path() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let current = dir.path().join("glimpse-shot");
        let picker = dir.path().join("glimpse-picker");
        std::fs::write(&current, b"").expect("writes");
        std::fs::write(&picker, b"").expect("writes");
        assert_eq!(resolve_executable("glimpse-picker", Some(&current)), picker);
        assert_eq!(
            resolve_executable("missing", Some(&current)),
            PathBuf::from("missing")
        );
    }

    #[derive(Clone)]
    enum Script {
        Done { stdout: String, code: i32 },
        Hang,
    }

    struct Fake {
        scripts: Mutex<VecDeque<Script>>,
        argv: Mutex<Vec<Vec<OsString>>>,
        launches: watch::Sender<usize>,
        killed: Arc<std::sync::atomic::AtomicBool>,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                scripts: Mutex::new(VecDeque::new()),
                argv: Mutex::new(Vec::new()),
                launches: watch::channel(0).0,
                killed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            }
        }

        fn push(&self, script: Script) {
            lock(&self.scripts).push_back(script);
        }

        fn argv(&self) -> Vec<Vec<OsString>> {
            lock(&self.argv).clone()
        }
    }

    impl Spawner for Fake {
        fn launch(&self, argv: Vec<OsString>) -> BoxFuture<'static, std::io::Result<Running>> {
            let script = lock(&self.scripts).pop_front();
            let Some(script) = script else {
                return Box::pin(async { Err(std::io::Error::other("no scripted child")) });
            };
            lock(&self.argv).push(argv);
            let killed = Arc::clone(&self.killed);
            let running = match script {
                Script::Done { stdout, code } => Running {
                    kill: Arc::new(move || {
                        killed.store(true, Ordering::SeqCst);
                    }),
                    wait: Box::pin(async move {
                        Finished {
                            stdout: stdout.into_bytes(),
                            code: Some(code),
                        }
                    }),
                },
                Script::Hang => {
                    let (sender, receiver) = oneshot::channel();
                    let sender = Arc::new(Mutex::new(Some(sender)));
                    let held = Arc::clone(&sender);
                    Running {
                        kill: Arc::new(move || {
                            killed.store(true, Ordering::SeqCst);
                            if let Some(sender) = lock(&held).take() {
                                let _ = sender.send(());
                            }
                        }),
                        wait: Box::pin(async move {
                            let _ = receiver.await;
                            Finished {
                                stdout: Vec::new(),
                                code: None,
                            }
                        }),
                    }
                }
            };
            let count = self.launches.borrow().wrapping_add(1);
            self.launches.send_replace(count);
            Box::pin(async move { Ok(running) })
        }
    }

    struct Started {
        _bus: PrivateBus,
        _server: Connection,
        directory: tempfile::TempDir,
        client: Connection,
        fake: Arc<Fake>,
    }

    impl Started {
        async fn new() -> Self {
            Self::with_directory(None).await
        }

        async fn with_directory(override_directory: Option<PathBuf>) -> Self {
            let directory = tempfile::tempdir().expect("a temporary directory");
            let fake = Arc::new(Fake::new());
            let bus = PrivateBus::isolated();
            let server = bus.connection().await;
            let template = FileTemplate::try_from("shot.png".to_owned()).expect("template");
            let screenshot = ScreenshotConfig {
                directory: Some(
                    override_directory.unwrap_or_else(|| directory.path().to_path_buf()),
                ),
                filename: template,
                ..ScreenshotConfig::default()
            };
            glimpse_dbus::own_name(&server, BUS_NAME)
                .await
                .expect("the portal name");
            serve_at(&server, Arc::clone(&fake) as Arc<dyn Spawner>, screenshot)
                .await
                .expect("the portal is served");
            let client = bus.connection().await;
            Self {
                _bus: bus,
                _server: server,
                directory,
                client,
                fake,
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unavailable_screenshot_directory_does_not_stop_color_or_interactive_requests() {
        let started = Started::with_directory(Some(PathBuf::from("/dev/null/glimpse-shot"))).await;
        let proxy = ShotPortalProxy::new(&started.client)
            .await
            .expect("a proxy");

        started.fake.push(Script::Done {
            stdout: r#"{"red":1,"green":2,"blue":3}"#.to_owned(),
            code: 0,
        });
        let (color, _) = proxy
            .pick_color(&handle("/req/no_folder_color"), "", "", HashMap::new())
            .await
            .expect("PickColor");
        assert_eq!(color, SUCCESS);

        started.fake.push(Script::Done {
            stdout: result_stdout("/tmp/portal-overlaid.png"),
            code: 0,
        });
        let (interactive, _) = proxy
            .screenshot(
                &handle("/req/no_folder_interactive"),
                "org.gimp.GIMP",
                "",
                flags(&[("interactive", true)]),
            )
            .await
            .expect("interactive Screenshot");
        assert_eq!(interactive, SUCCESS);

        let (headless, _) = proxy
            .screenshot(
                &handle("/req/no_folder_headless"),
                "org.gimp.GIMP",
                "",
                flags(&[("permission_store_checked", true)]),
            )
            .await
            .expect("headless Screenshot");
        assert_eq!(headless, OTHER);
        assert_eq!(started.fake.argv().len(), 2);
    }

    #[zbus::proxy(
        interface = "org.freedesktop.impl.portal.Screenshot",
        default_service = "me.aresa.Glimpse.Shot.Portal",
        default_path = "/org/freedesktop/portal/desktop"
    )]
    trait ShotPortal {
        #[zbus(name = "Screenshot")]
        fn screenshot(
            &self,
            handle: &ObjectPath<'_>,
            app_id: &str,
            parent_window: &str,
            options: HashMap<&str, Value<'_>>,
        ) -> zbus::Result<Reply>;

        #[zbus(name = "PickColor")]
        fn pick_color(
            &self,
            handle: &ObjectPath<'_>,
            app_id: &str,
            parent_window: &str,
            options: HashMap<&str, Value<'_>>,
        ) -> zbus::Result<Reply>;

        #[zbus(property, name = "version")]
        fn version(&self) -> zbus::Result<u32>;
    }

    #[zbus::proxy(interface = "org.freedesktop.impl.portal.Request")]
    trait PortalRequestTest {
        #[zbus(name = "Close")]
        fn close(&self) -> zbus::Result<()>;
    }

    fn flags<'a>(pairs: &'a [(&'a str, bool)]) -> HashMap<&'a str, Value<'a>> {
        pairs
            .iter()
            .map(|(key, value)| (*key, Value::from(*value)))
            .collect()
    }

    fn handle(path: &str) -> ObjectPath<'static> {
        ObjectPath::try_from(path)
            .expect("an object path")
            .into_owned()
    }

    fn result_stdout(path: &str) -> String {
        serde_json::to_string(&ResultV1 {
            version: 1,
            path: Some(PathBuf::from(path)),
            copied: false,
            width: 8,
            height: 8,
            scale: 1.0,
            output: Some("DP-1".to_owned()),
            target: TargetV1::Screen {
                name: "DP-1".to_owned(),
            },
            crop: None,
            dropped: 0,
            annotations: Vec::new(),
        })
        .expect("result json")
    }

    fn program_name(argv: &[OsString]) -> Option<&str> {
        Path::new(argv.first()?)
            .file_name()
            .and_then(|name| name.to_str())
    }

    async fn close_kills_the_child_and_the_screenshot_call_does_not_return_impl() {
        let started = Started::new().await;
        let proxy = ShotPortalProxy::new(&started.client)
            .await
            .expect("a proxy");
        started.fake.push(Script::Hang);
        let mut seen = started.fake.launches.subscribe();
        let client = started.client.clone();
        let call = tokio::spawn(async move {
            let proxy = ShotPortalProxy::new(&client).await.expect("a proxy");
            let path = handle("/req/close");
            proxy
                .screenshot(&path, "org.gimp.GIMP", "", flags(&[("interactive", true)]))
                .await
        });
        tokio::time::timeout(Duration::from_secs(2), seen.wait_for(|count| *count >= 1))
            .await
            .expect("the first child starts promptly")
            .expect("the child was spawned");

        let request = PortalRequestTestProxy::builder(&started.client)
            .path(handle("/req/close"))
            .expect("a path")
            .destination(BUS_NAME)
            .expect("a destination")
            .build()
            .await
            .expect("a request proxy");
        tokio::time::timeout(Duration::from_secs(2), request.close())
            .await
            .expect("Close returns promptly")
            .expect("Close returns");
        let killed = tokio::time::timeout(Duration::from_secs(2), async {
            while !started.fake.killed.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await;
        assert!(killed.is_ok(), "Close sends SIGTERM to the child");

        let pending = tokio::time::timeout(Duration::from_millis(400), call).await;
        assert!(
            pending.is_err(),
            "a closed request sends no Screenshot response"
        );

        started.fake.push(Script::Done {
            stdout: result_stdout("/tmp/after-close.png"),
            code: 0,
        });
        let again = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let reply = proxy
                    .screenshot(&handle("/req/after"), "org.gimp.GIMP", "", HashMap::new())
                    .await
                    .expect("a reply");
                if reply.0 == SUCCESS {
                    return reply;
                }
                assert_eq!(reply.0, OTHER);
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the slot is free after Close");
        assert_eq!(again.0, SUCCESS);
    }

    async fn pick_color_returns_fractions_of_255_and_cancel_is_response_1_impl() {
        let started = Started::new().await;
        let proxy = ShotPortalProxy::new(&started.client)
            .await
            .expect("a proxy");
        started.fake.push(Script::Done {
            stdout: r#"{"format":"rgb","value":"rgb(224 86 63)","red":224,"green":86,"blue":63}"#
                .to_owned(),
            code: 0,
        });
        let (response, results) = proxy
            .pick_color(
                &handle("/req/color"),
                "org.inkscape.Inkscape",
                "",
                HashMap::new(),
            )
            .await
            .expect("PickColor");
        assert_eq!(response, SUCCESS);
        let (red, green, blue) =
            <(f64, f64, f64)>::try_from(results.get("color").expect("color").clone())
                .expect("(ddd)");
        assert_eq!(
            (red, green, blue),
            (224.0 / 255.0, 86.0 / 255.0, 63.0 / 255.0)
        );
        assert_eq!(
            program_name(&started.fake.argv()[0]),
            Some("glimpse-picker")
        );
        assert_eq!(started.fake.argv()[0][1], OsString::from("--json"));

        started.fake.push(Script::Done {
            stdout: String::new(),
            code: 4,
        });
        let (response, results) = proxy
            .pick_color(&handle("/req/color_cancel"), "", "", HashMap::new())
            .await
            .expect("PickColor");
        assert_eq!(response, CANCELLED);
        assert!(results.is_empty());

        started.fake.push(Script::Done {
            stdout: "nope".to_owned(),
            code: 1,
        });
        let (response, results) = proxy
            .pick_color(&handle("/req/color_fail"), "", "", HashMap::new())
            .await
            .expect("PickColor");
        assert_eq!(response, OTHER);
        assert!(results.is_empty());
    }

    async fn a_second_request_while_one_is_open_returns_2_impl() {
        let started = Started::new().await;
        let proxy = ShotPortalProxy::new(&started.client)
            .await
            .expect("a proxy");
        started.fake.push(Script::Hang);
        let mut seen = started.fake.launches.subscribe();
        let client = started.client.clone();
        let first = tokio::spawn(async move {
            let proxy = ShotPortalProxy::new(&client).await.expect("a proxy");
            let path = handle("/req/busy");
            proxy
                .screenshot(&path, "org.gimp.GIMP", "", flags(&[("interactive", true)]))
                .await
        });
        tokio::time::timeout(Duration::from_secs(2), seen.wait_for(|count| *count >= 1))
            .await
            .expect("the first child starts promptly")
            .expect("the first child was spawned");

        let (response, results) = tokio::time::timeout(
            Duration::from_secs(2),
            proxy.pick_color(&handle("/req/busy_2"), "org.other.App", "", HashMap::new()),
        )
        .await
        .expect("a busy request returns at once")
        .expect("the second call returns");
        assert_eq!(response, OTHER);
        assert!(results.is_empty());
        assert_eq!(started.fake.argv().len(), 1, "the busy call spawns nothing");

        let request = PortalRequestTestProxy::builder(&started.client)
            .path(handle("/req/busy"))
            .expect("a path")
            .destination(BUS_NAME)
            .expect("a destination")
            .build()
            .await
            .expect("a request proxy");
        tokio::time::timeout(Duration::from_secs(2), request.close())
            .await
            .expect("Close returns promptly")
            .expect("Close");
        let pending = tokio::time::timeout(Duration::from_millis(400), first).await;
        assert!(pending.is_err());
    }

    async fn the_spawned_child_follows_the_decision_and_the_child_result_impl() {
        let started = Started::new().await;
        let proxy = ShotPortalProxy::new(&started.client)
            .await
            .expect("a proxy");
        assert_eq!(proxy.version().await.expect("version"), 2);

        let raw = format!("<b>{}</b>", "é".repeat(300));
        started.fake.push(Script::Done {
            stdout: result_stdout("/tmp/portal overlay.png"),
            code: 0,
        });
        let (response, results) = proxy
            .screenshot(
                &handle("/req/name"),
                &raw,
                "ignored",
                flags(&[("interactive", true), ("permission_store_checked", true)]),
            )
            .await
            .expect("Screenshot");
        assert_eq!(response, SUCCESS);
        assert_eq!(
            results
                .get("uri")
                .and_then(|value| String::try_from(value.clone()).ok())
                .as_deref(),
            Some("file:///tmp/portal%20overlay.png")
        );
        let argv = started.fake.argv();
        assert_eq!(program_name(&argv[0]), Some("glimpse-shot"));
        assert_eq!(
            &argv[0][1..],
            &[
                OsString::from("--screen"),
                OsString::from("--json"),
                OsString::from("--portal"),
                OsString::from(app_label(&raw)),
                OsString::from("--timeout"),
                OsString::from(OVERLAY_TIMEOUT_SECS.to_string()),
            ]
        );

        started.fake.push(Script::Done {
            stdout: result_stdout("/tmp/empty-app.png"),
            code: 0,
        });
        proxy
            .screenshot(
                &handle("/req/empty"),
                "   ",
                "",
                flags(&[("interactive", false)]),
            )
            .await
            .expect("Screenshot");
        let argv = started.fake.argv();
        let fallback = gettextrs::gettext("An application");
        assert_eq!(
            argv[1]
                .windows(2)
                .find(|pair| pair[0] == "--portal")
                .map(|pair| pair[1].as_os_str()),
            Some(std::ffi::OsStr::new(fallback.as_str()))
        );
        assert_eq!(argv[1][1], OsString::from("--screen"));

        let output = started.directory.path().join("shot.png");
        started.fake.push(Script::Done {
            stdout: result_stdout(output.to_str().expect("utf-8")),
            code: 0,
        });
        let (response, results) = proxy
            .screenshot(
                &handle("/req/headless"),
                "org.gimp.GIMP",
                "",
                flags(&[("interactive", false), ("permission_store_checked", true)]),
            )
            .await
            .expect("Screenshot");
        assert_eq!(response, SUCCESS);
        assert_eq!(
            results
                .get("uri")
                .and_then(|value| String::try_from(value.clone()).ok())
                .as_deref(),
            file_uri(&output).as_deref()
        );
        let argv = &started.fake.argv()[2];
        assert_eq!(program_name(argv), Some("glimpse-shot"));
        assert_eq!(argv[1], OsString::from("capture"));
        assert_eq!(argv[2], OsString::from("--screen"));
        assert_eq!(argv[3], OsString::from("--json"));
        assert_eq!(argv[4], OsString::from("-o"));
        assert_eq!(argv[5], output.as_os_str());
        assert!(output.starts_with(started.directory.path()));

        started.fake.push(Script::Done {
            stdout: String::new(),
            code: 4,
        });
        let (response, results) = proxy
            .screenshot(
                &handle("/req/esc"),
                "org.gimp.GIMP",
                "",
                flags(&[("interactive", true)]),
            )
            .await
            .expect("Screenshot");
        assert_eq!(response, CANCELLED);
        assert!(results.is_empty());

        started.fake.push(Script::Done {
            stdout: "crash".to_owned(),
            code: 9,
        });
        let (response, results) = proxy
            .screenshot(&handle("/req/crash"), "org.gimp.GIMP", "", HashMap::new())
            .await
            .expect("Screenshot");
        assert_eq!(response, OTHER);
        assert!(results.is_empty());
    }

    #[test]
    fn overlay_and_headless_commands_name_the_binaries() {
        let overlay = overlay_command("GIMP");
        assert_eq!(program_name(&overlay), Some("glimpse-shot"));
        assert_eq!(
            &overlay[1..],
            &[
                OsString::from("--screen"),
                OsString::from("--json"),
                OsString::from("--portal"),
                OsString::from("GIMP"),
                OsString::from("--timeout"),
                OsString::from(OVERLAY_TIMEOUT_SECS.to_string()),
            ]
        );
        let output = Path::new("/tmp/shots/shot.png");
        let headless = headless_command(output);
        assert_eq!(headless[1], OsString::from("capture"));
        assert_eq!(headless[5], output.as_os_str());
        let picker = picker_command();
        assert_eq!(program_name(&picker), Some("glimpse-picker"));
        assert_eq!(picker[1], OsString::from("--json"));

        let app_id = |command: &Command| {
            command
                .as_std()
                .get_envs()
                .find(|(name, _)| *name == std::ffi::OsStr::new(APP_ID_VARIABLE))
                .and_then(|(_, value)| value)
                .and_then(|value| value.to_str())
                .map(str::to_owned)
        };
        let first = child_command(overlay.clone()).expect("portal child command");
        let second = child_command(overlay).expect("next portal child command");
        let first_id = app_id(&first).expect("portal child ID");
        let second_id = app_id(&second).expect("next portal child ID");
        assert!(gtk4::gio::Application::id_is_valid(&first_id));
        assert!(gtk4::gio::Application::id_is_valid(&second_id));
        assert_ne!(first_id, "me.aresa.Glimpse.Shot");
        assert_ne!(first_id, second_id);
        assert!(app_id(&child_command(headless).expect("headless command")).is_none());
        assert!(app_id(&child_command(picker).expect("picker command")).is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn close_kills_the_child_and_the_screenshot_call_does_not_return() {
        tokio::time::timeout(
            Duration::from_secs(10),
            close_kills_the_child_and_the_screenshot_call_does_not_return_impl(),
        )
        .await
        .expect("Close test finishes on the private bus");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pick_color_returns_fractions_of_255_and_cancel_is_response_1() {
        tokio::time::timeout(
            Duration::from_secs(10),
            pick_color_returns_fractions_of_255_and_cancel_is_response_1_impl(),
        )
        .await
        .expect("PickColor test finishes on the private bus");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_second_request_while_one_is_open_returns_2() {
        tokio::time::timeout(
            Duration::from_secs(10),
            a_second_request_while_one_is_open_returns_2_impl(),
        )
        .await
        .expect("busy request test finishes on the private bus");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_spawned_child_follows_the_decision_and_the_child_result() {
        tokio::time::timeout(
            Duration::from_secs(10),
            the_spawned_child_follows_the_decision_and_the_child_result_impl(),
        )
        .await
        .expect("Screenshot flow test finishes on the private bus");
    }
}
