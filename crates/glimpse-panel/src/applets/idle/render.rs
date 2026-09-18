use chrono::{DateTime, Utc};
use gettextrs::{gettext, ngettext};
use glimpse_dbus::idle::{IdleInhibitorRecord, IdleProviderState, SourceKind};
use glimpse_widgets::{InhibitorEntry, InhibitorSource, InhibitorTargets};

pub const ICON_IDLE: &str = "preferences-desktop-screensaver-symbolic";
pub const ICON_ACTIVE: &str = "screen-shared-symbolic";
const WHY_CAP: usize = 80;

pub fn icon(active: bool) -> &'static str {
    match active {
        true => ICON_ACTIVE,
        false => ICON_IDLE,
    }
}

pub fn hero_subtitle(state: &IdleProviderState, manual_hold: &[u64]) -> String {
    if !state.available {
        return gettext("Idle daemon not running");
    }
    if state.inhibitors.is_empty() {
        return gettext("Nothing preventing idle");
    }

    let mine = state
        .inhibitors
        .iter()
        .any(|record| manual_hold.contains(&record.id));
    let others: Vec<&IdleInhibitorRecord> = state
        .inhibitors
        .iter()
        .filter(|record| !manual_hold.contains(&record.id))
        .collect();

    match (mine, others.len()) {
        (true, 0) => gettext("Awake until you turn it off"),
        (true, n) => format!(
            "{} · {}",
            gettext("Awake until you turn it off"),
            apps_wording(&others, n)
        ),
        (false, n) => apps_wording(&others, n),
    }
}

fn apps_wording(records: &[&IdleInhibitorRecord], count: usize) -> String {
    let sleepy = records
        .iter()
        .any(|record| record.targets.suspend || record.targets.shutdown);
    let template = match sleepy {
        true => ngettext(
            "{n} app preventing idle or sleep",
            "{n} apps preventing idle or sleep",
            count as u32,
        ),
        false => ngettext(
            "{n} app preventing idle",
            "{n} apps preventing idle",
            count as u32,
        ),
    };
    template.replace("{n}", &count.to_string())
}

pub fn tooltip(
    state: &IdleProviderState,
    manual_hold: &[u64],
    format: Option<&str>,
) -> Option<String> {
    if !state.available {
        return state.reason.clone();
    }
    let status = hero_subtitle(state, manual_hold);
    Some(match format {
        None => status,
        Some(format) => crate::applets::tokens::render(format, |token| match token {
            "status" => Some(status.as_str()),
            _ => None,
        }),
    })
}

pub fn newly_adopted_holds(records: &[IdleInhibitorRecord], known_ids: &[u64]) -> Vec<u64> {
    records
        .iter()
        .filter(|record| !known_ids.contains(&record.id) && is_manual_hold(record))
        .map(|record| record.id)
        .collect()
}

pub fn manual_hold_ids(records: &[IdleInhibitorRecord]) -> Vec<u64> {
    records
        .iter()
        .filter(|record| is_manual_hold(record))
        .map(|record| record.id)
        .collect()
}

fn is_manual_hold(record: &IdleInhibitorRecord) -> bool {
    record.can_release
        && record.source.kind == SourceKind::Login1
        && record.who == "glimpse-idle"
        && record.why == "Manual hold"
}

pub fn row_label(record: &IdleInhibitorRecord) -> String {
    if !record.process_name.is_empty() {
        return record.process_name.clone();
    }
    if !record.who.is_empty() {
        return record.who.clone();
    }
    if !record.bus_name.is_empty() {
        return record.bus_name.clone();
    }
    gettext("Unknown")
}

fn relative(now: DateTime<Utc>, added_at_unix: u64) -> String {
    let added = DateTime::<Utc>::from_timestamp(added_at_unix as i64, 0).unwrap_or(now);
    let minutes = (now - added).num_minutes().max(0);
    if minutes < 1 {
        return gettext("just now");
    }
    if minutes < 60 {
        return ngettext("{n} minute ago", "{n} minutes ago", minutes as u32)
            .replace("{n}", &minutes.to_string());
    }
    let hours = minutes / 60;
    ngettext("{n} hour ago", "{n} hours ago", hours as u32).replace("{n}", &hours.to_string())
}

