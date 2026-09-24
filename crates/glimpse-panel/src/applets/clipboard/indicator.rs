use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gettextrs::{gettext, ngettext};
use glimpse_config::{Applet as AppletConfig, AppletKind, ClipboardAppletConfig, ColorFormat};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{
    ClipboardEntry, ClipboardEntryId, ClipboardHandle, ClipboardKind, ClipboardState, CommandError,
};
use glimpse_widgets::{
    Clip, ClipAction, ClipActions, ClipboardPopover, Fact, IndicatorSpec, Thumbnail, rgba,
};
use gtk4::prelude::*;
use gtk4::{gdk, glib};

use super::render;

/// The side a tile's thumbnail is decoded to. Bounding the decode rather than the bytes is what
/// stops a small image that expands to an enormous bitmap.
const THUMBNAIL: i32 = 320;
const EXCERPT_LINES: usize = 6;
const EXCERPT_CHARS: usize = 200;
use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Input, Opener, Report, report_failure};

pub struct Clipboard {
    service: ClipboardHandle,
    notifications: NotificationsProviderHandle,
    state: ClipboardState,
    settings: ClipboardAppletConfig,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    shown: glib::WeakRef<ClipboardPopover>,
    spec: Vec<IndicatorSpec>,
    /// Decoded thumbnails, keyed by entry id. Decoding is the expensive half and an entry's bytes
    /// never change, so a texture is built once and pruned when its entry leaves.
    images: HashMap<ClipboardEntryId, Option<Thumbnail>>,
    icon: gtk4::gio::Icon,
    query: Rc<RefCell<String>>,
    expanded: Rc<Cell<bool>>,
}

impl Clipboard {
    pub fn start(service: ClipboardHandle, notifications: NotificationsProviderHandle) -> Self {
        Self {
            state: service.snapshot(),
            service,
            notifications,
            settings: ClipboardAppletConfig::default(),
            tooltip_format: None,
            footer: None,
            shown: glib::WeakRef::new(),
            spec: Vec::new(),
            images: HashMap::new(),
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
            return Clip {
                id: entry.id,
                title,
                icon: render::IMAGE_ICON.to_owned(),
                image: decoded.map(|(texture, _)| texture),
                facts,
                pinned: entry.pinned,
                ..Default::default()
            };
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
                clip.subtitle = render::title_of(&host, self.preview_chars());
                clip.icon = render::LINK_ICON.to_owned();
                clip.actions = vec![action(OPEN, gettext("Open in browser"), String::new())];
            }
            render::Shape::Color(channels) => {
                clip.subtitle = gettext("Color");
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
                clip.subtitle = gettext("Path");
                clip.icon = render::PATH_ICON.to_owned();
                clip.actions = vec![action(OPEN, gettext("Open"), String::new())];
            }
            render::Shape::Lines(count) => {
                clip.subtitle = ngettext("{count} line", "{count} lines", count as u32)
                    .replace("{count}", &count.to_string());
            }
            render::Shape::Plain => {}
        }
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

    fn dress(&mut self, shown: &ClipboardPopover) {
        let (pinned, _) = self.clips(true);
        let (recent, hidden) = self.clips(false);
        let total = self.state.entries.len();

        shown.set_actions(ClipActions {
            pin: gettext("Pin"),
            unpin: gettext("Unpin"),
            forget: gettext("Forget"),
        });
        shown.set_subtitle(
            summary(total, self.state.total_bytes)
                .or_else(|| {
                    self.state
                        .unavailable
                        .is_none()
                        .then(|| gettext("Nothing copied yet"))
                })
                .as_deref(),
        );
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
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
    }
}

const OPEN: &str = "open";

fn action(key: &str, title: String, value: String) -> ClipAction {
    ClipAction {
        key: key.to_owned(),
        title,
        value,
    }
}

fn act(entry: &ClipboardEntry, key: &str) -> Option<Acting> {
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

/// Nothing when the history is empty: the placeholder in the card already says so, and a hero
/// repeating it word for word makes the surface state it twice.
fn summary(count: usize, bytes: usize) -> Option<String> {
    match count {
        0 => None,
        _ => Some(
            gettext("{count} items · {size}")
                .replace("{count}", &count.to_string())
                .replace("{size}", &render::size(bytes)),
        ),
    }
}

impl Applet for Clipboard {
    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Clipboard(settings) = &config.kind else {
            return;
        };
        self.settings = settings.clone();
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()));
        self.refresh(&ctx.opener());
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => {
                self.state = self.service.snapshot();
                let live: Vec<_> = self.state.entries.iter().map(|entry| entry.id).collect();
                self.images.retain(|id, _| live.contains(id));
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
                    Acting::Copy(text) => {
                        let service = service.clone();
                        tell(
                            &notifications,
                            opener.clone(),
                            "clipboard.copy_text",
                            gettext("Could not copy that"),
                            async move { service.copy_text(text).await },
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
                tell(
                    &notifications,
                    opener.clone(),
                    "clipboard.restore",
                    gettext("Could not copy that entry"),
                    async move { service.restore(id).await },
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
        if let Some((_, command)) = &self.footer {
            let command = command.clone();
            shown.connect_footer_activated(move |_| run(&command));
        }

        self.shown.set(Some(&shown));
        self.refresh(&opener);
        Some(Box::new(shown))
    }
}
