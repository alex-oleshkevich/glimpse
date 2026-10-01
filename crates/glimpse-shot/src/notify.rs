use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;

use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use gettextrs::{gettext, pgettext};
use gio_unix::DesktopAppInfo;
use glimpse_dbus::freedesktop_notifications::FreedesktopNotificationsProxy;
use glimpse_shot::ports::{Notice, Notifier};
use gtk4::gio;
use gtk4::gio::prelude::*;
use tokio::sync::oneshot;
use zbus::Connection;
use zbus::zvariant::Value;

pub const EXPIRE_MS: i32 = 8_000;

const DESKTOP_ENTRY: &str = "me.aresa.Glimpse.Shot";

pub trait Opener: Send + Sync {
    fn launch(&self, connection: &Connection, path: &Path, token: &str) -> BoxFuture<'static, ()>;
}

enum Handler {
    Activatable(String),
    Spawn(gio::AppInfo),
}

type Resolve = Arc<dyn Fn(&Path) -> Option<Handler> + Send + Sync>;

struct Activation {
    app_id: String,
    uri: String,
    token: String,
}

struct Desktop {
    resolve: Resolve,
}

impl Desktop {
    fn system() -> Self {
        Self {
            resolve: Arc::new(system_handler),
        }
    }
}

impl Opener for Desktop {
    fn launch(&self, connection: &Connection, path: &Path, token: &str) -> BoxFuture<'static, ()> {
        let connection = connection.clone();
        let path = path.to_owned();
        let token = token.to_owned();
        let resolve = Arc::clone(&self.resolve);
        Box::pin(async move {
            let prepared =
                match tokio::task::spawn_blocking(move || prepare(&resolve, &path, &token)).await {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        tracing::warn!("cannot open the screenshot: {error}");
                        return;
                    }
                };
            if let Some(activation) = prepared {
                activate(
                    &connection,
                    &activation.app_id,
                    &activation.uri,
                    &activation.token,
                )
                .await;
            }
        })
    }
}

fn prepare(resolve: &Resolve, path: &Path, token: &str) -> Option<Activation> {
    let Some(handler) = resolve(path) else {
        tracing::warn!(path = %path.display(), "no application opens this file");
        return None;
    };
    let uri = file_uri(path);
    match handler {
        Handler::Activatable(app_id) => Some(Activation {
            app_id,
            uri,
            token: token.to_owned(),
        }),
        Handler::Spawn(info) => {
            spawn_app(&info, &uri, token);
            None
        }
    }
}

fn system_handler(path: &Path) -> Option<Handler> {
    let info = gio::AppInfo::default_for_type(&content_type(path)?, false)?;
    match activatable_id(&info) {
        Some(app_id) => Some(Handler::Activatable(app_id)),
        None => Some(Handler::Spawn(info)),
    }
}

fn activatable_id(info: &gio::AppInfo) -> Option<String> {
    let desktop = info.clone().downcast::<DesktopAppInfo>().ok()?;
    if !desktop.boolean("DBusActivatable") {
        return None;
    }
    bus_name_of(&desktop.filename()?)
}

fn bus_name_of(desktop_file: &Path) -> Option<String> {
    let app_id = desktop_file
        .file_name()?
        .to_str()?
        .strip_suffix(".desktop")?;
    zbus::names::WellKnownName::try_from(app_id).ok()?;
    Some(app_id.to_owned())
}

fn content_type(path: &Path) -> Option<String> {
    let file = gio::File::for_path(path);
    let queried = file
        .query_info(
            "standard::content-type",
            gio::FileQueryInfoFlags::NONE,
            None::<&gio::Cancellable>,
        )
        .ok()
        .and_then(|info| info.content_type().map(|kind| kind.to_string()))
        .filter(|kind| !kind.is_empty());
    if let Some(kind) = queried {
        return Some(kind);
    }
    let (kind, _) = gio::content_type_guess(Some(path), None::<&[u8]>);
    let kind = kind.to_string();
    (!kind.is_empty()).then_some(kind)
}

fn spawn_app(info: &gio::AppInfo, uri: &str, token: &str) {
    let context = gio::AppLaunchContext::new();
    if !token.is_empty() {
        context.setenv("XDG_ACTIVATION_TOKEN", token);
        context.setenv("DESKTOP_STARTUP_ID", token);
    }
    if let Err(error) = info.launch_uris(&[uri], Some(&context)) {
        tracing::warn!(%error, uri, "cannot open the screenshot");
    }
}

#[derive(Clone)]
enum Source {
    Session,
    #[cfg(test)]
    Open(Connection),
}

pub struct BusNotifier {
    source: Source,
    opener: Arc<dyn Opener>,
    done: std::sync::Mutex<Option<oneshot::Receiver<()>>>,
    #[cfg(test)]
    notify_returned: tokio::sync::watch::Sender<bool>,
    #[cfg(test)]
    hold_grace: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    grace_release: Arc<tokio::sync::Notify>,
}

