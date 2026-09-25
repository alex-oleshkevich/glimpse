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
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use glimpse_dbus::testing::PrivateBus;
    use glimpse_shot::ports::Notice;
    use tokio::sync::{Notify, watch};
    use zbus::connection::Connection;
    use zbus::object_server::SignalEmitter;
    use zbus::zvariant::OwnedValue;

    use super::*;

    const PATH: &str = "/org/freedesktop/Notifications";
    const NAME: &str = "org.freedesktop.Notifications";
    const VIEWER: &str = "org.example.Image-Viewer";

    #[derive(Clone)]
    struct Posted {
        id: u32,
        app_name: String,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    }

    struct Server {
        tx: watch::Sender<Option<Posted>>,
        next: u32,
        delay: Duration,
    }

    #[zbus::interface(name = "org.freedesktop.Notifications")]
    impl Server {
        #[allow(clippy::too_many_arguments)]
        async fn notify(
            &mut self,
            app_name: String,
            _replaces_id: u32,
            app_icon: String,
            summary: String,
            body: String,
            actions: Vec<String>,
            hints: HashMap<String, OwnedValue>,
            expire_timeout: i32,
        ) -> u32 {
            tokio::time::sleep(self.delay).await;
            self.next = self.next.saturating_add(1).max(1);
            let id = self.next;
            let _ = self.tx.send(Some(Posted {
                id,
                app_name,
                app_icon,
                summary,
                body,
                actions,
                hints,
                expire_timeout,
            }));
            id
        }

        async fn close_notification(&mut self, _id: u32) {}

        #[zbus(signal)]
        async fn action_invoked(
            emitter: &SignalEmitter<'_>,
            id: u32,
            action_key: &str,
        ) -> zbus::Result<()>;

        #[zbus(signal)]
        async fn activation_token(
            emitter: &SignalEmitter<'_>,
            id: u32,
            token: &str,
        ) -> zbus::Result<()>;

        #[zbus(signal)]
        async fn notification_closed(
            emitter: &SignalEmitter<'_>,
            id: u32,
            reason: u32,
        ) -> zbus::Result<()>;
    }

    #[derive(Clone, PartialEq, Eq, Debug)]
    struct Opened {
        uris: Vec<String>,
        activation_token: String,
        startup_id: String,
    }

    struct Viewer {
        entered: watch::Sender<bool>,
        release: Arc<Notify>,
        calls: watch::Sender<Option<Opened>>,
    }

    #[zbus::interface(name = "org.freedesktop.Application")]
    impl Viewer {
        async fn open(&mut self, uris: Vec<String>, platform_data: HashMap<String, OwnedValue>) {
            let _ = self.entered.send(true);
            self.release.notified().await;
            let _ = self.calls.send(Some(Opened {
                uris,
                activation_token: value_str(&platform_data, "activation-token"),
                startup_id: value_str(&platform_data, "desktop-startup-id"),
            }));
        }
    }

    fn value_str(data: &HashMap<String, OwnedValue>, key: &str) -> String {
        data.get(key)
            .and_then(|value| <&str>::try_from(value).ok())
            .unwrap_or("")
            .to_owned()
    }

    struct Fixture {
        server: Connection,
        client: Connection,
        incoming: watch::Receiver<Option<Posted>>,
        _bus: PrivateBus,
    }

    impl Fixture {
        async fn start() -> Self {
            Self::answering_after(Duration::ZERO).await
        }

        async fn answering_after(delay: Duration) -> Self {
            let bus = PrivateBus::isolated();
            let (tx, incoming) = watch::channel(None);
            let server = bus.connection().await;
            server
                .object_server()
                .at(PATH, Server { tx, next: 0, delay })
                .await
                .expect("the fixture is exported");
            glimpse_dbus::own_name(&server, NAME)
                .await
                .expect("the fixture owns the name");
            let client = bus.connection().await;
            Self {
                server,
                client,
                incoming,
                _bus: bus,
            }
        }

        fn notifier(&self, opener: Arc<dyn Opener>) -> BusNotifier {
            BusNotifier::on(self.client.clone(), opener)
        }

        async fn posted(&mut self) -> Posted {
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    if let Some(posted) = self.incoming.borrow().clone() {
                        return posted;
                    }
                    self.incoming.changed().await.expect("the watch stays open");
                }
            })
            .await
            .expect("the fixture received the post")
        }

        async fn emit_token(&self, id: u32, token: &str) {
            let iface = self.interface().await;
            Server::activation_token(iface.signal_emitter(), id, token)
                .await
                .expect("activation token");
        }

        async fn emit_action(&self, id: u32, key: &str) {
            let iface = self.interface().await;
            Server::action_invoked(iface.signal_emitter(), id, key)
                .await
                .expect("action");
        }

        async fn emit_closed(&self, id: u32) {
            let iface = self.interface().await;
            Server::notification_closed(iface.signal_emitter(), id, 1)
                .await
                .expect("closed");
        }

        async fn interface(&self) -> zbus::object_server::InterfaceRef<Server> {
            self.server
                .object_server()
                .interface(PATH)
                .await
                .expect("the fixture interface")
        }
    }

    #[derive(Clone, Default)]
    struct Recording {
        launches: Arc<Mutex<Vec<(PathBuf, String)>>>,
        arrived: Arc<Notify>,
    }

    impl Recording {
        async fn launched(&self) -> (PathBuf, String) {
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    if let Some(launch) = self.latest() {
                        return launch;
                    }
                    self.arrived.notified().await;
                }
            })
            .await
            .expect("the opener is called")
        }

        fn latest(&self) -> Option<(PathBuf, String)> {
            self.launches.lock().expect("lock").last().cloned()
        }
    }

    impl Opener for Recording {
        fn launch(
            &self,
            _connection: &Connection,
            path: &Path,
            token: &str,
        ) -> BoxFuture<'static, ()> {
            let launches = Arc::clone(&self.launches);
            let arrived = Arc::clone(&self.arrived);
            let path = path.to_owned();
            let token = token.to_owned();
            Box::pin(async move {
                launches.lock().expect("lock").push((path, token));
                arrived.notify_one();
            })
        }
    }

    fn saved(body: &str) -> Notice {
        Notice {
            summary: "Screenshot saved".to_owned(),
            body: body.to_owned(),
            image: Some(PathBuf::from(body)),
            actions: true,
        }
    }

    fn hint_str(hints: &HashMap<String, OwnedValue>, key: &str) -> String {
        hints
            .get(key)
            .and_then(|value| <&str>::try_from(value).ok())
            .unwrap_or("")
            .to_owned()
    }

    fn hint_bool(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
        hints.get(key).and_then(|value| bool::try_from(value).ok())
    }

    #[test]
    fn an_activatable_app_id_becomes_its_object_path() {
        assert_eq!(
            application_path("org.example.Image-Viewer"),
            "/org/example/Image_Viewer"
        );
    }

    #[test]
    fn the_bus_name_is_the_desktop_file_name_when_it_is_a_valid_one() {
        assert_eq!(
            bus_name_of(Path::new(
                "/usr/share/applications/kde/org.kde.Viewer.desktop"
            )),
            Some("org.kde.Viewer".to_owned())
        );
        assert_eq!(
            bus_name_of(Path::new("/usr/share/applications/viewer.desktop")),
            None
        );
        assert_eq!(
            bus_name_of(Path::new("/usr/share/applications/org.kde.Viewer")),
            None
        );
    }

    #[tokio::test]
    async fn a_saved_shot_is_posted_transient_with_actions_and_an_image() {
        let mut fixture = Fixture::start().await;
        let notifier = fixture.notifier(Arc::new(Recording::default()));
        let path = "/tmp/shot.png";
        notifier.notify(saved(path)).expect("started");
        let posted = fixture.posted().await;

        assert_eq!(posted.app_name, gettext("Screenshot"));
        assert_eq!(posted.app_icon, "");
        assert_eq!(posted.summary, gettext("Screenshot saved"));
        assert_eq!(posted.expire_timeout, EXPIRE_MS);
        assert_eq!(hint_bool(&posted.hints, "transient"), Some(true));
        assert_eq!(hint_str(&posted.hints, "desktop-entry"), DESKTOP_ENTRY);
        assert_eq!(
            hint_str(&posted.hints, "image-path"),
            file_uri(Path::new(path))
        );
        assert_eq!(
            posted.actions,
            vec![
                "default".to_owned(),
                pgettext("notification action", "Open"),
                "folder".to_owned(),
                gettext("Show in Folder"),
            ]
        );
    }

    #[tokio::test]
    async fn a_copied_shot_is_posted_without_actions_or_an_image() {
        let mut fixture = Fixture::start().await;
        let notifier = fixture.notifier(Arc::new(Recording::default()));
        notifier
            .notify(Notice {
                summary: "Screenshot copied".to_owned(),
                body: String::new(),
                image: None,
                actions: false,
            })
            .expect("started");
        let posted = fixture.posted().await;

        assert_eq!(posted.summary, gettext("Screenshot copied"));
        assert!(posted.actions.is_empty());
        assert!(!posted.hints.contains_key("image-path"));
        assert_eq!(posted.expire_timeout, EXPIRE_MS);
        assert_eq!(hint_bool(&posted.hints, "transient"), Some(true));
    }

    #[tokio::test]
    async fn a_file_name_with_markup_is_escaped_in_the_body() {
        let mut fixture = Fixture::start().await;
        let notifier = fixture.notifier(Arc::new(Recording::default()));
        notifier.notify(saved("/tmp/a<b>&c>.png")).expect("started");
        let posted = fixture.posted().await;

        assert_eq!(posted.body, "/tmp/a&lt;b&gt;&amp;c&gt;.png");
    }

    #[tokio::test]
    async fn an_open_action_launches_with_the_activation_token() {
        let mut fixture = Fixture::start().await;
        let recording = Recording::default();
        let notifier = fixture.notifier(Arc::new(recording.clone()));
        let path = PathBuf::from("/tmp/shot.png");
        notifier
            .notify(saved(path.to_str().expect("utf-8")))
            .expect("started");
        let posted = fixture.posted().await;

        fixture.emit_token(posted.id, "token-7").await;
        fixture.emit_action(posted.id, "default").await;

        let (opened, token) = recording.launched().await;
        assert_eq!(opened, path);
        assert_eq!(token, "token-7");
        tokio::time::timeout(Duration::from_secs(2), notifier.closed())
            .await
            .expect("handling the action ends the duty");
    }

    #[tokio::test]
    async fn an_activatable_handler_is_opened_with_the_activation_token() {
        let mut fixture = Fixture::start().await;
        let (entered_tx, mut entered) = watch::channel(false);
        let (calls_tx, mut calls) = watch::channel(None);
        let release = Arc::new(Notify::new());
        fixture
            .server
            .object_server()
            .at(
                application_path(VIEWER),
                Viewer {
                    entered: entered_tx,
                    release: Arc::clone(&release),
                    calls: calls_tx,
                },
            )
            .await
            .expect("the viewer is exported");
        glimpse_dbus::own_name(&fixture.server, VIEWER)
            .await
            .expect("the viewer owns its name");
        let opener = Arc::new(Desktop {
            resolve: Arc::new(|_| Some(Handler::Activatable(VIEWER.to_owned()))),
        });
        let notifier = fixture.notifier(opener);
        let path = "/tmp/shot.png";
        notifier.notify(saved(path)).expect("started");
        let posted = fixture.posted().await;

        fixture.emit_token(posted.id, "token-7").await;
        fixture.emit_action(posted.id, "default").await;
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if *entered.borrow() {
                    return;
                }
                entered.changed().await.expect("the watch stays open");
            }
        })
        .await
        .expect("Open was called");

        let duty = notifier.closed();
        tokio::pin!(duty);
        tokio::select! {
            _ = &mut duty => panic!("hold returned before Open answered"),
            () = tokio::time::sleep(Duration::from_millis(200)) => {}
        }
        release.notify_one();

        let opened = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(opened) = calls.borrow().clone() {
                    return opened;
                }
                calls.changed().await.expect("the watch stays open");
            }
        })
        .await
        .expect("Open answered");
        assert_eq!(
            opened,
            Opened {
                uris: vec![file_uri(Path::new(path))],
                activation_token: "token-7".to_owned(),
                startup_id: "token-7".to_owned(),
            }
        );
        tokio::time::timeout(Duration::from_secs(2), duty)
            .await
            .expect("the duty ends after Open answers");
    }

    #[tokio::test]
    async fn show_in_folder_falls_back_to_opening_the_containing_folder() {
        let mut fixture = Fixture::start().await;
        let recording = Recording::default();
        let notifier = fixture.notifier(Arc::new(recording.clone()));
        notifier.notify(saved("/tmp/shots/a.png")).expect("started");
        let posted = fixture.posted().await;

        fixture.emit_token(posted.id, "token-folder").await;
        fixture.emit_action(posted.id, "folder").await;

        let (opened, token) = recording.launched().await;
        assert_eq!(opened, PathBuf::from("/tmp/shots"));
        assert_eq!(token, "token-folder");
    }

    #[tokio::test]
    async fn a_close_for_this_notification_ends_a_hold_with_no_clipboard_duty() {
        let mut fixture = Fixture::start().await;
        let notifier = fixture.notifier(Arc::new(Recording::default()));
        notifier.notify(saved("/tmp/shot.png")).expect("started");
        let posted = fixture.posted().await;
        let duty = crate::hold::until_done(None::<std::future::Ready<()>>, Some(notifier.closed()));
        tokio::pin!(duty);

        fixture.emit_closed(posted.id).await;

        tokio::time::timeout(Duration::from_secs(2), duty)
            .await
            .expect("the notify duty ends and so does hold");
    }

    #[tokio::test]
    async fn a_close_for_another_notification_does_not_end_the_duty() {
        let mut fixture = Fixture::start().await;
        let notifier = fixture.notifier(Arc::new(Recording::default()));
        notifier.notify(saved("/tmp/shot.png")).expect("started");
        let posted = fixture.posted().await;
        let duty = notifier.closed();
        tokio::pin!(duty);

        fixture.emit_closed(posted.id.wrapping_add(1)).await;
        tokio::select! {
            _ = &mut duty => panic!("a different id ended the duty"),
            () = tokio::time::sleep(Duration::from_millis(400)) => {}
        }

        fixture.emit_closed(posted.id).await;
        tokio::time::timeout(Duration::from_secs(2), duty)
            .await
            .expect("our id ends the duty");
    }

    #[tokio::test]
    async fn a_missing_notification_server_ends_the_duty() {
        let bus = PrivateBus::isolated();
        let client = bus.connection().await;
        let notifier = BusNotifier::on(client, Arc::new(Recording::default()));
        notifier.notify(saved("/tmp/shot.png")).expect("started");
        tokio::time::timeout(Duration::from_secs(2), notifier.closed())
            .await
            .expect("no server still ends the duty");
    }

    #[tokio::test(start_paused = true)]
    async fn the_notify_duty_ends_at_the_grace_when_nothing_arrives() {
        tokio::time::resume();
        let fixture = Fixture::answering_after(Duration::from_millis(50)).await;
        let notifier = fixture.notifier(Arc::new(Recording::default()));
        notifier.hold_grace();
        let mut returned = notifier.notify_returned();
        let duty = crate::hold::notified(&notifier, saved("/tmp/shot.png"));
        tokio::pin!(duty);
        while !*returned.borrow() {
            tokio::select! {
                biased;
                _ = &mut duty => panic!("the duty ended before Notify returned"),
                changed = returned.changed() => {
                    changed.expect("the watch stays open");
                }
            }
        }
        tokio::time::pause();
        notifier.release_grace();
        tokio::task::yield_now().await;

        let started = tokio::time::Instant::now();
        tokio::time::advance(crate::hold::NOTIFY_GRACE - Duration::from_millis(1)).await;
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
        tokio::select! {
            biased;
            _ = &mut duty => panic!("the duty ended before the grace"),
            _ = std::future::ready(()) => {}
        }
        tokio::time::advance(Duration::from_millis(1)).await;
        (&mut duty).await;
        let elapsed = started.elapsed();
        assert!(
            elapsed >= crate::hold::NOTIFY_GRACE
                && elapsed - crate::hold::NOTIFY_GRACE < Duration::from_millis(2),
            "the duty ended {elapsed:?} after Notify returned"
        );
    }
}
