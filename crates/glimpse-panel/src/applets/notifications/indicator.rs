use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use chrono::Utc;
use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind, NotificationIndicatorStyle};
use glimpse_dbus::notifications::NotificationRecord;
use glimpse_dbus::notifications::{NotificationsProviderHandle, NotificationsProviderState};
use glimpse_services::CompositorHandle;
use glimpse_widgets::{
    Group, IndicatorSpec, NotificationsPopover, notification_icon, notification_image,
};
use gtk4::gdk::prelude::DisplayExt;
use gtk4::gio::prelude::AppLaunchContextExt;
use gtk4::prelude::{Cast, WidgetExt};
use gtk4::{gdk, gio, glib};

use crate::applet::popover::Seat;
use crate::applet::{Applet, Ctx, Input, Opener, spawn_command};

use super::render;

const MINUTE: Duration = Duration::from_secs(60);
type ImageCache = HashMap<String, Rc<RefCell<Option<gdk::Texture>>>>;

pub struct Notifications {
    list: Option<Vec<NotificationRecord>>,
    dnd: Rc<Cell<bool>>,
    trouble: Option<String>,
    tooltip_format: Option<String>,
    indicator_style: NotificationIndicatorStyle,
    held: Rc<RefCell<Vec<NotificationRecord>>>,
    icons: HashMap<String, gio::Icon>,
    images: Rc<RefCell<ImageCache>>,
    opener: Option<Opener>,
    bell: gio::Icon,
    muted: gio::Icon,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<NotificationsPopover>,
    notifications: NotificationsProviderHandle,
    compositor: CompositorHandle,
}

impl Applet for Notifications {
    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Notifications(settings) = &config.kind else {
            return;
        };
        self.indicator_style = settings.indicator_style;
        self.tooltip_format = config.kind.tooltip_format().map(str::to_owned);

        ctx.interval(MINUTE);
        self.opener = Some(ctx.opener());
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => self.sync(),
            Input::Tick => {}
            Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<gtk4::Widget> {
        let shown = NotificationsPopover::new();
        let opener = seat.opener();
        let held = self.held.clone();
        let dnd = self.dnd.clone();

        shown.connect_activated({
            let notifications = self.notifications.clone();
            let compositor = self.compositor.clone();
            let held = held.clone();
            move |popover, key| activate(&notifications, &compositor, popover, &held, &key)
        });
        shown.connect_action_invoked({
            let notifications = self.notifications.clone();
            let held = held.clone();
            let opener = opener.clone();
            move |popover, key, action| {
                invoke(&notifications, popover, &held, &key, &action);
                opener.close_popover();
            }
        });
        shown.connect_dismissed({
            let notifications = self.notifications.clone();
            let held = held.clone();
            move |_, key| dismiss(&notifications, &held, &key)
        });
        shown.connect_clear_group({
            let notifications = self.notifications.clone();
            move |_, app_id| {
                let notifications = notifications.clone();
                spawn_command("notifications.clear_application", async move {
                    notifications.clear_application(app_id).await
                });
            }
        });
        shown.connect_clear_all({
            let notifications = self.notifications.clone();
            move |_| {
                let notifications = notifications.clone();
                spawn_command("notifications.clear_all", async move {
                    notifications.clear_all().await
                });
            }
        });
        shown.connect_dnd_toggled({
            let notifications = self.notifications.clone();
            move |_, silenced| {
                dnd.set(silenced);
                let notifications = notifications.clone();
                spawn_command("notifications.set_do_not_disturb", async move {
                    notifications.set_do_not_disturb(silenced, 0).await
                });
                opener.wake();
            }
        });

        self.shown.set(Some(&shown));
        self.refresh();
        Some(shown.upcast())
    }
}

