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
use glimpse_services::WindowRef;
use glimpse_widgets::{Group, IndicatorSpec, NotificationsPopover, notification_image};
use gtk4::gdk::prelude::DisplayExt;
use gtk4::gio::prelude::AppLaunchContextExt;
use gtk4::prelude::{Cast, WidgetExt};
use gtk4::{gdk, gio, glib};

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Input, spawn_command};

use super::render;

const MINUTE: Duration = Duration::from_secs(60);
pub struct Notifications {
    list: Option<Vec<NotificationRecord>>,
    dnd: Rc<Cell<bool>>,
    trouble: Option<String>,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    indicator_style: NotificationIndicatorStyle,
    held: Rc<RefCell<Vec<NotificationRecord>>>,
    icons: HashMap<String, gio::Icon>,
    images: HashMap<String, Option<gdk::Texture>>,
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
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()));
        ctx.interval(MINUTE);
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

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
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
            move |popover, key, action| invoke(&notifications, popover, &held, &key, &action)
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
        if let Some((_, command)) = &self.footer {
            let command = command.clone();
            shown.connect_footer_activated(move |_| run(&command));
        }

        self.shown.set(Some(&shown));
        self.refresh();
        Some(Box::new(shown))
    }
}

impl Notifications {
    pub fn start(notifications: NotificationsProviderHandle, compositor: CompositorHandle) -> Self {
        let mut this = Self {
            list: None,
            dnd: Rc::new(Cell::new(false)),
            trouble: None,
            tooltip_format: None,
            footer: None,
            indicator_style: NotificationIndicatorStyle::default(),
            held: Rc::new(RefCell::new(Vec::new())),
            icons: HashMap::new(),
            images: HashMap::new(),
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
            self.images.clear();
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
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
        shown.set_groups(&self.filled(records));
    }

    fn filled(&mut self, records: &[NotificationRecord]) -> Vec<Group> {
        prune_cache(
            &mut self.icons,
            records.iter().filter_map(|record| record.icon.as_deref()),
        );
        prune_cache(
            &mut self.images,
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
                note.icon = record.icon.as_deref().map(|name| self.themed(name));
                note.image = record.image.as_deref().and_then(|path| self.image(path));
            }
        }
        groups
    }

    fn themed(&mut self, name: &str) -> gio::Icon {
        self.icons
            .entry(name.to_owned())
            .or_insert_with(|| gio::ThemedIcon::new(name).upcast())
            .clone()
    }

    fn image(&mut self, path: &str) -> Option<gdk::Texture> {
        self.images
            .entry(path.to_owned())
            .or_insert_with(|| notification_image(Path::new(path)))
            .clone()
    }
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
        if let Some(pid) = request.pid {
            match tokio::time::timeout(
                Duration::from_secs(5),
                compositor.focus_window(WindowRef::Pid { pid }),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::warn!(operation = "compositor.focus_window", %error, "service command failed")
                }
                Err(_) => tracing::warn!(
                    operation = "compositor.focus_window",
                    "service command timed out"
                ),
            }
        }
        if let Err(error) = notifications.activate(request.id, activation_token).await {
            tracing::warn!(operation = "notifications.activate", %error, "service command failed");
        }
    });
}

#[derive(Debug, PartialEq, Eq)]
struct Activation {
    id: u32,
    pid: Option<i32>,
}

fn activation(records: &[NotificationRecord], key: &str) -> Option<Activation> {
    let id = render::id_of(key)?;
    let record = records
        .iter()
        .find(|record| record.id == id && record.unread)?;
    Some(Activation {
        id,
        pid: record.app_pid,
    })
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
mod tests {
    use chrono::Utc;
    use glimpse_dbus::notifications::{DEFAULT_ACTION, NotificationAction, NotificationUrgency};

    use super::*;

    fn record(actions: &[&str], unread: bool) -> NotificationRecord {
        NotificationRecord {
            id: 7,
            app_id: "app".to_owned(),
            app_name: "App".to_owned(),
            app_pid: Some(42),
            icon: None,
            image: None,
            summary: "Summary".to_owned(),
            body: None,
            urgency: NotificationUrgency::Normal,
            actions: actions
                .iter()
                .map(|key| NotificationAction {
                    key: (*key).to_owned(),
                    label: "Action".to_owned(),
                })
                .collect(),
            progress: None,
            created: Utc::now(),
            unread,
            resident: false,
            expire_timeout: -1,
        }
    }

    #[test]
    fn only_a_live_offered_action_becomes_an_invocation() {
        let live = record(&[DEFAULT_ACTION, "reply"], true);
        assert_eq!(invocation(&[live], "7", DEFAULT_ACTION), Some(7));
        assert!(invocation(&[record(&["reply"], true)], "7", DEFAULT_ACTION).is_none());
        assert!(invocation(&[record(&[DEFAULT_ACTION], false)], "7", DEFAULT_ACTION).is_none());
    }

    #[test]
    fn any_unread_card_can_activate_and_default_action_is_optional() {
        assert_eq!(
            activation(&[record(&[], true)], "7"),
            Some(Activation {
                id: 7,
                pid: Some(42),
            })
        );
        assert_eq!(
            activation(&[record(&[DEFAULT_ACTION], true)], "7"),
            Some(Activation {
                id: 7,
                pid: Some(42),
            })
        );
        assert!(activation(&[record(&[], false)], "7").is_none());
    }

    #[test]
    fn closing_any_card_removes_it_from_the_popover() {
        assert_eq!(removal(&[record(&[], true)], "7"), Some(7));
        assert_eq!(removal(&[record(&[], false)], "7"), Some(7));
        assert_eq!(removal(&[record(&[], true)], "wrong"), None);
    }

    #[test]
    fn caches_keep_only_keys_present_in_the_latest_list() {
        let mut cache = HashMap::from([("gone".to_owned(), 1), ("kept".to_owned(), 2)]);
        prune_cache(&mut cache, ["kept"].into_iter());

        assert_eq!(cache, HashMap::from([("kept".to_owned(), 2)]));
    }
}
