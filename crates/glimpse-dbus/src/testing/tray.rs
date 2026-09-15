use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zbus::Connection;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

use crate::dbusmenu::MenuLayout;
use crate::status_notifier_item::{IconPixmap, ToolTip};
use crate::status_notifier_watcher::{Registry, WATCHER_NAME, WATCHER_PATH};

pub const AYATANA_PATH: &str = "/org/ayatana/NotificationItem/fake";
pub const ITEM_PATH: &str = "/StatusNotifierItem";
pub const MENU_PATH: &str = "/com/canonical/dbusmenu";

/// Which real-world implementation a fake imitates. The two differ in the *member set* they
/// export, which is the whole point: a host that assumes the full interface breaks on both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// `libayatana-appindicator`: `XAyatana*`, an absolute file path in `IconName`, and no
    /// `IconPixmap` at all.
    Ayatana,
    /// The Electron/Chromium shape: `IconPixmap` only, no `IconName`, tooltip prose in the title
    /// field. Its real counterpart also answers `Introspect` with an empty document, which an
    /// object server will not do — every other difference is reproduced.
    Pixmap,
}

impl Shape {
    pub fn path(self) -> &'static str {
        match self {
            Shape::Ayatana => AYATANA_PATH,
            Shape::Pixmap => ITEM_PATH,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Activate(i32, i32),
    SecondaryActivate(i32, i32),
    ContextMenu(i32, i32),
    Scroll(i32, String),
    AboutToShow(i32),
    Event(i32, String),
}

#[derive(Debug, Clone)]
pub struct ItemState {
    pub id: String,
    pub title: String,
    pub status: String,
    pub category: String,
    pub icon_name: String,
    pub icon_theme_path: String,
    pub attention_icon_name: String,
    pub pixmap: IconPixmap,
    pub label: String,
    pub tooltip: ToolTip,
    pub menu: bool,
}

impl ItemState {
    fn new(id: &str, shape: Shape) -> Self {
        Self {
            id: id.to_owned(),
            title: id.to_owned(),
            status: "Active".to_owned(),
            category: "ApplicationStatus".to_owned(),
            // A themed name that every icon theme really has, so a bar driven by this fixture
            // shows an icon rather than a broken-image glyph. The absolute-path and
            // missing-theme-directory branches of the ladder are covered by unit tests, which do
            // not need a file on disk to exercise them.
            icon_name: match shape {
                Shape::Ayatana => "folder-publicshare-symbolic".to_owned(),
                Shape::Pixmap => String::new(),
            },
            icon_theme_path: String::new(),
            attention_icon_name: String::new(),
            pixmap: match shape {
                Shape::Ayatana => Vec::new(),
                Shape::Pixmap => vec![(16, 16, ring(16)), (32, 32, ring(32))],
            },
            label: String::new(),
            tooltip: match shape {
                Shape::Ayatana => (String::new(), Vec::new(), String::new(), String::new()),
                Shape::Pixmap => (
                    String::new(),
                    Vec::new(),
                    "You have 1 notification".to_owned(),
                    String::new(),
                ),
            },
            menu: true,
        }
    }
}

/// A filled ring in ARGB32, network byte order — recognisable on a bar at a glance, and two sizes
/// so a scaled output really chooses between them.
fn ring(size: i32) -> Vec<u8> {
    let centre = (size as f32 - 1.0) / 2.0;
    let mut argb = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - centre;
            let dy = y as f32 - centre;
            let distance = (dx * dx + dy * dy).sqrt() / centre.max(1.0);
            let inside = (0.45..=0.95).contains(&distance);
            argb.extend_from_slice(&match inside {
                true => [0xff, 0x78, 0xae, 0xed],
                false => [0x00, 0x00, 0x00, 0x00],
            });
        }
    }
    argb
}

type Shared = Arc<Mutex<ItemState>>;
type Calls = Arc<Mutex<Vec<Call>>>;

fn menu_path(state: &Shared) -> ObjectPath<'static> {
    let offered = state.lock().expect("lock poisoned").menu;
    let path = if offered { MENU_PATH } else { "/" };
    ObjectPath::try_from(path)
        .expect("a literal path")
        .to_owned()
}

