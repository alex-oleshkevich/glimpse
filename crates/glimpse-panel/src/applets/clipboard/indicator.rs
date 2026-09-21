use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use chrono::Utc;
use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind, ClipboardAppletConfig};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{
    ClipboardEntryId, ClipboardHandle, ClipboardKind, ClipboardState, CommandError,
};
use glimpse_widgets::{Clip, ClipActions, ClipboardPopover, IndicatorSpec};
use gtk4::prelude::*;
use gtk4::{gdk, glib};

use super::render;

/// How often relative subtitles are restated.
const MINUTE: std::time::Duration = std::time::Duration::from_secs(60);

/// The side a row's thumbnail is decoded to. Bounding the decode rather than the bytes is what
/// stops a small image that expands to an enormous bitmap.
const THUMBNAIL: i32 = 48;
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
    /// Which entry has its actions unfolded. The applet's, not the widget's: a popover is rebuilt
    /// on every open and anything that expands something would otherwise come back.
    open: Option<ClipboardEntryId>,
    /// A signal closure has no `&mut self`, so the row it pressed is left here and drained on the
    /// wake it sends. Same shape as `Mpris`'s queued transport actions.
    pressed: Rc<Cell<Option<ClipboardEntryId>>>,
    /// Decoded thumbnails, keyed by entry id. Decoding is the expensive half and an entry's bytes
    /// never change, so a texture is built once and pruned when its entry leaves.
    images: HashMap<ClipboardEntryId, Option<gdk::Texture>>,
    icon: gtk4::gio::Icon,
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
            open: None,
            pressed: Rc::new(Cell::new(None)),
            images: HashMap::new(),
            icon: gtk4::gio::ThemedIcon::new(render::ICON).upcast(),
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

    fn refresh(&mut self) {
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

        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn clips(&mut self, pinned: bool) -> Vec<Clip> {
        let wording = wording();
        let now = Utc::now();
        let entries: Vec<_> = self
            .state
            .entries
            .iter()
            .filter(|entry| entry.pinned == pinned)
            .take(match pinned {
                // `visible` documents itself as counting recent entries only, and `History::evict`
                // goes to real trouble to make a pin outlive eviction; capping here would undo it.
                true => usize::MAX,
                false => self.visible(),
            })
            .cloned()
            .collect();

        entries
            .iter()
            .map(|entry| {
                let image = match entry.kind {
                    ClipboardKind::Image => self.texture(entry.id, &entry.data),
                    ClipboardKind::Text => None,
                };
                Clip {
                    id: entry.id,
                    title: match entry.kind {
                        // An image has no preview the service could make; the row is worded from
                        // what is actually known about it.
                        ClipboardKind::Image => gettext("Image · {size}")
                            .replace("{size}", &render::size(entry.data.len())),
                        ClipboardKind::Text => render::title(entry, self.preview_chars()),
                    },
                    subtitle: render::when(now, entry.at, &wording),
                    icon: render::icon_for(entry.kind).to_owned(),
                    image,
                    pinned: entry.pinned,
                }
            })
            .collect()
    }

    /// An image another application chose, decoded once. A picture that will not decode is not an
    /// error: the entry is still restorable, only its preview failed.
    fn texture(&mut self, id: ClipboardEntryId, data: &[u8]) -> Option<gdk::Texture> {
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
        let pinned = self.clips(true);
        let recent = self.clips(false);
        let total = self.state.entries.len();

        shown.set_actions(ClipActions {
            pin: gettext("Pin"),
            unpin: gettext("Unpin"),
            forget: gettext("Forget"),
        });
        shown.set_subtitle(summary(total, self.state.total_bytes).as_deref());
        shown.set_trouble(self.state.unavailable.as_deref());
        shown.set_pinned(&pinned);
        shown.set_recent(&recent);
        shown.set_open(self.open);
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

fn wording() -> render::Wording {
    render::Wording {
        just_now: gettext("just now"),
        minutes: gettext("{count} min ago"),
        hours: gettext("{count} h ago"),
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
        // Every row's subtitle is a relative time. Without a tick they freeze at whatever they said
        // when the popover opened, and "just now" stays "just now" for as long as it is held open.
        ctx.interval(MINUTE);
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => {
                if let Some(id) = self.pressed.take() {
                    // Pressing the open row's chevron again closes it: the control that opens a
                    // drawer is the one that closes it.
                    self.open = match self.open == Some(id) {
                        true => None,
                        false => Some(id),
                    };
                }
                self.state = self.service.snapshot();
                let live: Vec<_> = self.state.entries.iter().map(|entry| entry.id).collect();
                self.images.retain(|id, _| live.contains(id));
                if self.open.is_some_and(|open| !live.contains(&open)) {
                    self.open = None;
                }
            }
            // The clock moved, so every relative subtitle is restated.
            Input::Tick => {}
            Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        // A fresh widget is not a fresh popover: the applet's own fields survive, so anything that
        // expands something is reset here or it comes back under a row nobody pressed.
        self.open = None;
        let shown = ClipboardPopover::new();
        let opener = seat.opener();

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
        shown.connect_detailed({
            let opener = opener.clone();
            let pressed = Rc::clone(&self.pressed);
            move |_, id| {
                pressed.set(Some(id));
                opener.wake();
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
        self.refresh();
        Some(Box::new(shown))
    }
}