impl Notifications {
    pub fn start(notifications: NotificationsProviderHandle, compositor: CompositorHandle) -> Self {
        let mut this = Self {
            list: None,
            dnd: Rc::new(Cell::new(false)),
            trouble: None,
            tooltip_format: None,
            indicator_style: NotificationIndicatorStyle::default(),
            held: Rc::new(RefCell::new(Vec::new())),
            icons: HashMap::new(),
            images: Rc::new(RefCell::new(HashMap::new())),
            opener: None,
            bell: gio::ThemedIcon::new(render::BELL).upcast(),
            muted: gio::ThemedIcon::new(render::MUTED).upcast(),
            spec: Vec::new(),
            shown: glib::WeakRef::new(),
            notifications,
            compositor,
        };
        this.sync();
        this
    }

    fn sync(&mut self) {
        let NotificationsProviderState { view, unavailable } = self.notifications.snapshot();
        self.trouble = unavailable;
        let Some(view) = view else {
            self.list = None;
            self.dnd.set(false);
            return;
        };
        self.list = Some(view.notifications);
        self.dnd.set(view.do_not_disturb.enabled);
        if !view.serving {
            self.trouble = Some(view.reason);
        }
    }

    fn refresh(&mut self) {
        let Some(records) = self.list.clone() else {
            self.spec.clear();
            *self.held.borrow_mut() = Vec::new();
            self.icons.clear();
            self.images.borrow_mut().clear();
            if let Some(shown) = self.shown.upgrade() {
                self.dress(&shown, &[]);
            }
            return;
        };
        *self.held.borrow_mut() = records.clone();
        self.spec = vec![self.indicator(&records)];

        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown, &records);
        }
    }

    fn indicator(&self, records: &[NotificationRecord]) -> IndicatorSpec {
        let dnd = self.dnd.get();
        let chip = render::chip(records, dnd, self.indicator_style);
        IndicatorSpec {
            icon: Some(if chip.icon == render::MUTED {
                self.muted.clone()
            } else {
                self.bell.clone()
            }),
            badge: chip.badge,
            attention: chip.attention,
            severity: chip.severity,
            tooltip: Some(render::tooltip(
                records,
                dnd,
                self.tooltip_format.as_deref(),
            )),
            ..Default::default()
        }
    }

    fn dress(&mut self, shown: &NotificationsPopover, records: &[NotificationRecord]) {
        shown.set_dnd(self.dnd.get());
        shown.set_trouble(self.trouble.as_deref());
        shown.set_clear_label(Some(&gettext("Clear all")));

        shown.set_groups(&self.filled(records));
    }

    fn filled(&mut self, records: &[NotificationRecord]) -> Vec<Group> {
        prune_cache(&mut self.icons, records.iter().map(icon_key));
        prune_cache(
            &mut self.images.borrow_mut(),
            records.iter().filter_map(|record| record.image.as_deref()),
        );
        let now = Utc::now();
        let mut groups = render::groups(records, now);
        let records: HashMap<_, _> = records.iter().map(|record| (record.id, record)).collect();
        for group in &mut groups {
            for note in &mut group.notifications {
                let Some(record) =
                    render::id_of(&note.key).and_then(|id| records.get(&id).copied())
                else {
                    continue;
                };
                note.icon = Some(self.themed(icon_key(record)));
                note.image = record.image.as_deref().and_then(|path| self.image(path));
            }
        }
        groups
    }

    fn themed(&mut self, name: &str) -> gio::Icon {
        self.icons
            .entry(name.to_owned())
            .or_insert_with(|| notification_icon(name))
            .clone()
    }

    fn image(&mut self, path: &str) -> Option<gdk::Texture> {
        if let Some(texture) = self.images.borrow().get(path) {
            return texture.borrow().clone();
        }
        let opener = self.opener.clone()?;
        let path = path.to_owned();
        let target = path.clone();
        let load = load_image(
            self.images.clone(),
            path,
            gio::spawn_blocking(move || notification_image(Path::new(&target))),
        );
        glib::spawn_future_local(async move {
            load.await;
            opener.wake();
        });
        None
    }
}

