use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gettextrs::{gettext, ngettext};
use glimpse_config::{Applet as AppletConfig, AppletKind, ClipboardAppletConfig, ColorFormat};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{
    ClipboardEntry, ClipboardEntryId, ClipboardHandle, ClipboardKind, ClipboardState, CommandError,
    KdeconnectDeviceId, KdeconnectHandle,
};
use glimpse_widgets::{
    Clip, ClipAction, ClipActions, ClipboardPopover, Fact, IndicatorSpec, Thumbnail, rgba,
};
use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};

use super::render;

/// The side a tile's thumbnail is decoded to. Bounding the decode rather than the bytes is what
/// stops a small image that expands to an enormous bitmap.
const THUMBNAIL: i32 = 320;
const EXCERPT_LINES: usize = 6;
const EXCERPT_CHARS: usize = 200;
use crate::applet::footer::Footer;
use crate::applet::popover::{PopoverHandle, Seat};
use crate::applet::{Applet, Ctx, Input, Opener, Report, report_failure, spawn_reported, wording};

pub struct Clipboard {
    service: ClipboardHandle,
    kdeconnect: KdeconnectHandle,
    notifications: NotificationsProviderHandle,
    state: ClipboardState,
    settings: ClipboardAppletConfig,
    tooltip_format: Option<String>,
    footer: Footer,
    shown: glib::WeakRef<ClipboardPopover>,
    spec: Vec<IndicatorSpec>,
    /// Decoded thumbnails, keyed by entry id. Decoding is the expensive half and an entry's bytes
    /// never change, so a texture is built once and pruned when its entry leaves.
    images: HashMap<ClipboardEntryId, Option<Thumbnail>>,
    files: HashMap<ClipboardEntryId, Option<gio::File>>,
    icon: gtk4::gio::Icon,
    query: Rc<RefCell<String>>,
    expanded: Rc<Cell<bool>>,
}

impl Clipboard {
    pub fn start(
        service: ClipboardHandle,
        kdeconnect: KdeconnectHandle,
        notifications: NotificationsProviderHandle,
    ) -> Self {
        Self {
            kdeconnect,
            state: service.snapshot(),
            service,
            notifications,
            settings: ClipboardAppletConfig::default(),
            tooltip_format: None,
            footer: Footer::default(),
            shown: glib::WeakRef::new(),
            spec: Vec::new(),
            images: HashMap::new(),
            files: HashMap::new(),
            icon: gtk4::gio::ThemedIcon::new(render::ICON).upcast(),
            query: Rc::new(RefCell::new(String::new())),
            expanded: Rc::new(Cell::new(false)),
        }
    }

    fn visible(&self) -> usize {
        self.settings.visible.clamp(1, 50)
    }

    /// Capped at what a `Row` can actually hold: its title setter re-truncates at 128 characters,
    /// so a larger configured value would be silently ignored.
    fn preview_chars(&self) -> usize {
        self.settings.preview_chars.clamp(8, 128)
    }

    fn refresh(&mut self, opener: &Opener) {
        let count = self.state.entries.len();
        let unavailable = self.state.unavailable.is_some();
        self.spec = match render::shown(count, self.settings.show_when_empty, unavailable) {
            false => Vec::new(),
            true => vec![IndicatorSpec {
                icon: Some(self.icon.clone()),
                label: render::filled(self.settings.label_format.as_deref(), count),
                tooltip: render::filled(self.tooltip_format.as_deref(), count),
                severity: self
                    .state
                    .unavailable
                    .as_ref()
                    .map(|_| glimpse_widgets::Severity::Warning),
                ..Default::default()
            }],
        };

        let Some(shown) = self.shown.upgrade() else {
            return;
        };
        match self.spec.is_empty() {
            true => opener.close_popover(),
            false => self.dress(&shown),
        }
    }

    fn clips(&mut self, pinned: bool) -> (Vec<Clip>, usize) {
        let query = self.query.borrow().clone();
        let searching = !query.trim().is_empty();
        let entries: Vec<ClipboardEntry> = self
            .state
            .entries
            .iter()
            .filter(|entry| entry.pinned == pinned && render::matches(entry, &query))
            .cloned()
            .collect();
        let cap = match pinned || searching || self.expanded.get() {
            // `visible` documents itself as counting recent entries only, and `History::evict`
            // goes to real trouble to make a pin outlive eviction; capping here would undo it.
            true => usize::MAX,
            false => self.visible(),
        };
        let hidden = entries.len().saturating_sub(cap);
        let clips = entries
            .iter()
            .take(cap)
            .map(|entry| self.clip(entry))
            .collect();
        (clips, hidden)
    }