fn source_suffix(record: &IdleInhibitorRecord) -> Option<String> {
    match record.source.kind {
        SourceKind::Portal => Some(gettext("(Flatpak via portal)")),
        SourceKind::Login1 => Some(
            gettext("(systemd-inhibit · pid {pid})")
                .replace("{pid}", &record.source.pid.to_string()),
        ),
        SourceKind::ScreenSaver => None,
    }
}

fn row_status(record: &IdleInhibitorRecord, mine: bool, now: DateTime<Utc>) -> String {
    let why = match record.why.is_empty() {
        true => gettext("Preventing idle"),
        false => glimpse_utils::clean(&record.why, WHY_CAP),
    };
    let mut status = format!("{why} · {}", relative(now, record.added_at_unix));
    if !mine && let Some(suffix) = source_suffix(record) {
        status = format!("{status} {suffix}");
    }
    status
}

fn row_source(kind: SourceKind, mine: bool) -> InhibitorSource {
    if mine {
        return InhibitorSource::ManualHold;
    }
    match kind {
        SourceKind::ScreenSaver => InhibitorSource::ScreenSaver,
        SourceKind::Portal => InhibitorSource::Portal,
        SourceKind::Login1 => InhibitorSource::Login1,
    }
}

fn row_targets(record: &IdleInhibitorRecord) -> InhibitorTargets {
    InhibitorTargets {
        idle: record.targets.idle,
        suspend: record.targets.suspend,
        shutdown: record.targets.shutdown,
        lid_switch: record.targets.lid_switch,
        power_key: record.targets.power_key,
        suspend_key: record.targets.suspend_key,
        hibernate_key: record.targets.hibernate_key,
    }
}