#[derive(Clone)]
pub struct AyatanaItem {
    state: Shared,
    calls: Calls,
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl AyatanaItem {
    async fn activate(&self, x: i32, y: i32) {
        self.calls
            .lock()
            .expect("lock poisoned")
            .push(Call::Activate(x, y));
    }

    async fn secondary_activate(&self, x: i32, y: i32) {
        self.calls
            .lock()
            .expect("lock poisoned")
            .push(Call::SecondaryActivate(x, y));
    }

    #[zbus(name = "XAyatanaSecondaryActivate")]
    async fn x_ayatana_secondary_activate(&self, timestamp: u32) {
        self.calls
            .lock()
            .expect("lock poisoned")
            .push(Call::SecondaryActivate(timestamp as i32, 0));
    }

    async fn scroll(&self, delta: i32, orientation: String) {
        self.calls
            .lock()
            .expect("lock poisoned")
            .push(Call::Scroll(delta, orientation));
    }

    #[zbus(property)]
    async fn id(&self) -> String {
        self.state.lock().expect("lock poisoned").id.clone()
    }

    #[zbus(property)]
    async fn category(&self) -> String {
        self.state.lock().expect("lock poisoned").category.clone()
    }

    #[zbus(property)]
    async fn status(&self) -> String {
        self.state.lock().expect("lock poisoned").status.clone()
    }

    #[zbus(property)]
    async fn title(&self) -> String {
        self.state.lock().expect("lock poisoned").title.clone()
    }

    #[zbus(property)]
    async fn icon_name(&self) -> String {
        self.state.lock().expect("lock poisoned").icon_name.clone()
    }

    #[zbus(property)]
    async fn icon_theme_path(&self) -> String {
        self.state
            .lock()
            .expect("lock poisoned")
            .icon_theme_path
            .clone()
    }

    #[zbus(property)]
    async fn icon_accessible_desc(&self) -> String {
        self.state.lock().expect("lock poisoned").title.clone()
    }

    #[zbus(property)]
    async fn attention_icon_name(&self) -> String {
        self.state
            .lock()
            .expect("lock poisoned")
            .attention_icon_name
            .clone()
    }

    #[zbus(property)]
    async fn attention_accessible_desc(&self) -> String {
        String::new()
    }

    #[zbus(property, name = "ToolTip")]
    async fn tool_tip(&self) -> ToolTip {
        self.state.lock().expect("lock poisoned").tooltip.clone()
    }

    #[zbus(property)]
    async fn menu(&self) -> ObjectPath<'static> {
        menu_path(&self.state)
    }

    #[zbus(property, name = "XAyatanaLabel")]
    async fn x_ayatana_label(&self) -> String {
        self.state.lock().expect("lock poisoned").label.clone()
    }

    #[zbus(property, name = "XAyatanaLabelGuide")]
    async fn x_ayatana_label_guide(&self) -> String {
        String::new()
    }

    #[zbus(property, name = "XAyatanaOrderingIndex")]
    async fn x_ayatana_ordering_index(&self) -> u32 {
        0
    }