fn load_image(
    images: Rc<RefCell<ImageCache>>,
    path: String,
    load: impl Future<Output = std::thread::Result<Option<gdk::Texture>>>,
) -> impl Future<Output = ()> {
    let image = images.borrow_mut().entry(path).or_default().clone();
    async move {
        match load.await {
            Ok(loaded) => *image.borrow_mut() = loaded,
            Err(_) => tracing::debug!("notification image load panicked"),
        }
    }
}

fn icon_key(record: &NotificationRecord) -> &str {
    record.icon.as_deref().unwrap_or(&record.app_id)
}

fn dismiss(
    notifications: &NotificationsProviderHandle,
    records: &RefCell<Vec<NotificationRecord>>,
    key: &str,
) {
    let id = {
        let records = records.borrow();
        removal(&records, key)
    };
    if let Some(id) = id {
        let notifications = notifications.clone();
        spawn_command("notifications.remove", async move {
            notifications.remove(id).await
        });
    }
}

fn removal(records: &[NotificationRecord], key: &str) -> Option<u32> {
    let id = render::id_of(key)?;
    records.iter().any(|record| record.id == id).then_some(id)
}

fn invoke(
    notifications: &NotificationsProviderHandle,
    widget: &NotificationsPopover,
    records: &RefCell<Vec<NotificationRecord>>,
    key: &str,
    action: &str,
) {
    let request = {
        let records = records.borrow();
        invocation(&records, key, action)
    };
    let Some(request) = request else {
        return;
    };
    let notifications = notifications.clone();
    let action = action.to_owned();
    let token = token(widget);
    spawn_command("notifications.invoke_action", async move {
        notifications.invoke_action(request, action, token).await
    });
}

fn activate(
    notifications: &NotificationsProviderHandle,
    compositor: &CompositorHandle,
    widget: &NotificationsPopover,
    records: &RefCell<Vec<NotificationRecord>>,
    key: &str,
) {
    let request = {
        let records = records.borrow();
        activation(&records, key)
    };
    let Some(request) = request else {
        return;
    };
    let notifications = notifications.clone();
    let compositor = compositor.clone();
    let activation_token = token(widget);
    relm4::spawn(async move {
        match tokio::time::timeout(
            Duration::from_secs(5),
            compositor.focus_app(request.app_pid, &[&request.app_id, &request.app_name]),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(operation = "compositor.focus_app", %error, "service command failed")
            }
            Err(_) => tracing::warn!(
                operation = "compositor.focus_app",
                "service command timed out"
            ),
        }
        if let Err(error) = notifications.activate(request.id, activation_token).await {
            tracing::warn!(operation = "notifications.activate", %error, "service command failed");
        }
    });
}

fn activation(records: &[NotificationRecord], key: &str) -> Option<NotificationRecord> {
    let id = render::id_of(key)?;
    records
        .iter()
        .find(|record| record.id == id && record.unread)
        .cloned()
}

fn invocation(records: &[NotificationRecord], key: &str, action: &str) -> Option<u32> {
    let id = render::id_of(key)?;
    let record = records.iter().find(|record| record.id == id)?;
    if !record.unread || !record.actions.iter().any(|offer| offer.key == action) {
        return None;
    }
    Some(id)
}

fn prune_cache<'a, T>(cache: &mut HashMap<String, T>, wanted: impl Iterator<Item = &'a str>) {
    let wanted: HashSet<&str> = wanted.collect();
    cache.retain(|key, _| wanted.contains(key.as_str()));
}

fn token(widget: &NotificationsPopover) -> Option<String> {
    let id = widget
        .display()
        .app_launch_context()
        .startup_notify_id(gio::AppInfo::NONE, &[])?;
    (!id.is_empty()).then(|| id.to_string())
}

#[cfg(test)]
mod tests;