    fn clip(&mut self, entry: &ClipboardEntry) -> Clip {
        let size = render::size(entry.data.len());
        if entry.kind == ClipboardKind::Image {
            let decoded = self.texture(entry.id, &entry.data);
            let title = gettext("Image · {size}").replace("{size}", &size);
            let facts = vec![Fact::new(
                gettext("Size"),
                match &decoded {
                    Some((_, (width, height))) => format!("{size} · {width} × {height}"),
                    None => size.clone(),
                },
            )];
            let mut clip = Clip {
                id: entry.id,
                title,
                icon: render::IMAGE_ICON.to_owned(),
                file: decoded.as_ref().and_then(|_| self.spill(entry)),
                image: decoded.map(|(texture, _)| texture),
                actions: vec![
                    action(SAVE, gettext("Save as…"), String::new()),
                    action(OPEN_WITH, gettext("Open with…"), String::new()),
                ],
                facts,
                pinned: entry.pinned,
                ..Default::default()
            };
            if clip.file.is_some() {
                clip.actions.extend(self.send());
            }
            return clip;
        }

        let text = String::from_utf8_lossy(&entry.data);
        let title = render::title(entry, self.preview_chars());
        let excerpt = render::excerpt(&text, EXCERPT_LINES, EXCERPT_CHARS);
        let mut clip = Clip {
            id: entry.id,
            icon: render::TEXT_ICON.to_owned(),
            excerpt: match excerpt == title {
                true => String::new(),
                false => excerpt,
            },
            facts: vec![Fact::new(gettext("Size"), size)],
            pinned: entry.pinned,
            title,
            ..Default::default()
        };
        match render::shape(&text) {
            render::Shape::Link { host, rest, .. } => {
                clip.title = match rest.is_empty() {
                    true => render::title_of(&host, self.preview_chars()),
                    false => render::title_of(&rest, self.preview_chars()),
                };
                clip.facts.insert(
                    0,
                    Fact::new(
                        gettext("Site"),
                        render::title_of(&host, self.preview_chars()),
                    ),
                );
                clip.icon = render::LINK_ICON.to_owned();
                clip.actions = vec![action(OPEN, gettext("Open in browser"), String::new())];
            }
            render::Shape::Color(channels) => {
                clip.facts
                    .insert(0, Fact::new(gettext("Type"), gettext("Color")));
                clip.swatch = Some(rgba(channels));
                clip.excerpt = String::new();
                clip.actions = [ColorFormat::Rgb, ColorFormat::Hsl]
                    .into_iter()
                    .map(|format| {
                        action(
                            format.name(),
                            gettext("Copy as {format}").replace("{format}", format.label()),
                            format.render(channels),
                        )
                    })
                    .collect();
            }
            render::Shape::Path(_) => {
                clip.facts
                    .insert(0, Fact::new(gettext("Type"), gettext("Path")));
                clip.icon = render::PATH_ICON.to_owned();
                clip.actions = vec![action(OPEN, gettext("Open"), String::new())];
            }
            render::Shape::Lines(count) => {
                clip.facts
                    .insert(0, Fact::new(gettext("Lines"), count.to_string()));
            }
            render::Shape::Plain => {}
        }
        clip.actions.extend(self.send());
        clip
    }

    /// An image another application chose, decoded once. A picture that will not decode is not an
    /// error: the entry is still restorable, only its preview failed.
    fn texture(&mut self, id: ClipboardEntryId, data: &[u8]) -> Option<(gdk::Texture, (i32, i32))> {
        if let Some(held) = self.images.get(&id) {
            return held.clone();
        }
        let built = glimpse_widgets::thumbnail(data, THUMBNAIL);
        if built.is_none() {
            tracing::debug!(id, "a clipboard image would not decode");
        }
        self.images.insert(id, built.clone());
        built
    }