    #[zbus(signal)]
    async fn new_title(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_tool_tip(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_attention_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_icon_theme_path(emitter: &SignalEmitter<'_>, path: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_status(emitter: &SignalEmitter<'_>, status: &str) -> zbus::Result<()>;

    #[zbus(signal, name = "XAyatanaNewLabel")]
    async fn x_ayatana_new_label(
        emitter: &SignalEmitter<'_>,
        label: &str,
        guide: &str,
    ) -> zbus::Result<()>;
}

#[derive(Clone)]
pub struct PixmapItem {
    state: Shared,
    calls: Calls,
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl PixmapItem {
    async fn activate(&self, x: i32, y: i32) {
        self.calls
            .lock()
            .expect("lock poisoned")
            .push(Call::Activate(x, y));
    }

    async fn secondary_activate(&self, x: i32, y: i32) {
        self.calls
            .lock()
            .expect("lock poisoned")
            .push(Call::SecondaryActivate(x, y));
    }

    async fn context_menu(&self, x: i32, y: i32) {
        self.calls
            .lock()
            .expect("lock poisoned")
            .push(Call::ContextMenu(x, y));
    }

    async fn scroll(&self, delta: i32, orientation: String) {
        self.calls
            .lock()
            .expect("lock poisoned")
            .push(Call::Scroll(delta, orientation));
    }

    #[zbus(property)]
    async fn id(&self) -> String {
        self.state.lock().expect("lock poisoned").id.clone()
    }

    #[zbus(property)]
    async fn title(&self) -> String {
        self.state.lock().expect("lock poisoned").title.clone()
    }

    #[zbus(property)]
    async fn status(&self) -> String {
        self.state.lock().expect("lock poisoned").status.clone()
    }

    #[zbus(property)]
    async fn category(&self) -> String {
        self.state.lock().expect("lock poisoned").category.clone()
    }

    #[zbus(property)]
    async fn icon_pixmap(&self) -> IconPixmap {
        self.state.lock().expect("lock poisoned").pixmap.clone()
    }

    #[zbus(property)]
    async fn attention_icon_name(&self) -> String {
        self.state
            .lock()
            .expect("lock poisoned")
            .attention_icon_name
            .clone()
    }

    #[zbus(property)]
    async fn attention_icon_pixmap(&self) -> IconPixmap {
        Vec::new()
    }

    #[zbus(property)]
    async fn attention_movie_name(&self) -> String {
        String::new()
    }

    #[zbus(property)]
    async fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property, name = "ToolTip")]
    async fn tool_tip(&self) -> ToolTip {
        self.state.lock().expect("lock poisoned").tooltip.clone()
    }

    #[zbus(property)]
    async fn menu(&self) -> ObjectPath<'static> {
        menu_path(&self.state)
    }