pub fn to_inhibitor_entries(
    records: &[IdleInhibitorRecord],
    manual_hold: &[u64],
    now: DateTime<Utc>,
) -> Vec<InhibitorEntry> {
    records
        .iter()
        .map(|record| {
            let mine = manual_hold.contains(&record.id);
            InhibitorEntry {
                id: record.id,
                source: row_source(record.source.kind, mine),
                label: row_label(record),
                status: row_status(record, mine, now),
                targets: row_targets(record),
                can_release: record.can_release,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;
    use glimpse_dbus::idle::{
        BackendHealth, HealthKind, IdleInhibitorSource, InhibitionTargets, InhibitorsHealth,
        Login1Mode,
    };

    use super::*;

    fn healthy() -> InhibitorsHealth {
        let ready = BackendHealth {
            kind: HealthKind::Ready,
            message: String::new(),
        };
        InhibitorsHealth {
            screen_saver: ready.clone(),
            portal: ready.clone(),
            login1: ready,
        }
    }

    fn state(inhibitors: Vec<IdleInhibitorRecord>) -> IdleProviderState {
        IdleProviderState {
            inhibitors,
            health: healthy(),
            available: true,
            reason: None,
            owner: true,
        }
    }

    fn record(
        id: u64,
        targets: InhibitionTargets,
        source: IdleInhibitorSource,
    ) -> IdleInhibitorRecord {
        IdleInhibitorRecord {
            id,
            who: "Firefox".to_owned(),
            why: "Playing video".to_owned(),
            bus_name: ":1.42".to_owned(),
            process_name: "firefox-bin".to_owned(),
            source,
            targets,
            can_release: true,
            added_at_unix: 0,
        }
    }

    fn manual_hold_record(id: u64) -> IdleInhibitorRecord {
        IdleInhibitorRecord {
            id,
            who: "glimpse-idle".to_owned(),
            why: "Manual hold".to_owned(),
            bus_name: ":1.7".to_owned(),
            process_name: String::new(),
            source: IdleInhibitorSource::login1(4242, 1000, Login1Mode::Block),
            targets: InhibitionTargets::manual_hold(),
            can_release: true,
            added_at_unix: 0,
        }
    }

    fn screen_saver_record(id: u64) -> IdleInhibitorRecord {
        record(
            id,
            InhibitionTargets::idle_only(),
            IdleInhibitorSource::screen_saver(7),
        )
    }

    #[test]
    fn a_daemon_with_no_owner_reports_itself_first() {
        let mut down = state(vec![screen_saver_record(1)]);
        down.available = false;
        assert_eq!(hero_subtitle(&down, &[]), "Idle daemon not running");
    }

    #[test]
    fn no_inhibitors_at_all_reads_as_nothing_preventing_idle() {
        assert_eq!(
            hero_subtitle(&state(vec![]), &[]),
            "Nothing preventing idle"
        );
    }

    #[test]
    fn a_lone_manual_hold_says_it_is_the_reason() {
        let held = state(vec![manual_hold_record(9)]);
        assert_eq!(hero_subtitle(&held, &[9]), "Awake until you turn it off");
    }

    #[test]
    fn a_hold_id_not_present_any_more_is_not_trusted() {
        let held = state(vec![screen_saver_record(1)]);
        assert_eq!(hero_subtitle(&held, &[999]), "1 app preventing idle");
    }

    #[test]
    fn two_holds_of_our_own_still_read_as_one_awake_clause() {
        let held = state(vec![manual_hold_record(9), manual_hold_record(10)]);
        assert_eq!(
            hero_subtitle(&held, &[9, 10]),
            "Awake until you turn it off"
        );
    }

    #[test]
    fn idle_only_apps_say_idle_and_nothing_about_sleep() {
        let held = state(vec![screen_saver_record(1), screen_saver_record(2)]);
        assert_eq!(hero_subtitle(&held, &[]), "2 apps preventing idle");
    }

    #[test]
    fn one_app_taking_suspend_upgrades_the_whole_summary_to_or_sleep() {
        let sleepy = record(
            5,
            InhibitionTargets {
                idle: true,
                suspend: true,
                ..InhibitionTargets::NONE
            },
            IdleInhibitorSource::portal("/req/1", "org.videolan.VLC"),
        );
        let held = state(vec![screen_saver_record(1), sleepy]);
        assert_eq!(hero_subtitle(&held, &[]), "2 apps preventing idle or sleep");
    }

    #[test]
    fn a_shutdown_target_counts_as_sleepy_wording_too() {
        let record = record(
            1,
            InhibitionTargets {
                idle: true,
                shutdown: true,
                ..InhibitionTargets::NONE
            },
            IdleInhibitorSource::login1(4400, 0, Login1Mode::Block),
        );
        assert_eq!(
            hero_subtitle(&state(vec![record]), &[]),
            "1 app preventing idle or sleep"
        );
    }

    #[test]
    fn manual_hold_plus_others_combines_both_clauses() {
        let held = state(vec![manual_hold_record(9), screen_saver_record(1)]);
        assert_eq!(
            hero_subtitle(&held, &[9]),
            "Awake until you turn it off · 1 app preventing idle"
        );
    }

    #[test]
    fn newly_adopted_holds_finds_every_fresh_manual_hold_shaped_record() {
        let records = vec![
            screen_saver_record(1),
            manual_hold_record(2),
            manual_hold_record(3),
        ];
        assert_eq!(newly_adopted_holds(&records, &[1]), vec![2, 3]);
        assert_eq!(
            newly_adopted_holds(&records, &[1, 2, 3]),
            Vec::<u64>::new(),
            "an id already known is not adopted a second time"
        );
        assert_eq!(
            newly_adopted_holds(&[screen_saver_record(3)], &[]),
            Vec::<u64>::new(),
            "a fresh record that is not shaped like the manual hold is never adopted"
        );
    }

    #[test]
    fn a_forged_manual_hold_shaped_record_is_never_adopted_when_it_cannot_be_released() {
        let mut forged = manual_hold_record(2);
        forged.can_release = false;
        assert_eq!(
            newly_adopted_holds(&[forged], &[]),
            Vec::<u64>::new(),
            "a record the daemon itself refuses to release is never ours, whatever its shape"
        );
    }

    #[test]
    fn existing_manual_holds_are_owned_when_the_panel_starts() {
        let records = vec![screen_saver_record(1), manual_hold_record(2)];
        assert_eq!(manual_hold_ids(&records), vec![2]);
    }

    #[test]
    fn row_label_falls_back_process_then_who_then_bus_name() {
        let mut record = screen_saver_record(1);
        assert_eq!(row_label(&record), "firefox-bin");

        record.process_name.clear();
        assert_eq!(row_label(&record), "Firefox");

        record.who.clear();
        assert_eq!(row_label(&record), ":1.42");

        record.bus_name.clear();
        assert_eq!(row_label(&record), "Unknown");
    }

    #[test]
    fn a_portal_row_is_labelled_flatpak_and_a_login1_row_carries_its_pid() {
        let now = Utc.with_ymd_and_hms(2026, 9, 18, 12, 0, 0).unwrap();
        let portal = record(
            1,
            InhibitionTargets::idle_only(),
            IdleInhibitorSource::portal("/req/1", "org.videolan.VLC"),
        );
        let login1 = record(
            2,
            InhibitionTargets::idle_only(),
            IdleInhibitorSource::login1(555, 0, Login1Mode::Block),
        );

        let entries = to_inhibitor_entries(&[portal, login1], &[], now);
        assert!(entries[0].status.ends_with("(Flatpak via portal)"));
        assert!(entries[1].status.contains("pid 555"));
    }

    #[test]
    fn the_manual_holds_own_row_carries_no_source_suffix() {
        let now = Utc.with_ymd_and_hms(2026, 9, 18, 12, 0, 0).unwrap();
        let entries = to_inhibitor_entries(&[manual_hold_record(9)], &[9], now);
        assert_eq!(entries[0].source, InhibitorSource::ManualHold);
        assert!(!entries[0].status.contains("pid"));
    }

    #[test]
    fn a_second_hold_of_our_own_is_recognised_as_ours_too() {
        let now = Utc.with_ymd_and_hms(2026, 9, 18, 12, 0, 0).unwrap();
        let entries = to_inhibitor_entries(
            &[manual_hold_record(9), manual_hold_record(10)],
            &[9, 10],
            now,
        );
        assert!(
            entries
                .iter()
                .all(|entry| entry.source == InhibitorSource::ManualHold),
            "a hold pressed while already holding must not render as a stranger"
        );
    }

    #[test]
    fn an_extremely_long_reason_never_pushes_the_source_suffix_off_the_end() {
        let now = Utc.with_ymd_and_hms(2026, 9, 18, 12, 0, 0).unwrap();
        let mut verbose = record(
            1,
            InhibitionTargets::idle_only(),
            IdleInhibitorSource::portal("/req/1", "org.videolan.VLC"),
        );
        verbose.why = "x".repeat(240);

        let entries = to_inhibitor_entries(&[verbose], &[], now);
        assert!(
            entries[0].status.ends_with("(Flatpak via portal)"),
            "a hostile why must not be able to push the source marker off the end: {}",
            entries[0].status
        );
    }

    #[test]
    fn relative_time_buckets_from_just_now_through_hours() {
        let now = Utc.with_ymd_and_hms(2026, 9, 18, 12, 0, 0).unwrap();
        assert_eq!(relative(now, now.timestamp() as u64), "just now");
        assert_eq!(
            relative(now, (now.timestamp() - 120) as u64),
            "2 minutes ago"
        );
        assert_eq!(
            relative(now, (now.timestamp() - 7_200) as u64),
            "2 hours ago"
        );
    }

    #[test]
    fn tooltip_falls_back_to_the_reason_while_the_daemon_is_down() {
        let mut down = state(vec![]);
        down.available = false;
        down.reason = Some("provider has no bus owner".to_owned());
        assert_eq!(
            tooltip(&down, &[], None).as_deref(),
            Some("provider has no bus owner")
        );
    }

    #[test]
    fn tooltip_always_reports_something_while_the_daemon_is_up() {
        assert_eq!(
            tooltip(&state(vec![]), &[], None).as_deref(),
            Some("Nothing preventing idle"),
            "the chip is always shown while the daemon is up, so its tooltip always has content"
        );
    }

    #[test]
    fn a_tooltip_format_fills_the_status_token() {
        let held = state(vec![screen_saver_record(1)]);
        assert_eq!(
            tooltip(&held, &[], Some("Idle: {status}")).as_deref(),
            Some("Idle: 1 app preventing idle")
        );
    }

    #[test]
    fn the_icon_follows_whether_anything_is_active() {
        assert_eq!(icon(false), ICON_IDLE);
        assert_eq!(icon(true), ICON_ACTIVE);
    }
}