    fn spill(&mut self, entry: &ClipboardEntry) -> Option<gio::File> {
        if let Some(held) = self.files.get(&entry.id) {
            return held.clone();
        }
        let written = spill_path(entry).and_then(|path| {
            write_private(&path, &entry.data)
                .inspect_err(|error| tracing::warn!("cannot write {}: {error}", path.display()))
                .ok()
                .map(|()| gio::File::for_path(path))
        });
        self.files.insert(entry.id, written.clone());
        written
    }

    fn send(&self) -> Option<ClipAction> {
        let choices: Vec<ClipAction> = self
            .kdeconnect
            .snapshot()
            .devices
            .into_iter()
            .filter(|device| device.actions.share)
            .map(|device| {
                action(
                    &format!("{SEND}{}", device.id.as_str()),
                    render::title_of(&device.name, DEVICE_CHARS),
                    String::new(),
                )
            })
            .collect();
        (!choices.is_empty()).then(|| ClipAction {
            choices,
            ..action(SEND, gettext("Send to device"), String::new())
        })
    }

    fn dress(&mut self, shown: &ClipboardPopover) {
        let (pinned, _) = self.clips(true);
        let (recent, hidden) = self.clips(false);
        let total = self.state.entries.len();

        shown.set_actions(ClipActions {
            pin: gettext("Pin"),
            unpin: gettext("Unpin"),
            forget: gettext("Forget"),
        });
        shown.set_trouble(self.state.unavailable.as_deref());
        shown.set_searchable(total > 0);
        shown.set_pinned(&pinned);
        shown.set_overflow(
            match (hidden, self.expanded.get()) {
                (0, false) => None,
                (_, true) => Some(gettext("Show fewer")),
                (hidden, false) => Some(
                    ngettext("{count} more", "{count} more", hidden as u32)
                        .replace("{count}", &hidden.to_string()),
                ),
            }
            .as_deref(),
        );
        shown.set_recent(&recent);
        // `History::clear` keeps pins, so with nothing else held the row would succeed and change
        // nothing — a control that reports success and does not act.
        let clearable = self.state.entries.iter().filter(|e| !e.pinned).count();
        let clear = gettext("Clear history");
        shown.set_clear_label(match clearable {
            0 => None,
            _ => Some(clear.as_str()),
        });
        shown.set_footer(self.footer.label().as_deref());
    }
}

const OPEN: &str = "open";
const SAVE: &str = "save";
const SEND: &str = "send:";
const DEVICE_CHARS: usize = 40;
const OPEN_WITH: &str = "open-with";

fn spill_path(entry: &ClipboardEntry) -> Option<PathBuf> {
    Some(
        glimpse_config::runtime_dir()?
            .join("clipboard")
            .join(entry.id.to_string())
            .join(file_name(entry)),
    )
}