    #[zbus(signal)]
    async fn new_title(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_tool_tip(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_status(emitter: &SignalEmitter<'_>, status: &str) -> zbus::Result<()>;
}

/// One node of the layout `GetLayout` returns. `props` are the untyped `a{sv}` a real menu sends,
/// with the defaults left off so a decoder has to supply them.
struct Node {
    id: i32,
    props: Vec<(&'static str, Value<'static>)>,
    children: Vec<Node>,
}

fn node(id: i32, props: Vec<(&'static str, Value<'static>)>, children: Vec<Node>) -> Node {
    Node {
        id,
        props,
        children,
    }
}

fn properties(node: &Node) -> HashMap<String, OwnedValue> {
    node.props
        .iter()
        .map(|(key, value)| {
            (
                (*key).to_owned(),
                OwnedValue::try_from(value.clone()).expect("a plain value"),
            )
        })
        .collect()
}

/// Children travel as `av`, each a variant wrapping the same `(ia{sv}av)` structure, which is what
/// forces a decoder to recurse through untyped values rather than a typed tree.
fn children(node: &Node) -> Vec<OwnedValue> {
    node.children
        .iter()
        .map(|child| {
            OwnedValue::try_from(Value::from((child.id, properties(child), children(child))))
                .expect("a layout structure")
        })
        .collect()
}

/// A layout deep enough to prove a recursive decoder: three levels of submenu, a separator, a
/// checkmark, a disabled item, a hidden one, a disposition, and both mnemonic forms.
fn layout() -> Node {
    node(
        0,
        vec![("children-display", Value::from("submenu"))],
        vec![
            node(
                1,
                vec![
                    ("label", Value::from("_Open Nextcloud")),
                    ("icon-name", Value::from("web-browser-symbolic")),
                ],
                Vec::new(),
            ),
            node(2, vec![("type", Value::from("separator"))], Vec::new()),
            node(
                3,
                vec![
                    ("label", Value::from("Recent")),
                    ("children-display", Value::from("submenu")),
                ],
                vec![
                    node(
                        4,
                        vec![("label", Value::from("2026-09 __invoice.odt"))],
                        Vec::new(),
                    ),
                    node(
                        5,
                        vec![
                            ("label", Value::from("Archive")),
                            ("children-display", Value::from("submenu")),
                        ],
                        vec![node(6, vec![("label", Value::from("2025"))], Vec::new())],
                    ),
                ],
            ),
            node(
                7,
                vec![
                    ("label", Value::from("Pause syncing")),
                    ("toggle-type", Value::from("checkmark")),
                    ("toggle-state", Value::from(1i32)),
                ],
                Vec::new(),
            ),
            node(
                8,
                vec![
                    ("label", Value::from("Resolve conflicts")),
                    ("enabled", Value::from(false)),
                ],
                Vec::new(),
            ),
            node(
                9,
                vec![
                    ("label", Value::from("Never shown")),
                    ("visible", Value::from(false)),
                ],
                Vec::new(),
            ),
            node(
                10,
                vec![
                    ("label", Value::from("Quit")),
                    ("disposition", Value::from("alert")),
                ],
                Vec::new(),
            ),
        ],
    )
}

#[derive(Clone)]
pub struct FakeMenu {
    calls: Calls,
    revision: Arc<Mutex<u32>>,
    status: Arc<Mutex<String>>,
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl FakeMenu {
    async fn get_layout(
        &self,
        _parent_id: i32,
        _recursion_depth: i32,
        _property_names: Vec<String>,
    ) -> (u32, MenuLayout) {
        let revision = *self.revision.lock().expect("lock poisoned");
        let root = layout();
        (revision, (root.id, properties(&root), children(&root)))
    }

    async fn about_to_show(&self, id: i32) -> bool {
        self.calls
            .lock()
            .expect("lock poisoned")
            .push(Call::AboutToShow(id));
        false
    }

    async fn event(&self, id: i32, event_id: String, _data: Value<'_>, _timestamp: u32) {
        self.calls
            .lock()
            .expect("lock poisoned")
            .push(Call::Event(id, event_id));
    }

    #[zbus(property)]
    async fn status(&self) -> String {
        self.status.lock().expect("lock poisoned").clone()
    }

    #[zbus(property)]
    async fn text_direction(&self) -> String {
        "ltr".to_owned()
    }

    #[zbus(property)]
    async fn version(&self) -> u32 {
        4
    }

    #[zbus(property)]
    async fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(signal)]
    async fn layout_updated(
        emitter: &SignalEmitter<'_>,
        revision: u32,
        parent_id: i32,
    ) -> zbus::Result<()>;
}

/// A tray item on a bus of the test's own, with the knobs a host has to react to. Dropping it
/// leaves the objects up: the connection owns them, so drop that to take the item away.
pub struct FakeItem {
    connection: Connection,
    shape: Shape,
    state: Shared,
    calls: Calls,
    revision: Arc<Mutex<u32>>,
    status: Arc<Mutex<String>>,
}

impl FakeItem {
    pub async fn start(connection: Connection, shape: Shape, id: &str) -> zbus::Result<Self> {
        let state: Shared = Arc::new(Mutex::new(ItemState::new(id, shape)));
        let calls: Calls = Arc::new(Mutex::new(Vec::new()));
        let revision = Arc::new(Mutex::new(1));
        let status = Arc::new(Mutex::new("normal".to_owned()));

        let server = connection.object_server();
        match shape {
            Shape::Ayatana => {
                server
                    .at(
                        AYATANA_PATH,
                        AyatanaItem {
                            state: state.clone(),
                            calls: calls.clone(),
                        },
                    )
                    .await?;
            }
            Shape::Pixmap => {
                server
                    .at(
                        ITEM_PATH,
                        PixmapItem {
                            state: state.clone(),
                            calls: calls.clone(),
                        },
                    )
                    .await?;
            }
        }
        server
            .at(
                MENU_PATH,
                FakeMenu {
                    calls: calls.clone(),
                    revision: revision.clone(),
                    status: status.clone(),
                },
            )
            .await?;

        Ok(Self {
            connection,
            shape,
            state,
            calls,
            revision,
            status,
        })
    }

    pub fn path(&self) -> &'static str {
        self.shape.path()
    }

    pub fn unique_name(&self) -> String {
        self.connection
            .unique_name()
            .map(ToString::to_string)
            .unwrap_or_default()
    }

    /// The key a watcher stores: the owner's unique name concatenated with the object path.
    pub fn key(&self) -> String {
        format!("{}{}", self.unique_name(), self.path())
    }

    /// Register with whatever watcher holds the name, over the item's **own** connection: the
    /// sender the watcher records has to be the one the objects live on, not whoever made the call.
    pub async fn register(&self) -> zbus::Result<()> {
        let watcher =
            crate::status_notifier_watcher::StatusNotifierWatcherProxy::new(&self.connection)
                .await?;
        watcher.register_status_notifier_item(self.path()).await
    }

    /// Take a well-known `org.kde.StatusNotifierItem-*` name, the way a hand-rolled client such as
    /// Steam does. A sweep finds these; an item holding only a unique name cannot be recovered.
    pub async fn claim_well_known_name(&self) -> zbus::Result<String> {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
        let ordinal = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = format!(
            "org.kde.StatusNotifierItem-{}-{ordinal}",
            std::process::id()
        );
        crate::own_name(&self.connection, &name).await?;
        Ok(name)
    }

    /// Re-register whenever a host announces itself, which is what every Qt and
    /// libayatana-appindicator client does and what makes announce-before-sweep a race worth
    /// deduping.
    pub async fn re_register_on_host_announcement(&self) -> zbus::Result<()> {
        let watcher =
            crate::status_notifier_watcher::StatusNotifierWatcherProxy::new(&self.connection)
                .await?;
        let mut announcements = watcher.receive_status_notifier_host_registered().await?;
        let path = self.path();
        tokio::spawn(async move {
            while futures_util::StreamExt::next(&mut announcements)
                .await
                .is_some()
            {
                let _ = watcher.register_status_notifier_item(path).await;
            }
        });
        Ok(())
    }

    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().expect("lock poisoned").clone()
    }

    pub async fn set_status(&self, status: &str) -> zbus::Result<()> {
        self.state.lock().expect("lock poisoned").status = status.to_owned();
        AyatanaItem::new_status(&self.emitter()?, status).await
    }

    pub async fn set_icon_name(&self, name: &str) -> zbus::Result<()> {
        self.state.lock().expect("lock poisoned").icon_name = name.to_owned();
        AyatanaItem::new_icon(&self.emitter()?).await
    }

    pub async fn set_title(&self, title: &str) -> zbus::Result<()> {
        self.state.lock().expect("lock poisoned").title = title.to_owned();
        AyatanaItem::new_title(&self.emitter()?).await
    }

    /// `XAyatanaLabel` exists only on the Ayatana shape; on the other this changes nothing and
    /// emits nothing, which is what a host has to cope with.
    pub async fn set_label(&self, label: &str) -> zbus::Result<()> {
        if self.shape != Shape::Ayatana {
            return Ok(());
        }
        self.state.lock().expect("lock poisoned").label = label.to_owned();
        AyatanaItem::x_ayatana_new_label(&self.emitter()?, label, "").await
    }

    /// Take the menu away, the way an application that closes its menu does.
    pub async fn drop_menu(&self) -> zbus::Result<()> {
        self.state.lock().expect("lock poisoned").menu = false;
        self.connection
            .object_server()
            .remove::<FakeMenu, _>(MENU_PATH)
            .await?;
        Ok(())
    }

    pub async fn set_menu_status(&self, status: &str) -> zbus::Result<()> {
        *self.status.lock().expect("lock poisoned") = status.to_owned();
        let reference = self
            .connection
            .object_server()
            .interface::<_, FakeMenu>(MENU_PATH)
            .await?;
        reference
            .get()
            .await
            .status_changed(reference.signal_emitter())
            .await
    }

    pub async fn bump_menu_revision(&self) -> zbus::Result<()> {
        let revision = {
            let mut guard = self.revision.lock().expect("lock poisoned");
            *guard += 1;
            *guard
        };
        FakeMenu::layout_updated(
            &SignalEmitter::new(&self.connection, MENU_PATH)?,
            revision,
            0,
        )
        .await
    }

    /// Both shapes export the same interface name, so one emitter serves either: what selects the
    /// object is the path, not the Rust type the signal was declared on.
    fn emitter(&self) -> zbus::Result<SignalEmitter<'static>> {
        SignalEmitter::new(&self.connection, self.path())
    }
}

