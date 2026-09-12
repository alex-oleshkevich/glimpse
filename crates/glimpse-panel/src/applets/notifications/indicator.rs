use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use chrono::Utc;
use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind, NotificationIndicatorStyle};
use glimpse_contracts::{
    DoNotDisturb, FocusWindow, Message as _, NotificationRecord, NotificationsActivate,
    NotificationsClearAll, NotificationsClearApp, NotificationsDnd, NotificationsInvokeAction,
    NotificationsList, NotificationsRemove, NotificationsSetDnd, ServiceState, SystemServices,
    WindowRef,
};
use glimpse_widgets::{Group, IndicatorSpec, NotificationsPopover, artwork, notification_image};
use gtk4::gdk::prelude::DisplayExt;
use gtk4::gio::prelude::AppLaunchContextExt;
use gtk4::prelude::{Cast, WidgetExt};
use gtk4::{gdk, gio, glib};

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Caller, Ctx, Input, payload};

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
    avatars: HashMap<String, Option<gdk::Texture>>,
    images: HashMap<String, Option<gdk::Texture>>,
    bell: gio::Icon,
    muted: gio::Icon,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<NotificationsPopover>,
}

impl Applet for Notifications {
    fn topics(&self) -> &'static [&'static str] {
        &[
            NotificationsList::NAME,
            NotificationsDnd::NAME,
            SystemServices::NAME,
        ]
    }

    fn start() -> Self {
        Self {
            list: None,
            dnd: Rc::new(Cell::new(false)),
            trouble: None,
            tooltip_format: None,
            footer: None,
            indicator_style: NotificationIndicatorStyle::default(),
            held: Rc::new(RefCell::new(Vec::new())),
            icons: HashMap::new(),
            avatars: HashMap::new(),
            images: HashMap::new(),
            bell: gio::ThemedIcon::new(render::BELL).upcast(),
            muted: gio::ThemedIcon::new(render::MUTED).upcast(),
            spec: Vec::new(),
            shown: glib::WeakRef::new(),
        }
    }

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
            Input::Topic(event) => {
                if let Some(list) = payload::<NotificationsList>(event) {
                    self.list = Some(list.notifications);
                } else if let Some(status) = payload::<NotificationsDnd>(event) {
                    self.dnd.set(render::silenced(status.dnd));
                } else if let Some(status) = payload::<SystemServices>(event) {
                    self.trouble = match status.services.get("notifications") {
                        Some(ServiceState::Degraded { reason }) => Some(reason.clone()),
                        _ => None,
                    };
                } else {
                    return;
                }
            }
            Input::Tick | Input::Woken => {}
            Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = NotificationsPopover::new();
        let caller = seat.caller();
        let opener = seat.opener();
        let held = self.held.clone();
        let dnd = self.dnd.clone();

        shown.connect_activated({
            let caller = caller.clone();
            let held = held.clone();
            move |popover, key| activate(&caller, popover, &held, &key)
        });
        shown.connect_action_invoked({
            let caller = caller.clone();
            let held = held.clone();
            move |popover, key, action| invoke(&caller, popover, &held, &key, &action)
        });
        shown.connect_dismissed({
            let caller = caller.clone();
            let held = held.clone();
            move |_, key| dismiss(&caller, &held, &key)
        });
        shown.connect_clear_group({
            let caller = caller.clone();
            move |_, app_id| {
                caller.call::<NotificationsClearApp>(NotificationsClearApp { app_id });
            }
        });
        shown.connect_clear_all({
            let caller = caller.clone();
            move |_| {
                caller.call::<NotificationsClearAll>(NotificationsClearAll {});
            }
        });
        shown.connect_dnd_toggled({
            let caller = caller.clone();
            move |_, silenced| {
                dnd.set(silenced);
                caller.call::<NotificationsSetDnd>(NotificationsSetDnd {
                    dnd: DoNotDisturb {
                        enabled: silenced,
                        until: None,
                    },
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
    fn refresh(&mut self) {
        let Some(records) = self.list.clone() else {
            self.spec.clear();
            *self.held.borrow_mut() = Vec::new();
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
            &mut self.avatars,
            records.iter().filter_map(|record| record.avatar.as_deref()),
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
                note.avatar = record.avatar.as_deref().and_then(|path| self.avatar(path));
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

    fn avatar(&mut self, path: &str) -> Option<gdk::Texture> {
        self.avatars
            .entry(path.to_owned())
            .or_insert_with(|| artwork(Path::new(path), 64))
            .clone()
    }

    fn image(&mut self, path: &str) -> Option<gdk::Texture> {
        self.images
            .entry(path.to_owned())
            .or_insert_with(|| notification_image(Path::new(path)))
            .clone()
    }
}

fn dismiss(caller: &Caller, records: &RefCell<Vec<NotificationRecord>>, key: &str) {
    let id = {
        let records = records.borrow();
        removal(&records, key)
    };
    if let Some(id) = id {
        caller.call::<NotificationsRemove>(NotificationsRemove { id });
    }
}

fn removal(records: &[NotificationRecord], key: &str) -> Option<u32> {
    let id = render::id_of(key)?;
    records.iter().any(|record| record.id == id).then_some(id)
}

fn invoke(
    caller: &Caller,
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
    caller.call::<NotificationsInvokeAction>(NotificationsInvokeAction {
        id: request,
        action: action.to_owned(),
        activation_token: token(widget),
    });
}

fn activate(
    caller: &Caller,
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
    if let Some(pid) = request.pid {
        caller.call::<FocusWindow>(FocusWindow {
            target: WindowRef::Pid { pid },
        });
    }
    caller.call::<NotificationsActivate>(NotificationsActivate {
        id: request.id,
        activation_token: token(widget),
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
    use glimpse_contracts::{DEFAULT_ACTION, NotificationAction, NotificationUrgency};

    use super::*;

    fn record(actions: &[&str], unread: bool) -> NotificationRecord {
        NotificationRecord {
            id: 7,
            app_id: "app".to_owned(),
            app_name: "App".to_owned(),
            app_pid: Some(42),
            icon: None,
            avatar: None,
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