fn file_name(entry: &ClipboardEntry) -> String {
    let subtype = entry.mime.strip_prefix("image/").unwrap_or_default();
    let extension: String = subtype
        .split('+')
        .next()
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    let stem = entry
        .at
        .with_timezone(&chrono::Local)
        .format("Clipboard %Y-%m-%d %H-%M-%S");
    match extension.is_empty() {
        true => format!("{stem}.png"),
        false => format!("{stem}.{extension}"),
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?
        .write_all(bytes)
}

fn forget_spill(id: ClipboardEntryId) {
    if let Some(dir) = glimpse_config::runtime_dir() {
        let _ = std::fs::remove_dir_all(dir.join("clipboard").join(id.to_string()));
    }
}

fn action(key: &str, title: String, value: String) -> ClipAction {
    ClipAction {
        key: key.to_owned(),
        title,
        value,
        choices: Vec::new(),
    }
}

fn act(entry: &ClipboardEntry, key: &str) -> Option<Acting> {
    if let Some(device) = key.strip_prefix(SEND) {
        let device = KdeconnectDeviceId::new(device);
        return match entry.kind {
            ClipboardKind::Image => spill_path(entry)
                .map(|path| Acting::SendFile(device, gio::File::for_path(path).uri().to_string())),
            ClipboardKind::Text => Some(Acting::SendText(
                device,
                String::from_utf8_lossy(&entry.data).into_owned(),
            )),
        };
    }
    if entry.kind == ClipboardKind::Image {
        return match key {
            SAVE => Some(Acting::Save(entry.clone())),
            OPEN_WITH => spill_path(entry).map(Acting::OpenWith),
            _ => None,
        };
    }
    let text = String::from_utf8_lossy(&entry.data);
    match (render::shape(&text), key) {
        (render::Shape::Link { url, .. }, OPEN) => Some(Acting::Open(url)),
        (render::Shape::Path(path), OPEN) => Some(Acting::Open(
            gtk4::gio::File::for_path(render::home_path(&path, &glib::home_dir()))
                .uri()
                .to_string(),
        )),
        (render::Shape::Color(channels), key) => ColorFormat::ALL
            .into_iter()
            .find(|format| format.name() == key)
            .map(|format| Acting::Copy(format.render(channels))),
        _ => None,
    }
}

enum Acting {
    Open(String),
    Copy(String),
    Save(ClipboardEntry),
    OpenWith(PathBuf),
    SendFile(KdeconnectDeviceId, String),
    SendText(KdeconnectDeviceId, String),
}

fn save(entry: ClipboardEntry, notifications: NotificationsProviderHandle) {
    relm4::spawn_local(async move {
        let dialog = gtk4::FileDialog::builder()
            .title(gettext("Save image"))
            .initial_name(file_name(&entry))
            .modal(false)
            .build();
        let Ok(file) = dialog.save_future(None::<&gtk4::Window>).await else {
            return;
        };
        let bytes = glib::Bytes::from(&*entry.data);
        if let Err((_, error)) = file
            .replace_contents_future(bytes, None, false, gio::FileCreateFlags::NONE)
            .await
        {
            let report = Report {
                notifications,
                app_name: gettext("Clipboard"),
                icon: render::ICON.to_owned(),
                summary: gettext("Could not save the image"),
            };
            report_failure("clipboard.save", report, Some(error.to_string()), error).await;
        }
    });
}

fn send<F>(notifications: &NotificationsProviderHandle, operation: &'static str, future: F)
where
    F: std::future::Future<Output = Result<(), CommandError>> + Send + 'static,
{
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Clipboard"),
        icon: render::ICON.to_owned(),
        summary: gettext("Could not send that"),
    };
    let unavailable = gettext("KDE Connect is not running.");
    spawn_reported(
        operation,
        report,
        move |error| wording(error, &unavailable),
        future,
    );
}

fn open_with(path: PathBuf) {
    let launcher = gtk4::FileLauncher::new(Some(&gio::File::for_path(&path)));
    launcher.set_always_ask(true);
    launcher.launch(
        None::<&gtk4::Window>,
        gio::Cancellable::NONE,
        move |result| {
            if let Err(error) = result {
                tracing::warn!(path = %path.display(), %error, "could not open the image");
            }
        },
    );
}

fn open(uri: String) {
    relm4::spawn_local(async move {
        if let Err(error) = gtk4::gio::AppInfo::launch_default_for_uri_future(
            &uri,
            gtk4::gio::AppLaunchContext::NONE,
        )
        .await
        {
            tracing::warn!(uri, %error, "could not open that location");
        }
    });
}

/// A command the viewer asked for answers in words they can read. `spawn_command` only logs, and a
/// row that does nothing when pressed is exactly the failure a person needs told about.
fn tell<F, T>(
    notifications: &NotificationsProviderHandle,
    opener: Opener,
    operation: &'static str,
    summary: String,
    future: F,
) where
    F: std::future::Future<Output = Result<T, CommandError>> + 'static,
    T: 'static,
{
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Clipboard"),
        icon: render::ICON.to_owned(),
        summary,
    };
    let unavailable = gettext("The clipboard is unavailable.");
    relm4::spawn_local(async move {
        let Err(error) = future.await else {
            return;
        };
        opener.wake();
        report_failure(operation, report, wording_for(&error, &unavailable), error).await;
    });
}

fn wording_for(error: &CommandError, unavailable: &str) -> Option<String> {
    match error {
        CommandError::Unavailable(_) => Some(unavailable.to_owned()),
        CommandError::InvalidArgument(_) => {
            Some(gettext("That entry is no longer in the history."))
        }
        _ => None,
    }
}