/// An incumbent watcher, for a test that needs glimpse to find the name already taken. The two
/// interface names are two Rust types because an object server keys by interface name, and both
/// have to answer at one path or a client that only knows the freedesktop spelling sees nothing.
#[derive(Clone)]
pub struct FakeWatcher {
    registry: Arc<Mutex<Registry>>,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl FakeWatcher {
    async fn register_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let registered = self
            .registry
            .lock()
            .expect("lock poisoned")
            .register(&sender, service);
        if let Some(key) = registered {
            Self::status_notifier_item_registered(&emitter, &key).await?;
        }
        Ok(())
    }

    async fn register_status_notifier_host(
        &self,
        _service: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        if self
            .registry
            .lock()
            .expect("lock poisoned")
            .register_host(&sender)
        {
            Self::status_notifier_host_registered(&emitter).await?;
        }
        Ok(())
    }

    async fn unregister_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let removed = self
            .registry
            .lock()
            .expect("lock poisoned")
            .unregister(&sender, service);
        if let Some(key) = removed {
            Self::status_notifier_item_unregistered(&emitter, &key).await?;
        }
        Ok(())
    }

    #[zbus(property)]
    async fn registered_status_notifier_items(&self) -> Vec<String> {
        self.registry
            .lock()
            .expect("lock poisoned")
            .items()
            .to_vec()
    }

    #[zbus(property)]
    async fn is_status_notifier_host_registered(&self) -> bool {
        self.registry
            .lock()
            .expect("lock poisoned")
            .host_registered()
    }

    #[zbus(property)]
    async fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(signal)]
    async fn status_notifier_item_registered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_item_unregistered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_registered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