impl BusNotifier {
    pub fn session() -> Self {
        Self::with(Source::Session, Arc::new(Desktop::system()))
    }

    #[cfg(test)]
    pub fn on(connection: Connection, opener: Arc<dyn Opener>) -> Self {
        Self::with(Source::Open(connection), opener)
    }

    fn with(source: Source, opener: Arc<dyn Opener>) -> Self {
        Self {
            source,
            opener,
            done: std::sync::Mutex::new(None),
            #[cfg(test)]
            notify_returned: tokio::sync::watch::channel(false).0,
            #[cfg(test)]
            hold_grace: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            grace_release: Arc::new(tokio::sync::Notify::new()),
        }
    }

    #[cfg(test)]
    fn notify_returned(&self) -> tokio::sync::watch::Receiver<bool> {
        self.notify_returned.subscribe()
    }

    #[cfg(test)]
    fn hold_grace(&self) {
        self.hold_grace
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    #[cfg(test)]
    fn release_grace(&self) {
        self.grace_release.notify_one();
    }
}

#[cfg(test)]
type Returned = tokio::sync::watch::Sender<bool>;
#[cfg(not(test))]
type Returned = ();

#[cfg(test)]
struct GraceGate {
    hold: bool,
    release: Arc<tokio::sync::Notify>,
}
#[cfg(not(test))]
struct GraceGate;

impl Notifier for BusNotifier {
    fn notify(&self, notice: Notice) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        *self
            .done
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(rx);
        let source = self.source.clone();
        let opener = Arc::clone(&self.opener);
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| "the notification is posted from the hold runtime".to_owned())?;
        #[cfg(test)]
        let _ = self.notify_returned.send(false);
        #[cfg(test)]
        let returned = self.notify_returned.clone();
        #[cfg(not(test))]
        let returned = ();
        #[cfg(test)]
        let gate = GraceGate {
            hold: self.hold_grace.load(std::sync::atomic::Ordering::SeqCst),
            release: Arc::clone(&self.grace_release),
        };
        #[cfg(not(test))]
        let gate = GraceGate;
        runtime.spawn(async move {
            if let Err(error) = post(source, opener.as_ref(), notice, returned, gate).await {
                tracing::warn!("cannot post the notification: {error}");
            }
            let _ = tx.send(());
        });
        Ok(())
    }

    async fn closed(&self) {
        let done = self
            .done
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(done) = done {
            let _ = done.await;
        }
    }
}

enum Heard {
    Token(String),
    Action(String),
    Closed,
}

fn heard(message: &zbus::message::Message, id: u32) -> Option<Heard> {
    let header = message.header();
    let member = header.member()?;
    match member.as_str() {
        "ActivationToken" => {
            let (got, token) = message.body().deserialize::<(u32, String)>().ok()?;
            (got == id).then_some(Heard::Token(token))
        }
        "ActionInvoked" => {
            let (got, key) = message.body().deserialize::<(u32, String)>().ok()?;
            (got == id).then_some(Heard::Action(key))
        }
        "NotificationClosed" => {
            let (got, _reason) = message.body().deserialize::<(u32, u32)>().ok()?;
            (got == id).then_some(Heard::Closed)
        }
        _ => None,
    }
}

async fn post(
    source: Source,
    opener: &dyn Opener,
    notice: Notice,
    returned: Returned,
    gate: GraceGate,
) -> Result<(), String> {
    let connection = connect(&source).await?;
    let proxy = FreedesktopNotificationsProxy::new(&connection)
        .await
        .map_err(|error| error.to_string())?;
    let mut signals = within(async {
        proxy
            .inner()
            .receive_all_signals()
            .await
            .map_err(|error| error.to_string())
    })
    .await?;
    let id = send(&proxy, &notice).await?;
    arm_returned(&returned);
    wait_to_arm(&gate).await;
    let mut grace = std::pin::pin!(tokio::time::sleep(crate::hold::NOTIFY_GRACE));
    let mut token = String::new();
    loop {
        tokio::select! {
            biased;
            message = signals.next() => match message {
                Some(message) => match heard(&message, id) {
                    Some(Heard::Token(value)) => token = value,
                    Some(Heard::Action(key)) => {
                        if invoked(&connection, opener, &notice, &key, &token).await {
                            return Ok(());
                        }
                    }
                    Some(Heard::Closed) => return Ok(()),
                    None => {}
                },
                None => return Ok(()),
            },
            _ = &mut grace => return Ok(()),
        }
    }
}

#[cfg(test)]
fn arm_returned(returned: &Returned) {
    let _ = returned.send(true);
}

#[cfg(not(test))]
fn arm_returned(_returned: &Returned) {}

#[cfg(test)]
async fn wait_to_arm(gate: &GraceGate) {
    if gate.hold {
        gate.release.notified().await;
    }
}

#[cfg(not(test))]
async fn wait_to_arm(_gate: &GraceGate) {}