impl Applet for Clipboard {
    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Clipboard(settings) = &config.kind else {
            return;
        };
        self.settings = settings.clone();
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer.configure(&config.common);
        self.refresh(&ctx.opener());
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => {
                self.state = self.service.snapshot();
                let live: Vec<_> = self.state.entries.iter().map(|entry| entry.id).collect();
                self.images.retain(|id, _| live.contains(id));
                self.files.retain(|id, _| {
                    let kept = live.contains(id);
                    if !kept {
                        forget_spill(*id);
                    }
                    kept
                });
            }
            Input::Tick | Input::Pointer(_) => return,
        }
        self.refresh(&ctx.opener());
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = ClipboardPopover::new();
        let opener = seat.opener();
        self.query.replace(String::new());
        self.expanded.set(false);
        opener.typing(true);

        shown.connect_searched({
            let (query, opener) = (self.query.clone(), opener.clone());
            move |_, text| {
                query.replace(text);
                opener.wake();
            }
        });
        shown.connect_more({
            let (expanded, opener) = (self.expanded.clone(), opener.clone());
            move |_| {
                expanded.set(!expanded.get());
                opener.wake();
            }
        });
        shown.connect_acted({
            let service = self.service.clone();
            let kdeconnect = self.kdeconnect.clone();
            let notifications = self.notifications.clone();
            let opener = opener.clone();
            move |_, id, key| {
                let Some(acting) = service
                    .snapshot()
                    .entries
                    .iter()
                    .find(|entry| entry.id == id)
                    .and_then(|entry| act(entry, &key))
                else {
                    return;
                };
                match acting {
                    Acting::Open(uri) => open(uri),
                    Acting::Save(entry) => save(entry, notifications.clone()),
                    Acting::OpenWith(path) => open_with(path),
                    Acting::SendFile(device, uri) => {
                        let kdeconnect = kdeconnect.clone();
                        send(&notifications, "kdeconnect.share", async move {
                            kdeconnect.share(device, vec![uri]).await
                        });
                    }
                    Acting::SendText(device, text) => {
                        let kdeconnect = kdeconnect.clone();
                        send(&notifications, "kdeconnect.share_text", async move {
                            kdeconnect.share_text(device, text).await
                        });
                    }
                    Acting::Copy(text) => {
                        let service = service.clone();
                        let acknowledged = opener.clone();
                        tell(
                            &notifications,
                            opener.clone(),
                            "clipboard.copy_text",
                            gettext("Could not copy that"),
                            async move {
                                service
                                    .copy_text(text)
                                    .await
                                    .inspect(|_| acknowledged.acknowledge())
                            },
                        );
                    }
                }
                opener.close_popover();
            }
        });

        shown.connect_restored({
            let service = self.service.clone();
            let notifications = self.notifications.clone();
            let opener = opener.clone();
            move |_, id| {
                let service = service.clone();
                let acknowledged = opener.clone();
                tell(
                    &notifications,
                    opener.clone(),
                    "clipboard.restore",
                    gettext("Could not copy that entry"),
                    async move {
                        service
                            .restore(id)
                            .await
                            .inspect(|_| acknowledged.acknowledge())
                    },
                );
                // Copying is what the press meant, so the surface closes on it.
                opener.close_popover();
            }
        });
        shown.connect_pinned({
            let service = self.service.clone();
            let notifications = self.notifications.clone();
            let opener = opener.clone();
            move |_, id, pinned| {
                let service = service.clone();
                tell(
                    &notifications,
                    opener.clone(),
                    "clipboard.pin",
                    gettext("Could not pin that entry"),
                    async move { service.pin(id, pinned).await },
                );
                opener.wake();
            }
        });
        shown.connect_removed({
            let service = self.service.clone();
            let notifications = self.notifications.clone();
            let opener = opener.clone();
            move |_, id| {
                let service = service.clone();
                tell(
                    &notifications,
                    opener.clone(),
                    "clipboard.remove",
                    gettext("Could not forget that entry"),
                    async move { service.remove(id).await },
                );
                opener.wake();
            }
        });
        shown.connect_cleared({
            let service = self.service.clone();
            let notifications = self.notifications.clone();
            let opener = opener.clone();
            move |_| {
                let service = service.clone();
                tell(
                    &notifications,
                    opener.clone(),
                    "clipboard.clear_history",
                    gettext("Could not clear the history"),
                    async move { service.clear_history().await },
                );
                opener.wake();
            }
        });
        let footer = self.footer.clone();
        shown.connect_footer_activated(move |_| footer.run());

        self.shown.set(Some(&shown));
        self.refresh(&opener);
        Some(Box::new(shown))
    }
}