#[derive(Clone)]
pub struct FakeWatcherAlias {
    registry: Arc<Mutex<Registry>>,
}

#[zbus::interface(name = "org.freedesktop.StatusNotifierWatcher")]
impl FakeWatcherAlias {
    async fn register_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        self.registry
            .lock()
            .expect("lock poisoned")
            .register(&sender, service);
        Ok(())
    }

    #[zbus(property)]
    async fn registered_status_notifier_items(&self) -> Vec<String> {
        self.registry
            .lock()
            .expect("lock poisoned")
            .items()
            .to_vec()
    }

    #[zbus(property)]
    async fn is_status_notifier_host_registered(&self) -> bool {
        self.registry
            .lock()
            .expect("lock poisoned")
            .host_registered()
    }

    #[zbus(property)]
    async fn protocol_version(&self) -> i32 {
        0
    }
}

/// Holds the watcher name on a test's bus until it is dropped.
pub struct IncumbentWatcher {
    connection: Connection,
}

impl IncumbentWatcher {
    pub async fn start(connection: Connection) -> zbus::Result<Self> {
        let registry = Arc::new(Mutex::new(Registry::default()));
        let server = connection.object_server();
        server
            .at(
                WATCHER_PATH,
                FakeWatcher {
                    registry: registry.clone(),
                },
            )
            .await?;
        server
            .at(WATCHER_PATH, FakeWatcherAlias { registry })
            .await?;
        crate::own_name(&connection, WATCHER_NAME).await?;
        Ok(Self { connection })
    }

    /// Release the name and take the objects down, the way a panel shutting down does.
    pub async fn stop(self) -> zbus::Result<()> {
        self.connection.release_name(WATCHER_NAME).await?;
        self.connection
            .object_server()
            .remove::<FakeWatcher, _>(WATCHER_PATH)
            .await?;
        Ok(())
    }
}