async fn connect(source: &Source) -> Result<Connection, String> {
    within(async {
        match source {
            Source::Session => zbus::Connection::session()
                .await
                .map_err(|error| error.to_string()),
            #[cfg(test)]
            Source::Open(connection) => Ok(connection.clone()),
        }
    })
    .await
}

async fn send(proxy: &FreedesktopNotificationsProxy<'_>, notice: &Notice) -> Result<u32, String> {
    let name = gettext("Screenshot");
    let summary = summary(&notice.summary);
    let body = escaped_body(&notice.body);
    let labels = labels(notice.actions);
    let actions: Vec<&str> = labels.iter().map(String::as_str).collect();
    let image = notice
        .image
        .as_deref()
        .map(file_uri)
        .filter(|uri| !uri.is_empty());
    let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
    hints.insert("transient", Value::from(true));
    hints.insert("desktop-entry", Value::from(DESKTOP_ENTRY));
    if let Some(uri) = image.as_deref() {
        hints.insert("image-path", Value::from(uri));
    }
    within(async {
        proxy
            .notify(&name, 0, "", &summary, &body, &actions, hints, EXPIRE_MS)
            .await
            .map_err(|error| error.to_string())
    })
    .await
}

async fn invoked(
    connection: &Connection,
    opener: &dyn Opener,
    notice: &Notice,
    key: &str,
    token: &str,
) -> bool {
    let Some(path) = notice.image.as_deref() else {
        return matches!(key, "default" | "folder");
    };
    match key {
        "default" => {
            opener.launch(connection, path, token).await;
            true
        }
        "folder" => {
            reveal(connection, path, token, opener).await;
            true
        }
        _ => false,
    }
}

async fn reveal(connection: &Connection, path: &Path, token: &str, opener: &dyn Opener) {
    let uri = file_uri(path);
    let shown = async {
        let proxy = FileManager1Proxy::new(connection)
            .await
            .map_err(|error| error.to_string())?;
        proxy
            .show_items(&[uri.as_str()], token)
            .await
            .map_err(|error| error.to_string())
    };
    let failed = match tokio::time::timeout(glimpse_dbus::DEADLINE, shown).await {
        Ok(Ok(())) => false,
        Ok(Err(error)) => {
            tracing::warn!(%error, "the file manager did not show the screenshot");
            true
        }
        Err(_) => {
            tracing::warn!("the file manager did not answer in time");
            true
        }
    };
    if failed
        && let Some(folder) = path
            .parent()
            .filter(|folder| !folder.as_os_str().is_empty())
    {
        opener.launch(connection, folder, token).await;
    }
}

async fn activate(connection: &Connection, app_id: &str, uri: &str, token: &str) {
    let path = application_path(app_id);
    let call = async {
        let proxy = FreedesktopApplicationProxy::builder(connection)
            .destination(app_id)
            .map_err(|error| error.to_string())?
            .path(path.as_str())
            .map_err(|error| error.to_string())?
            .build()
            .await
            .map_err(|error| error.to_string())?;
        let mut platform_data = HashMap::new();
        if !token.is_empty() {
            platform_data.insert("activation-token", Value::from(token));
            platform_data.insert("desktop-startup-id", Value::from(token));
        }
        proxy
            .open(&[uri], platform_data)
            .await
            .map_err(|error| error.to_string())
    };
    match tokio::time::timeout(glimpse_dbus::DEADLINE, call).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!(%error, app_id, "cannot open the screenshot"),
        Err(_) => tracing::warn!(app_id, "the application did not answer in time"),
    }
}

fn application_path(app_id: &str) -> String {
    format!("/{}", app_id.replace('.', "/").replace('-', "_"))
}

fn summary(summary: &str) -> String {
    match summary {
        "Screenshot saved" => gettext("Screenshot saved"),
        "Screenshot copied" => gettext("Screenshot copied"),
        other => other.to_owned(),
    }
}

fn labels(actions: bool) -> Vec<String> {
    if !actions {
        return Vec::new();
    }
    vec![
        "default".to_owned(),
        pgettext("notification action", "Open"),
        "folder".to_owned(),
        gettext("Show in Folder"),
    ]
}

fn escaped_body(body: &str) -> String {
    let escaped = body
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    glimpse_utils::markup::sanitize_body(&escaped)
}

fn file_uri(path: &Path) -> String {
    gio::File::for_path(path).uri().to_string()
}

async fn within<T>(work: impl Future<Output = Result<T, String>>) -> Result<T, String> {
    match tokio::time::timeout(glimpse_dbus::DEADLINE, work).await {
        Ok(result) => result,
        Err(_) => Err("the notification daemon did not answer in time".to_owned()),
    }
}

#[zbus::proxy(interface = "org.freedesktop.Application")]
trait FreedesktopApplication {
    fn open(&self, uris: &[&str], platform_data: HashMap<&str, Value<'_>>) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.FileManager1",
    default_service = "org.freedesktop.FileManager1",
    default_path = "/org/freedesktop/FileManager1"
)]
trait FileManager1 {
    fn show_items(&self, uris: &[&str], startup_id: &str) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests;
