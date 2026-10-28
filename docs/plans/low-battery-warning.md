# Low-battery warning

| | |
| --- | --- |
| Bead | **glimpse-f2woi** (spike) — tasks are sliced from §11 and carry `Source: docs/plans/low-battery-warning.md §N` |
| Status | Final; decisions recorded in §13. |
| Rule | Every claim is tagged **measured** (run on this machine, date given), **source** (read in local code at `a397149c` as `file:line`, or in upstream at a pinned revision) or **documented** (upstream docs, URL given). |

## 1. Goal

UPower already decides when a laptop battery is Low, Critical or at Action. glimpse reads that
decision and tints an icon. After this change, a crossing into one of those levels also posts one
desktop notification, replaced in place as the level worsens and withdrawn when the battery stops
discharging. Wireless peripherals that UPower tracks warn the same way, one notification per device.
A Critical or Action card posted while the screen was locked is posted again when the user unlocks.

- **Who notifies:** `glimpse-settings-daemon` (§13 Q1), not the panel, not the lock, and no new process.
- **What decides the level:** UPower's `WarningLevel` and nothing of ours. No thresholds in glimpse config.
- **What the user sees:** an `org.freedesktop.Notifications` notification; Low expires after 30 s and
  stays in history; Critical and Action are urgency `critical` with no timeout.

```text
 today                                     after
 UPower ──► panel Battery ──► tinted icon   UPower ──► panel Battery ──► tinted icon (unchanged)
        ├─► lock  Battery ──► tinted chip          ├─► lock  Battery ──► tinted chip (unchanged)
        └─► idle (OnBattery only)                  ├─► idle (OnBattery only, unchanged)
                                                   └─► settings-daemon: Battery + LockedHint
                                                         └─► Warnings ──► Notify / CloseNotification
                                                                          ──► glimpse-notifications
```

## 2. Current state (tree at `a397149c`)

| What | Where |
| --- | --- |
| UPower wire types: `WarningLevel` {Unknown, None, Discharging, Low, Critical, Action} from codes 0–5 | `crates/glimpse-dbus/src/clients/upower.rs:277-299` |
| `DeviceKind` (mouse, keyboard, headset, … 28 kinds), `ChargeState` | `upower.rs:173-205`, `:251-261` |
| Device decode: `Model` capped at 24 chars, `TimeToEmpty` (≤ 0 becomes `None`), `WarningLevel` | `upower.rs:11`, `:365-391`, `:422-427` |
| `Battery` mirror service: `BatteryState { on_battery, display, internals, devices, profile }`; `Charge` carries `warning`, `time_to_empty`, `percentage`, `state` | `crates/glimpse-services/src/services/battery.rs:25-42`, `:87-94` |
| `display` is UPower's composite `DisplayDevice`; real internal batteries are `internals`; every other present device is a `Peripheral` | `battery.rs:370-400` |
| Peripheral name = Model, else Vendor, else NativePath, else path tail | `battery.rs:432-445` |
| Service resyncs on any UPower device `PropertiesChanged`, `DeviceAdded/Removed`, `OnBattery`, and UPower owner change; with no system bus it degrades | `battery.rs:447-498` |
| Only `Upower` events call `ctx.running()` and publish; `Publisher::set` suppresses equal states | `battery.rs:261-265`; `crates/glimpse-services/src/publisher.rs:15-23` |
| Extra peripheral sources are injected via `Dependencies { sources }`; the panel passes KDE Connect, the lock passes none | `battery.rs:165-167`; `crates/glimpse-panel/src/services.rs:173-182`; `crates/glimpse-lock/src/app.rs:1164` |
| Panel bar: `warning()` maps Low → Warning, Critical/Action → Error + attention | `crates/glimpse-panel/src/applets/power/render.rs:37`, `:392-398` |
| Peripheral rows use a hard-coded `LOW = 20` instead of `WarningLevel` | `render.rs:17`, `:205-207` |
| Lock status chip: Low → Warning, Critical/Action → Error | `crates/glimpse-lock/src/status.rs:19-27`, `:52-58` (the bead's `status.rs:100` no longer exists; the file is 65 lines) |
| Lock hosts `Battery` once at daemon start and shows only battery, notification counts and the power button | `crates/glimpse-lock/README.md:58-63` |
| **Settings daemon, startup:** `Buses::connect()` (session and system), then `start_with_buses`: actions export, `Running::build` for Geolocation, Solar and DesktopMode, the Settings1 provider acquires the name, then the pending services start | `crates/glimpse-settings-daemon/src/services.rs:22-70` (`:26`, `:40-47`, `:48-50`, `:54-56`, `:57-61`) |
| **Settings daemon, struct, reload, report and shutdown:** `SettingsServices` fields, `reconfigure` (location, mode), `report`, `shutdown` | `services.rs:12-19`, `:72-75`, `:77-81`, `:83-91` |
| **Settings daemon, main loop:** current-thread runtime; config load, `watch_config`, `Reload::Changed` → `reconfigure` + `report(Ok)`, `Rejected` → `report(Err)`, SIGTERM/SIGINT break and `shutdown`; one exit code `FAILURE` | `crates/glimpse-settings-daemon/src/main.rs:18`, `:32-78` (`:37-40`, `:41-45`, `:60-66`, `:75`); `errors.rs:3-5` |
| **Settings daemon, `Settings1`:** properties and methods read only the solar, location and mode handles; battery state never enters it | `crates/glimpse-settings-daemon/src/provider.rs:8-12`, `:25-60` |
| **Settings daemon, dependencies:** `glimpse-config`, `glimpse-dbus`, `glimpse-services`, `glimpse-utils`, `futures-util`, `tokio`, `tracing`, `zbus`; no GTK, no Wayland | `crates/glimpse-settings-daemon/Cargo.toml:8-22` |
| **Settings daemon, tests:** `start_with_buses` is the test seam; one test already starts a duplicate instance; tests use `Buses::for_session` (no system bus) | `crates/glimpse-settings-daemon/src/services/tests.rs:36-38`, `:54-56` |
| **Settings daemon, charter and unit:** README paragraph 1 says it owns `me.aresa.Glimpse.Settings`, exits on a second instance, reads `config.toml` only, reloads on file change and SIGHUP; unit is `Type=dbus`, `PartOf`/`Requisite` the session target, `Restart=on-failure`, seven sandbox options, no network or address-family restriction | `crates/glimpse-settings-daemon/README.md:3-14`; `data/systemd/glimpse-settings-daemon.service:4-23` (sandbox `:17-23`); `data/dbus-1/services/me.aresa.Glimpse.Settings.service`; `data/systemd/glimpse-session.target:6-7` |
| The daemon already uses the system bus (GeoClue) | `crates/glimpse-services/src/services/geolocation.rs:337` |
| A standalone provider hosting a `glimpse-services` service | `crates/glimpse-weather/src/services.rs:34-40` |
| `Session` service already follows `LockedHint` (`session_path`, `receive_locked_hint_changed` subscribed before the first read) but requires a `CompositorHandle`, which the daemon does not have | `crates/glimpse-services/src/services/session.rs:35-47`, `:134-161`; `crates/glimpse-dbus/src/clients/login1.rs:66-67`, `:132` |
| The lock follows `LockedHint` the same way | `crates/glimpse-lock/src/logind.rs:64-66` |
| Notification proxy: `notify(app_name, replaces_id, app_icon, summary, body, actions, hints, expire_timeout) -> u32`, `close_notification(id)`, `notification_closed` | `crates/glimpse-dbus/src/clients/freedesktop_notifications.rs:5-34` |
| Existing posters: agents (urgency hint `U8(1)`, markup-escaped, 5 s deadline) and lock (critical urgency `U8(2)`, id ignored, failures logged) | `crates/glimpse-agents/src/session.rs:240-262`; `crates/glimpse-lock/src/notify.rs:11-12`, `:43-78`, `:88-112` |
| Daemon: `replaces_id != 0` reuses the id and `fetch_max`es the counter; unknown ids are accepted | `crates/glimpse-services/src/services/notifications.rs:993-1008` |
| Daemon capabilities `actions, body, body-markup, persistence` (no `sound`); summary capped at 160; `urgency` hint is a `u8` (0 low, 2 critical, else normal) | `notifications.rs:30`, `:945-950`, `:1218-1224` |
| Popup: critical or zero-timeout never expires; `-1` falls back to `[notifications] hide-delay`; the lock gate clears popups and records received while a gate is active are never replayed (they stay in history) | `crates/glimpse-notifications/README.md:30-32`, `:45-46` |
| Hostile-text helpers: `glimpse_utils::clean(text, cap)`; body escaping `escaped_body` + `markup::sanitize_body` | `crates/glimpse-utils/src/text.rs:9`; `crates/glimpse-shot/src/notify.rs:419-425`; `crates/glimpse-utils/src/markup.rs:25` |
| Locale: providers call `init_locale()` (weather) and UI binaries call `init_translations(config.regional.language())`; the settings daemon calls neither; the docs say a provider never gets `init_translations` because its output is a journal | `crates/glimpse-weather/src/main.rs:26`; `crates/glimpse-agents/src/main.rs:50`; `crates/glimpse-utils/src/i18n.rs:43-61`; `crates/glimpse-utils/README.md:38-40`; `docs/translations.md:3-6`, `:16-19` |
| Resume signal precedent: `PrepareForSleep` | `crates/glimpse-dbus/src/clients/login1.rs:54-55`; `crates/glimpse-services/src/services/solar.rs:166` |
| Duration text `clock()` is private to the panel applet, translated msgids `{minutes} m`, `{hours} h`, `{hours} h {minutes} m`, `Less than a minute` | `render.rs:368-379` |
| Config declaration shape: one struct per table, `#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]`, doc comment per field, hand-written `Default`, registered in `mod.rs` (mod list, `pub use`, `Config` field) | `crates/glimpse-config/src/schema/idle.rs:4-24`; `schema/mod.rs:1-22`, `:48`, `:64`, `:68-94` |
| The panel already owns `[applets.power]` (`panel.toml`: tooltip, indicator style, label); the new table is `[power]` in `config.toml` | `crates/glimpse-config/src/schema/applets.rs:291-321` |

Nothing in the tree posts a notification for battery state: no crate under `crates/` combines
`WarningLevel` with `FreedesktopNotificationsProxy`.

## 3. Facts

| Fact | Tag |
| --- | --- |
| upower and upowerd 1.91.5 (`upower --version`) | measured 2026-10-26 |
| `DisplayDevice`: `Type 2`, `State 5` (PendingCharge), `Percentage 80`, `WarningLevel 1` (None), `TimeToEmpty 0`; every property `emits-change` (`busctl --system introspect org.freedesktop.UPower /org/freedesktop/UPower/devices/DisplayDevice`) | measured 2026-10-26 |
| Devices here: `battery_BAT1` (ASUS A32-K55, `PowerSupply true`), `line_power_ACAD`, `DisplayDevice`; no peripheral exposed (`upower -e`) | measured 2026-10-26 |
| `/etc/UPower/UPower.conf`: `UsePercentageForPolicy=true`, `PercentageLow=20`, `PercentageCritical=5`, `PercentageAction=2`, `TimeLow=1200`, `TimeCritical=300`, `TimeAction=120`, `AllowRiskyCriticalPowerAction=false`, `ExpectBatteryRecalibration=false`, `CriticalPowerAction=Auto` | measured 2026-10-26 |
| `GetCriticalAction()` returns `"Sleep"` (so `Auto` resolves to `Sleep`) | measured 2026-10-26 (`busctl --system call org.freedesktop.UPower /org/freedesktop/UPower org.freedesktop.UPower GetCriticalAction`) |
| `Auto` → `Sleep`; `Suspend`/`Ignore` are rewritten to `HybridSleep` unless `AllowRiskyCriticalPowerAction` | source upower `src/linux/up-backend.c:642-651` @ `233615a1` |
| Display-device level (the only level that triggers the action): `None` unless kind is Battery/UPS; on AC with not every battery discharging → `None`; otherwise `compute_warning_level` | source `src/up-daemon.c:423-441` @ `233615a1` |
| `compute_warning_level`: not `Discharging` → `None`; with `UsePercentageForPolicy=false` and `TimeToEmpty > 0` it compares time against `TimeLow/Critical/Action`, else percentage against `PercentageLow/Critical/Action` (`>low` → none/UPS-discharging, `>critical` → Low, `>action` → Critical, else Action); `ExpectBatteryRecalibration` on AC → `None` | source `up-daemon.c:819-887` @ `233615a1` |
| So `WarningLevel` already folds in percentages, times, `UsePercentageForPolicy`, AC presence and multi-battery state; a client needs no threshold of its own | source (above) |
| Several batteries: composite percentage is energy-weighted (`100 * energy / energy_full`), else averaged; composite state is Discharging if any is, and `state_all_discharging` gates the AC suppression | source `up-daemon.c:278-290` @ `233615a1` |
| On `Action`: UPower takes an inhibitor lock, waits `UP_DAEMON_ACTION_DELAY = 20` s, releases the lock and runs the action; any level change before then cancels the timer | source `up-daemon.c:93`, `:774-815`, `:754-766` @ `233615a1` |
| Per-device level for peripherals: mouse/keyboard/touchpad → ≤ 10 % Low, ≤ 5 % Critical (never Action); every other kind uses the same `compute_warning_level` and so can reach `Action`, which triggers nothing for a peripheral; a coarse `BatteryLevel` is trusted unless it is `Critical` | source `up-daemon.c:837-846`; `up-device.c:101-117` @ `233615a1` |
| UPower 1.91.5 exposes BlueZ `Battery1` devices as UPower devices (kind from Appearance/class); `upowerd` references `org.bluez.Battery1` | source `src/linux/up-device-bluez.c:31-148` @ `233615a1`; measured 2026-10-26 (`strings /usr/lib/upowerd`) |
| BlueZ devices here (Pixel 10 Pro, MX KEYS S, WH-1000XM4) are all `Connected false`, so none is a UPower device now | measured 2026-10-26 |
| KDE Connect phone battery: `charge 35`, `iconName "battery-low-symbolic"`, `isCharging false`, `hasBattery true`; no warning level, no thresholds | measured 2026-10-26 (`busctl --user introspect org.kde.kdeconnect …/battery`) |
| Adwaita ships `battery-low-symbolic`, `battery-caution-symbolic`, `battery-action-symbolic` | measured 2026-10-26 (`/usr/share/icons/Adwaita/symbolic/status`) |
| The running `glimpse-settings-daemon` unit has `PrivateNetwork=no`, `RestrictAddressFamilies=` unset, `NoNewPrivileges=yes`: the system-bus socket is reachable (`systemctl --user show glimpse-settings-daemon -p PrivateNetwork -p RestrictAddressFamilies -p NoNewPrivileges`) | measured 2026-10-26 |
| `LockedHint` of the login session is readable by this user without a prompt (`busctl --system get-property org.freedesktop.login1 /org/freedesktop/login1/session/auto org.freedesktop.login1.Session LockedHint` → `b false`) | measured 2026-10-26 |
| Session units running here include `glimpse-idle` (UPower reader), `glimpse-settings-daemon` (pid 4498), `glimpse-notifications` (owns `org.freedesktop.Notifications`), `glimpse-panel`, `glimpse-lock`; no battery notifier | measured 2026-10-26 (`systemctl --user list-units 'glimpse-*'`, `busctl --user list`) |
| RSS: `glimpse-settings-daemon` 13 MB, `glimpse-idle` 8 MB, `glimpse-sunset` 7.5 MB, `glimpse-weather` 24 MB; hosting adds a tokio task and one `Battery` service graph instead of a new process | measured 2026-10-26 (`ps`) |
| **gsd-power** (GNOME) follows UPower's per-device `WarningLevel`: Low → "Low Battery / N% battery remaining", transient, 30 s, plays a sound; Critical → "Battery Almost Empty / Connect power now", never expires, sound loop; Action → "Battery is Empty / about to hibernate\|shutdown", never expires; `None` closes the one shared slot. No time-left text. | source `plugins/power/gsd-power-manager.c:790-1015`, `:1027-1062` @ gnome-settings-daemon `306e33d1` |
| gsd peripherals: per-kind titles ("Mouse battery low", …), debounced per serial so a device warns only when its level rises and never again after it recovers; kinds modem/network/video/wearable/toy skipped; any other kind falls through to the last table row (Pen) | source `gsd-power-manager.c:477-506`, `:509-640`, `:518-523`, `:828`, `:901` @ `306e33d1` |
| **COSMIC does notify**, contrary to the bead: `cosmic-settings-daemon` (not the battery applet) watches the display device's `Percentage` and posts "Battery Low" at < 20 % (normal, 5 s) and "Battery Critical" at < 10 % (critical, 30 s) with its own thresholds, plus a nag sound and a plug/unplug sound; it ignores `WarningLevel`. The applet has no notification code. | source `cosmic-settings-daemon/src/battery.rs:104-215` @ `655b14fb`; `cosmic-applets/cosmic-applet-battery/src/*.rs` contain no `notif` |

## 4. Decisions

| # | Decision | Why |
| --- | --- | --- |
| D1 | **Levels are UPower's.** The notifier reacts to `WarningLevel` {Low, Critical, Action}; `Unknown`, `None` and `Discharging` post nothing. | It already honors every `UPower.conf` key and the AC rules (§3), and `Action` is the exact moment UPower starts its 20 s countdown. Our own percentages would disagree with `CriticalPowerAction`. COSMIC's own thresholds are the counter-example. |
| D2 | **System warning follows the composite `DisplayDevice` only** (`BatteryState.display`). Individual internal batteries (`internals`) never warn. | UPower acts on the composite level only (`up-daemon.c:774-815`); a per-pack warning on a two-battery laptop would fire while the machine is fine. |
| D3 | **One system slot, replaced in place.** Low → Critical → Action all reuse the id returned by the first `Notify` (`replaces_id`). Levels only rise: a lower non-`None` level posts nothing. A `None` level re-arms (the next crossing posts again) but does not close the notification while the battery is still `Discharging`. | A gauge that wobbles across a boundary must not stack or flicker; the daemon accepts a replace of an unknown id (`notifications.rs:993-1008`). |
| D4 | **Withdraw** (`CloseNotification`) when the level is `None` and the display device is not `Discharging`, or the display device disappears. | "Withdrawn when AC returns." UPower itself reports `None` on AC unless every battery discharges (`up-daemon.c:423-441`). |
| D5 | **Urgency and lifetime.** Low: urgency normal (`U8(1)`), `expire_timeout` 30 000 ms, then history. Critical and Action: urgency critical (`U8(2)`), `expire_timeout` 0. Icons: Low `battery-low-symbolic`; Critical and Action `battery-caution-symbolic`. Hints: `urgency` only. | The daemon never times out critical or zero-timeout popups (`README.md:45-46`). 30 s matches gsd for Low (`gsd-power-manager.c:76`, `:846`); `-1` would be the 4 s `hide-delay`, easy to miss. `transient` is not set, so the card stays in history. User decision Q4. |
| D6 | **Text** (§7.3): Low and Critical say the percentage and, when `Charge.time_to_empty` is `Some`, the time left through the shared `clock()`; Action names the action UPower will take, read with `GetCriticalAction()` at the crossing, and the 20 s delay. Body carries no device-derived text. | `TimeToEmpty` is `None` unless UPower knows it (`upower.rs:422-427`); the user asked for the time when known. The action string differs per machine (measured `Sleep`). |
| D7 | **Peripherals** (UPower devices that are not the display device, not an internal supply): each has its own slot keyed by UPower object path, same rules as D3/D4, level capped at Critical, `Action` treated as `Critical`. Withdrawn when the device disappears. Summary names the device by its already-capped `Model` through `clean(name, 24)`, falling back to a translated kind label. Default on. | UPower computes the per-device level and already merges BlueZ `Battery1` (§3), so BlueZ needs no code of ours. gsd's never-warn-again-after-recovery is dropped: the re-arm of D3 is one notification per discharge. User decision Q2. |
| D8 | **Kinds that warn:** Mouse, Keyboard, Touchpad, GamingInput, Pen, Tablet, Pda, Phone, MediaPlayer, Computer, Headset, Headphones, Speakers, OtherAudio, RemoteControl. All others (Unknown, LinePower, Monitor, Modem, Network, Video, Wearable, Toy, Printer, Scanner, Camera, BluetoothGeneric, Battery/UPS without `power-supply`) do not. | gsd's skip list (`:518-523`) plus kinds without a clear "battery low" meaning. gsd's fall-through to a *Pen* title for unlisted kinds (`:828`) is a bug we do not copy. |
| D9 | **KDE Connect phones never warn.** No warning level exists on the wire, and the phone warns itself. | The only signal is an icon name (`kdeconnect.rs:41-46`); we would be inventing thresholds, which D1 forbids. |
| D10 | **UPS:** a UPS display device (kind UPS) warns for Low/Critical/Action like a battery; UPower's `Discharging` level (UPS on battery, 2) posts nothing. | `Discharging` is a UPS-only status, not a warning (`up-daemon.c:847`); a desktop on a UPS is out of the bead's scope, so no extra text is written for it. |
| D11 | **Owner is `glimpse-settings-daemon`.** It hosts the existing `Battery` mirror with `Dependencies::default()` (as the lock does, `app.rs:1164`, and as weather hosts its service, `weather/src/services.rs:34-40`) inside a new module `power`; the decision logic is a pure state machine in `power/warnings.rs`; the mirror stays policy-free. The notifier is one `tokio::spawn`ed task owned by `SettingsServices`, not a `glimpse-services` `Service`. | User decision Q1. Mirror services add no policy (`docs/handbook.md:86-92`); reusing `Battery` reuses decode, caps, UPower-owner-change resync and tests. The daemon already runs with the system bus (`geolocation.rs:337`), reloads `config.toml` and exits on a second instance (README:3-14). A task panic is caught by the runtime and cannot stop the daemon (`main.rs:18` current-thread runtime; no `panic = "abort"`, `handbook.md:183-184`). Cost: `Battery` also subscribes to power-profiles-daemon, which the notifier ignores. |
| D12 | **Config:** table `[power]` in `config.toml` with `warnings` and `peripheral-warnings` (§6). No threshold, sound or text fields. | Thresholds belong to `UPower.conf` (D1). `config.toml` holds one table per service (`handbook.md:65-67`) and the daemon reads it already (`main.rs:38`). |
| D13 | **Startup.** The first state after start is treated like a crossing: a laptop that boots at 4 % posts Critical once. The mirror's initial default state (`display: None`) posts nothing. | A user who logs in at 4 % must be told; `display: None` is also a desktop without a battery, which must stay silent. |
| D14 | **Resume.** No special code. The state machine is edge-triggered on the level, so a resume that leaves the level unchanged posts nothing, and one that crosses a level posts it. `PrepareForSleep` is not subscribed. | UPower re-evaluates after resume and publishes `WarningLevel`; the mirror resyncs on `PropertiesChanged` (`battery.rs:447-498`). |
| D15 | **Failure.** A `Notify` that fails or times out (`glimpse_dbus::DEADLINE`, `clients/mod.rs:12`) leaves the level unposted; it is retried on the next state change (UPower publishes on every percentage step). The failure is logged, as `glimpse-lock` does (`notify.rs:105-108`). It is not itself a notification: the notification service is the thing that failed. | "A failed command is a notification" has no channel here. Retry-on-change gives the late-starting-daemon case (daemon up before `glimpse-notifications`) without a retry loop. |
| D16 | **Locked.** Posting continues. While the session is locked the notification daemon gates popups and keeps the card in history (`notifications/README.md:30-32`); the lock chip counts it by urgency (`lock/README.md:58-63`) and shows the battery severity (`status.rs:19-27`). A Critical or Action card posted while locked is marked hidden and **posted again, replacing itself, when `LockedHint` turns false** with fresh percentage and time (§7.1 rule 8). Low is not re-posted. | User decision Q3. The daemon never replays records received behind a gate, so without the re-post the user who unlocks at 4 % sees nothing. The lock must not host a notifier (`AGENTS.md` hard rules) and two hosts of `Battery` would double-post. |
| D17 | **Desktop without a battery or without UPower, and any failure of the notifier, must not exit or degrade the daemon.** `display` is `None` → no slot, no notification. UPower absent or no system bus → `Battery` degrades (`battery.rs:447-452`) and follows its owner when it appears (`:472-481`); nothing in the daemon reads `Battery` health: `Settings1` reads only solar, location and mode handles (`provider.rs:25-60`) and `report` carries only config status (`services.rs:77-81`). `power::Runtime::start` is infallible (it returns no `Result`), so it adds no new `?` to `start_with_buses` and no new exit path (`errors.rs:3-5`). | A restart of the settings daemon drops `Settings1` for weather, sunset and every mode consumer; a battery quirk must not cost that. Same silent posture as idle ("assuming AC power", `idle/src/services.rs:84-89`). |
| D18 | **No sound, no actions, no `desktop-entry` hint, no `NotificationClosed` tracking.** | The notification daemon has no `sound` capability (`notifications.rs:945-950`). A user-dismissed card is not re-posted at the same level, which the level-only-rises rule already guarantees (the unlock re-post of D16 is the one deliberate exception, and only for hidden cards). |
| D19 | **`clock()` moves to `glimpse-utils`** (`duration.rs`, `pub fn clock(seconds: u32) -> String`) and the panel calls it; msgids are unchanged. | Two binaries need it (`handbook.md:215-218` spirit; `AGENTS.md` "never hand-roll"); `glimpse-utils` owns the gettext binding (`docs/translations.md:3-4`). |
| D20 | **Identity:** `app_name = "glimpse-power"`, not `"glimpse-settings-daemon"`. The notification daemon groups by `desktop-entry`, else by the executable's desktop file, else by the name the sender gives (`notifications/README.md:21-24`). | The name is what a user sees on the card and writes into `[notifications] suppress`, which matches the sender (`schema/notifications.rs:10-12`); it must name the feature, not the host process, and stay valid if the feature is ever re-hosted. Other posters use their binary name because the binary is the feature (`session.rs:246-248`, `notify.rs:64-67`); here it is not. |
| D21 | **No new bus name, interface or unit.** The feature adds no `Settings1` member. A second daemon instance is stopped by the existing name acquisition at `services.rs:54-56`, which runs before the power runtime starts, so it posts nothing (README:5-6; duplicate-instance test at `services/tests.rs:54-56`). | One owner of the feature means one owner of the name; the existing `Exported::start` guard (`provider.rs` via `own_name`, `dbus.rs:68-73`) already covers it. |
| D22 | **Locale:** the daemon calls `init_translations(config.regional.language())` after the config load (as `glimpse-agents/src/main.rs:50`), instead of nothing. The rule "never give a provider `init_translations`" becomes "a provider that posts user-facing text calls it; one whose output is a journal calls `init_locale()`". A language change applies on restart. | The notification text is user-facing and translated (`handbook.md:162-166`); without the catalog the daemon posts English. `init_translations` includes `init_locale` (`i18n.rs:61`), so the `LC_MEASUREMENT` fix weather needs comes with it. |
| D23 | **Lock state source:** a small follower in `power/locked.rs` copying the pattern at `session.rs:134-161` (`session_path`, `receive_locked_hint_changed` subscribed before `locked_hint`), publishing a `watch::Receiver<bool>`. The existing `Session` service is not hosted. An unreadable `LockedHint` publishes `false` and logs once. | `Session` requires a `CompositorHandle` (`session.rs:45-47`), which the daemon does not run; hosting it drags a compositor mirror into the settings daemon. `false` on failure means no card is ever marked hidden, so no spurious re-post. (The `Session` service itself treats failure as locked, `session.rs:109-112`, because it gates popups; here the safe side is the opposite.) |

## 5. Architecture

### 5.1 Crates and files

| Crate | Role | Contents |
| --- | --- | --- |
| `glimpse-settings-daemon` (existing bin) | owner | `src/power.rs` (`Runtime`: hosts `Running<Battery>`, owns the task and the config channel); `src/power/warnings.rs` + `src/power/warnings/tests.rs` (state machine, §7.1); `src/power/notify.rs` + `src/power/notify/tests.rs` (dispatch and text, §7.2–7.3); `src/power/locked.rs` (§D23); wiring in `services.rs` and `main.rs` (below); tests in `src/services/tests.rs` |
| `glimpse-config` | `[power]` table | `schema/power.rs`, registered per `schema/mod.rs:1-22,48,64,68-94`; `data/config.commented.toml` and `data/config.schema.json` regenerated (`just gen-config-commented config`, `justfile`) |
| `glimpse-utils` | shared duration text | `clock()` moved from `render.rs:368-379` |
| `glimpse-dbus` | one proxy method | `GetCriticalAction` on `UPowerProxy` (`upower.rs:20-29`); a `testing`-feature constructor so a test can hand `Buses` a system and a session connection together (today `for_session` and `for_system` give one each, `dbus.rs:42`) |
| `glimpse-services` | unchanged | `Battery`, `BatteryState`, `Running` are consumed as they are |

Wiring inside the daemon:
- `services.rs:12-19`: `SettingsServices` gains `power: power::Runtime`.
- `services.rs:29-70`: after the pending services start (`:57-61`) and before `Ok(Self { … })`, `power::Runtime::start(buses.clone(), session.clone(), document).await`; `buses` is cloned before the final move at `:50`. It is placed after the name acquisition at `:54-56` on purpose (D21).
- `services.rs:72-75`: `reconfigure` also calls `self.power.reconfigure(document)`.
- `services.rs:83-91`: `shutdown` calls `self.power.shutdown().await` first, so no post races the teardown.
- `main.rs:37-40`: `init_translations(config.regional.language())` after the config load (D22). No other change: the reload arm at `:60-66` already calls `reconfigure`.
- `Cargo.toml:8-22`: no new dependency (`glimpse-services`, `glimpse-dbus`, `glimpse-utils`, `tokio`, `zbus`, `futures-util` are present).
- `README.md:3-6` charter gets the sentence in §8; it replaces nothing and adds one sentence.

Dependency order stays one-way: `glimpse-settings-daemon → glimpse-services → glimpse-dbus`. No Wayland or GTK dependency (the agents' `glib::markup_escape_text`, `session.rs:242`, is not used; the body is escaped with the `escaped_body` recipe, `glimpse-shot/src/notify.rs:419-425`, which moves next to `markup::sanitize_body` if a second caller needs it).

### 5.2 Who owns what

| State | Owner | Others |
| --- | --- | --- |
| Warning level, time to empty, kind, percentage | UPower | `Battery` mirror (panel, lock, `glimpse-settings-daemon`) |
| Session lock state | logind | panel/notifications `Session` service, lock, `power/locked.rs` |
| Thresholds, `CriticalPowerAction` | `/etc/UPower/UPower.conf` (UPower) | read-only via `GetCriticalAction()` |
| Whether a level has been announced, hidden flag, notification ids per subject | `power::Runtime` in `glimpse-settings-daemon`, in memory, lost on restart | nobody |
| Notification card, popup lifetime, history, lock gate | `glimpse-notifications` | — |
| `[power]` switches | `config.toml`, read by `glimpse-settings-daemon` | validated by every binary that loads `config.toml` |
| Bar/lock icon severity | panel / lock, from the same `WarningLevel` | unchanged |
| Location, solar phase, desktop mode, `Settings1` | `glimpse-settings-daemon`, unchanged | unaffected by this feature (D17) |

### 5.3 How the pieces connect

UPower `PropertiesChanged` → `Battery` service resync (`battery.rs:447-498`) → `Publisher::set` publishes a new `BatteryState` only when it differs. Separately logind `LockedHint` → `power/locked.rs` → `watch::Receiver<bool>`. `power::Runtime`'s task selects over three inputs: the `BatteryState` receiver calls `Warnings::observe(&state)`; the lock receiver calls `Warnings::set_locked(locked)`; the config receiver calls `Warnings::reconfigure(config)`. Each returns `Vec<Step>` (`Post`/`Withdraw`). `power/notify.rs` turns a `Post` into one `FreedesktopNotificationsProxy::notify(...)` call on the **session** connection (for `Action`, first one `GetCriticalAction()` on the system connection), and on success calls `Warnings::posted(&notice, id)`; a `Withdraw` becomes `close_notification(id)` → `glimpse-notifications` renders or gates it. The daemon's reload path reaches the task as: `main.rs:60-62` `Reload::Changed` → `SettingsServices::reconfigure` (`services.rs:72-75`) → `Runtime::reconfigure` → `watch::Sender<PowerConfig>` → the task. A rejected reload never reaches it (`main.rs:66`), so the last valid `[power]` stays.

## 6. Config

`crates/glimpse-config/src/schema/power.rs`, same attributes and doc-comment style as
`schema/idle.rs:4-24`; exported as `PowerConfig` (like `WeatherConfig`, `schema/mod.rs:64`) because
`Power` is already the applet's alias (`mod.rs:37`); added to `Config` as `pub power: PowerConfig`.

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Power {
    /// Whether you get a notification when the battery runs low, critically low, or is about to
    /// trigger UPower's critical action. The levels come from `/etc/UPower/UPower.conf`.
    pub warnings: bool,
    /// Whether wireless mice, keyboards, headsets and other devices UPower tracks warn too, one
    /// notification per device.
    pub peripheral_warnings: bool,
}

impl Default for Power {
    fn default() -> Self {
        Self {
            warnings: true,
            peripheral_warnings: true,
        }
    }
}
```

```toml
[power]
warnings = true
peripheral-warnings = true
```

Owner: `glimpse-settings-daemon`, which reads `config.toml` only (README:11) and reloads on file
change and `SIGHUP` (README:11-13). `[applets.power]` in `panel.toml` is a different document and is
untouched. `warnings = false` withdraws any open system card and suppresses peripheral cards too (a
switch named `warnings` that leaves peripherals chattering would surprise); `peripheral-warnings` is
meaningful only while `warnings` is true. Validation: none beyond types.

## 7. Internal shapes

### 7.1 State machine (`glimpse-settings-daemon/src/power/warnings.rs`)

```rust
use std::collections::BTreeMap;

use glimpse_config::PowerConfig;
use glimpse_dbus::upower::{DeviceKind, WarningLevel};
use glimpse_services::BatteryState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Low,
    Critical,
    Action,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Subject {
    System,
    Peripheral(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub subject: Subject,
    pub level: Level,
    pub percentage: u8,
    pub seconds_left: Option<u32>,
    pub device: Option<String>,
    pub kind: Option<DeviceKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Post { notice: Notice, replaces: Option<u32> },
    Withdraw { subject: Subject, id: u32 },
}

#[derive(Debug, Default)]
struct Slot {
    announced: Option<Level>,
    id: Option<u32>,
    hidden: bool,
}

pub struct Warnings {
    config: PowerConfig,
    slots: BTreeMap<Subject, Slot>,
    last: BatteryState,
    locked: bool,
}

impl Warnings {
    pub fn new(config: PowerConfig) -> Self;
    pub fn observe(&mut self, state: &BatteryState) -> Vec<Step>;
    pub fn reconfigure(&mut self, config: PowerConfig) -> Vec<Step>;
    pub fn set_locked(&mut self, locked: bool) -> Vec<Step>;
    pub fn posted(&mut self, notice: &Notice, id: u32);
}

pub fn level_of(warning: WarningLevel) -> Option<Level>;
pub fn warns(kind: DeviceKind) -> bool;
```

Rules (the whole policy):

1. `level_of`: `Low → Low`, `Critical → Critical`, `Action → Action`, everything else `None`. For
   `Subject::Peripheral`, `Action` is lowered to `Critical` (D7).
2. For a subject with level `Some(l)`: if `Some(l) > slot.announced` emit `Post { replaces: slot.id }`.
   `announced` is set only by `posted()` (D15). A lower or equal level emits nothing.
3. For a subject with level `None`: `slot.announced = None`. If the subject's `ChargeState` is not
   `Discharging` or the subject is gone, and `slot.id` is `Some`, emit `Withdraw`, clear the id and
   `hidden`.
4. `warnings == false`: every level is treated as `None` and `state` as not discharging
   (withdraws everything). `peripheral_warnings == false`: peripheral subjects are treated the same.
5. A peripheral subject exists for each `BatteryState.devices` entry with `warns(kind)`, keyed by
   `Peripheral.path`; `Peripheral.charge.warning` and `.state` are its level and state.
6. `Notice.device` is `clean(peripheral.name, 24)` (the name is already capped at 24 in decode,
   `upower.rs:11,369`; `clean` removes control and bidi characters, `glimpse-utils/src/text.rs:9`).
7. `posted(notice, id)` records `announced = notice.level`, `id = Some(id)` and
   `hidden = self.locked` for `notice.subject`. `observe` stores each state in `last`.
8. `set_locked(locked)` stores the flag. On the transition `true → false` it emits, for every slot
   with `hidden`, `announced >= Some(Critical)` and `id` set, a `Post { replaces: id }` built from
   `last` (fresh percentage and time) provided that subject's current level is still `Critical` or
   `Action` and it still discharges; `posted()` then clears `hidden` because `locked` is `false`.
   Slots announced as `Low` are never re-posted. `false → false` and `→ true` emit nothing.

### 7.2 Dispatch (`power/notify.rs`) and runtime (`power.rs`)

```rust
pub async fn dispatch(
    session: &FreedesktopNotificationsProxy<'_>,
    system: Option<&UPowerProxy<'_>>,
    step: Step,
) -> Result<Option<u32>, String>;

pub struct Runtime {
    battery: Running<Battery>,
    config: watch::Sender<PowerConfig>,
    task: JoinHandle<()>,
}

impl Runtime {
    pub async fn start(buses: Buses, session: Connection, document: &Config) -> Self;
    pub fn reconfigure(&self, document: &Config);
    pub async fn shutdown(mut self);
}
```

`Post` → `notify("glimpse-power", replaces.unwrap_or(0), icon, &summary, &body, &[], hints, expire)`
with hints `{ "urgency": Value::U8(1 | 2) }`, `expire = 30_000` for Low and `0` otherwise (D5). The
whole call runs under `tokio::time::timeout(glimpse_dbus::DEADLINE, …)`; error strings follow
`glimpse-lock/src/notify.rs:74-77`. `Withdraw` → `close_notification(id)`. `Runtime::start` builds
the notifications proxy on the session connection it is given, spawns `Running::<Battery>` with
`Dependencies::default()`, starts the `locked` follower on the system bus, and spawns the task; it
returns no `Result` (D17). `shutdown` aborts the task and stops the `Battery` service, the same
pattern as `services.rs:83-91`.

### 7.3 Text (all `gettext`, double-quoted msgids, named placeholders filled with `.replace`)

| Level | Summary | Body (time known / unknown) | Icon |
| --- | --- | --- | --- |
| System Low | `Battery low` | `{percentage}% left, about {remaining} remaining` / `{percentage}% left` | `battery-low-symbolic` |
| System Critical | `Battery critically low` | `{percentage}% left, about {remaining} remaining. Plug in now.` / `{percentage}% left. Plug in now.` | `battery-caution-symbolic` |
| System Action, action `Sleep` or `HybridSleep` | `Battery empty` | `The computer will suspend in about 20 seconds. Plug in now.` | `battery-caution-symbolic` |
| System Action, `Hibernate` | `Battery empty` | `The computer will hibernate in about 20 seconds. Plug in now.` | same |
| System Action, `PowerOff` | `Battery empty` | `The computer will shut down in about 20 seconds. Plug in now.` | same |
| System Action, any other or failed `GetCriticalAction` | `Battery empty` | `The computer is about to protect your data. Plug in now.` | same |
| Peripheral Low | `{device} battery low` (no model: `{kind} battery low`, one msgid per kind in D8) | `{percentage}% left` | `battery-low-symbolic` |
| Peripheral Critical | same summary | `{percentage}% left. It will stop working soon.` | `battery-caution-symbolic` |

`{remaining}` is `glimpse_utils::clock(seconds)` (D19). Bodies contain no device-derived text; the
only free text is the summary, plain text capped by `clean(…, 24)` plus the notification daemon's
160-char cap (`notifications.rs:30`). Body goes through the escaping recipe regardless, because the
daemon advertises `body-markup` (`notifications.rs:945-950`). "About 20 seconds" is
`UP_DAEMON_ACTION_DELAY` (§3), a UPower constant, not configuration.

Visible surface (ASCII, no compiled preview; the card is the existing `NotificationCard`):

```text
 Low (normal, 30 s)                 Critical (critical, sticky)           Action (critical, sticky)
 ┌────────────────────────────┐    ┌────────────────────────────────┐    ┌──────────────────────────────────┐
 │ ▯ Battery low              │    │ ▯ Battery critically low       │    │ ▯ Battery empty                  │
 │   18% left, about 24 m     │    │   4% left, about 6 m remaining.│    │   The computer will suspend in   │
 │   remaining                │    │   Plug in now.                 │    │   about 20 seconds. Plug in now. │
 └────────────────────────────┘    └────────────────────────────────┘    └──────────────────────────────────┘
```

## 8. systemd, packaging and docs

No new unit, D-Bus service file, binary or package entry: `data/systemd/glimpse-settings-daemon.service`,
`data/dbus-1/services/me.aresa.Glimpse.Settings.service`, `glimpse-session.target:6-7`, `justfile:10`,
`scripts/check-units.sh:36`, `crates/glimpse-package/` and `glimpse-lock.service` are untouched
(`handbook.md:261-264`).

**Sandbox check.** The unit's options are `NoNewPrivileges`, `ProtectKernelTunables`,
`ProtectKernelModules`, `ProtectControlGroups`, `LockPersonality`, `RestrictRealtime`,
`RestrictSUIDSGID` (`glimpse-settings-daemon.service:17-23`). None restricts sockets: measured
`PrivateNetwork=no`, `RestrictAddressFamilies` unset (§3), and the daemon already talks to GeoClue on
the system bus (`geolocation.rs:337`). UPower access is ordinary system-bus method calls and property
reads; `LockedHint` read as this user was measured to work (§3). Notifications go over the session
bus the daemon already holds (`services.rs:35-39`). So the unit needs no change.

**Docs.**
- `crates/glimpse-settings-daemon/README.md`: add one sentence to paragraph 1 (after line 6), present
  tense, replacing nothing: "It also posts battery warnings: it follows UPower's warning level through the
  `Battery` mirror and the session's lock state, posts Low, Critical and Action as notifications and
  re-posts a Critical or Action card that appeared while locked; it exports no D-Bus API for this, and a
  failure of the warning path never degrades location, solar phase or desktop mode."
- `crates/glimpse-utils/README.md:38-40` and `docs/translations.md:3-6,16-19`: replace "Never give a
  provider `init_translations`" with the D22 rule; `docs/translations.md:5-6` drops "The daemon … do
  not" for the settings daemon (the paragraph keeps its length).
- `website/config/services/power.md` (new page, like `website/config/services/idle.md`) and one phrase in the
  `glimpse-settings-daemon.service` row of `website/developers/systemd.md:21`.

## 9. Flows

**Discharge (system).** AC unplugged → UPower `State Discharging` → percentage falls through 20 % →
UPower sets `WarningLevel 3` on `DisplayDevice` → `Battery` publishes → `observe` emits
`Post{Low, replaces: None}` → `Notify(…, 0, …)` returns id 7 → `posted(&notice, 7)`. At 5 %:
`WarningLevel 4` → `Post{Critical, replaces: Some(7)}` → same card becomes sticky and critical. At
2 %: `WarningLevel 5` → one `GetCriticalAction()` → `Post{Action, Some(7)}`; UPower starts its 20 s
timer and takes the action unless the level changes.

**AC returns.** `State Charging` → UPower `WarningLevel 1` → `observe` sees `None` and state not
`Discharging` → `Withdraw{System, 7}` → `CloseNotification(7)`; `announced` is `None`, so the next
discharge posts Low again.

**Gauge wobble.** Discharging at 20.3 % → 20.6 % across the boundary → level `None` while still
`Discharging` → `announced = None`, card stays → back below → `Post{Low, Some(7)}` replaces in place.

**Startup low.** Login at 4 % on battery → first `BatteryState` has `warning Critical` →
`Post{Critical, None}`. Desktop without battery: `display` is `None`, nothing happens (D13, D17).

**Resume.** Lid opens → UPower re-reads → if the level is unchanged, `PropertiesChanged` carries no new
level and nothing is posted; if sleep drained across a level, the new level posts (D14).

**Peripheral.** MX Keys connects over BlueZ → UPower adds a Keyboard device → falls to 10 % →
per-device `WarningLevel 3` → `Post{Peripheral(path), Low}` with summary from the model → charges →
state `Charging` → level `None` → `Withdraw`. Disconnect → device gone → `Withdraw`.

**Notification daemon missing.** `Notify` times out (5 s, in the notifier task, never in a service
handler) → logged → `announced` stays `None` → the next percentage step publishes a new state →
`observe` emits the same `Post` again. `Settings1` keeps answering throughout.

**Locked.** Session locks → `locked` receiver true → `set_locked(true)`. Battery crosses Critical →
`Post` → id 9 → `posted()` marks it `hidden`; the daemon gates the popup, the card is in history, the
lock chip counts it. Unlock → `LockedHint` false → `set_locked(false)` → hidden Critical slot, level
still Critical → `Post{Critical, replaces: Some(9)}` with the current percentage → the popup appears on
the desktop. A Low posted while locked, or a Critical withdrawn by AC before unlock, posts nothing on
unlock.

**Reload.** `[power] warnings = false` saved → `watch_config` → `Reload::Changed` →
`SettingsServices::reconfigure` → `Runtime::reconfigure` → task → `Warnings::reconfigure` → `Withdraw`
for every open slot → `warnings = true` → `observe(last)` posts the current level if one holds. A
rejected file keeps the previous `[power]` (`main.rs:66`).

**Second instance.** Another `glimpse-settings-daemon` starts → name acquisition fails at
`services.rs:54-56` → `start` returns an error → exit `FAILURE` before `power::Runtime::start` runs →
no post, no second `Battery` graph.

## 10. Acceptance criteria

```
AC-1  As a laptop user on battery, I want a notification when UPower says the battery is Low, so that I
      plug in before it matters.
      GIVEN a fake UPower display device Discharging at WarningLevel None, then 3 WHEN the state is
      published THEN exactly one Notify (app_name "glimpse-power", urgency 1, expire 30000, "Battery low",
      body names the percentage and, with TimeToEmpty 1440, "about 24 m remaining").
      Verify by: warnings.rs unit test (`low_posts_once`) and a daemon service test on PrivateBus with a
      fake UPower and a fake Notifications server (pattern: glimpse-agents/src/session/tests.rs:269-301;
      seam: `SettingsServices::start_with_buses`, `services.rs:29`).

AC-2  As a laptop user, I want the notification to get louder, not multiply, so that my screen is not buried.
      GIVEN AC-1 posted id 7 WHEN WarningLevel becomes 4 THEN Notify carries replaces_id 7, urgency 2,
      expire 0, "Battery critically low"; WHEN it becomes 5 THEN again replaces_id 7 and the body names the
      action returned by GetCriticalAction ("suspend" for Sleep).
      Verify by: unit test (`levels_replace_in_place`); service test asserting the fake server saw one id.

AC-3  As a laptop user, I want the warning gone when I plug in.
      GIVEN a posted card WHEN State becomes Charging and WarningLevel None THEN CloseNotification(7) is
      sent once and a later discharge posts Low again.
      Verify by: unit test (`ac_withdraws_and_rearms`).

AC-4  As a laptop user, I want no flicker when the gauge wobbles.
      GIVEN Low posted WHEN WarningLevel flips to None and back to Low while State stays Discharging
      THEN no CloseNotification, and at most one Notify replacing id 7.
      Verify by: unit test (`wobble_does_not_withdraw`).

AC-5  As a user who logs in at 4 %, I want to be told immediately.
      GIVEN the daemon starts with the display device Critical and Discharging THEN one critical Notify.
      Verify by: service test; live check with a fake UPower on a private bus.

AC-6  As a desktop user, I want silence and no restart loop.
      GIVEN a system bus with no UPower, and one with a UPower whose DisplayDevice IsPresent is false
      WHEN the daemon runs THEN no Notify, the process stays up, and when UPower appears with a Low battery
      a Notify follows.
      Verify by: service test (`no_system_bus_stays_silent`, `no_display_device_stays_silent`).

AC-7  As a user of a two-battery laptop, I want one warning for the machine, not per pack.
      GIVEN internals BAT0 at WarningLevel Critical and BAT1 at None while display is None THEN no Notify.
      Verify by: unit test (`internals_never_warn`).

AC-8  As a user with a wireless mouse, I want one warning per discharge for that mouse.
      GIVEN a Mouse peripheral at WarningLevel Low then Critical then Action THEN Notify Low, then replace
      with critical urgency, then no further post (Action is capped at Critical); WHEN it charges THEN
      Withdraw; WHEN it falls again THEN it posts again. Two mice have two ids.
      Verify by: unit tests (`peripheral_caps_action`, `peripherals_have_own_slots`).

AC-9  As a user, I want peripheral warnings optional.
      GIVEN peripheral-warnings = false WHEN a mouse is Critical THEN no Notify and any open card is
      withdrawn; GIVEN warnings = false THEN nothing posts and open cards are withdrawn.
      Verify by: unit test (`switches_withdraw_open_cards`); live reload check of config.toml.

AC-10 As a user, I want a hostile device name to do nothing.
      GIVEN a peripheral Model of 1 MB with bidi overrides, "<b>" and newlines WHEN it warns THEN the
      summary is at most 24 chars with hostile characters removed, the body contains no device text, and
      no markup survives.
      Verify by: unit test (`hostile_model_is_capped_and_plain`).

AC-11 As a KDE Connect user, I want my phone not to produce laptop warnings.
      GIVEN a KDE Connect source at 3 % THEN no Notify (the daemon hosts `Battery` with no sources).
      Verify by: service test; inspection of `Dependencies::default()` use in `power.rs`.

AC-12 As a user whose notification daemon starts late, I want the warning once it is up.
      GIVEN Notify fails with a timeout WHEN the next state change publishes THEN the same Post is retried
      and, after success, id and level are recorded; no duplicate after that.
      Verify by: unit test (`failed_post_is_retried_on_next_state`); service test with a delayed fake server.

AC-13 As a user, I want the texts translated.
      GIVEN every msgid in §7.3 and a `[regional] language = "ru"` WHEN the daemon posts THEN the Russian
      catalog's text is used (the daemon called `init_translations`), and each msgid is extracted by
      `just check-strings` with the same placeholders.
      Verify by: `just check-strings`; `just extract-strings` diff; live check with `LANGUAGE=ru`.

AC-14 As a locked-screen user, I want the warning kept even though no popup shows.
      GIVEN the session is locked WHEN Critical is posted THEN the card lands in history unread and the
      lock chip count reflects it; no popup surface appears.
      Verify by: live check on a second session with a fake UPower; the gate itself is covered by the
      notifications tests (`notifications/README.md:30-32`).

AC-15 As a user who unlocks at 4 %, I want to see the warning I missed.
      GIVEN Critical (or Action) was posted while locked as id 9 WHEN LockedHint becomes false and the level
      is still Critical or Action THEN one Notify with replaces_id 9, the current percentage and critical
      urgency; GIVEN the card was posted while unlocked, or the level was Low, or AC withdrew it, THEN
      nothing is posted on unlock; GIVEN LockedHint cannot be read THEN no card is ever hidden and nothing
      is re-posted.
      Verify by: unit tests (`unlock_reposts_hidden_critical`, `unlock_ignores_low_visible_and_withdrawn`,
      `unreadable_lock_state_never_hides`); service test driving a fake login1 session on the private
      system bus.

AC-16 As a settings-daemon user, I want a battery or notification failure never to disturb desktop mode,
      location or solar phase.
      GIVEN (a) no system bus, (b) a fake UPower that vanishes mid-run, (c) a Notifications server that
      never answers, and (d) a panic injected in the notifier task WHEN the daemon runs THEN `Settings1`
      `Mode`, `Solar` and `Geolocation` still answer and follow their inputs, the `config-status` action
      reports as before, and the process does not exit.
      Verify by: service tests beside `services/tests.rs:26-291` (existing assertions unchanged, a new
      test per case).

AC-17 As a user, I want a second daemon instance to do no harm.
      GIVEN one settings daemon owns me.aresa.Glimpse.Settings WHEN another starts THEN it exits with an
      error before the power runtime starts and the fake Notifications server sees no Notify from it.
      Verify by: extend the duplicate-instance test at `services/tests.rs:54-56`.

AC-18 As a maintainer, I want CI to enforce the config table and the unchanged packaging.
      GIVEN the `[power]` table THEN `just verify` passes, `config.schema.json` has `power`,
      `just gen-config-commented config` shows no diff, and no unit, D-Bus file or package list changed.
      Verify by: `just verify`; `git diff --stat` of data/ and crates/glimpse-package/ is empty.
```

## 11. Rollout and slicing

Each step merges alone and leaves the desktop working.

| Step | Task | Crates | Depends on |
| --- | --- | --- | --- |
| 1 | `[power]` table, registration, commented config and schema regeneration | glimpse-config, data | — |
| 1 | `clock()` moves to `glimpse-utils`; panel calls it | glimpse-utils, glimpse-panel | — |
| 1 | `GetCriticalAction` on `UPowerProxy`; a `testing` constructor giving `Buses` a system and a session connection | glimpse-dbus | — |
| 2 | `Warnings` state machine including `set_locked` and its tests (AC-1…4, 7…10, 12, 15 unit part) | glimpse-settings-daemon | step 1 |
| 3 | `power` module: `Runtime`, `notify` dispatch and text, hosting `Battery`, wiring in `services.rs` and `main.rs` (`init_translations`), reload path; fake UPower + fake Notifications tests (AC-1, 2, 5, 6, 11, 12, 13, 16, 17) | glimpse-settings-daemon | step 2 |
| 4 | `power/locked.rs` follower and its wiring; fake login1 test (AC-14, 15 service part) | glimpse-settings-daemon | step 3 |
| 5 | Docs: daemon README sentence, translations rule, `website/config/services/power.md`, systemd row; live check on the real session (AC-14, 18) | docs, website | step 4 |

## 12. Rejected

- ~~Notify from the panel~~: dies with the panel, and a hidden or restarting bar loses the warning (bead).
- ~~Notify from the lock~~: the lock is a security boundary that must stay minimal (`AGENTS.md`), and the panel and lock both host `Battery`, so two posters would double-post.
- ~~A new `glimpse-power` provider (own binary, unit, `me.aresa.Glimpse.Power` name)~~: cleaner charter, but a crate, a unit, a bus-name file, about ten packaging and doc touch points and a process for what the settings daemon can host (user decision Q1).
- ~~Fold into `glimpse-idle`~~: idle's charter is the inhibitor authority, it deliberately has no `glimpse-services` graph (`idle/README.md:70-73`), and a battery bug would restart the idle timers (user decision Q1).
- ~~Own thresholds in glimpse config (COSMIC's 20/10)~~: would disagree with `CriticalPowerAction` and `UPower.conf` (D1).
- ~~Per-internal-battery warnings~~: UPower acts on the composite (D2).
- ~~Warn from BlueZ `Battery1` directly~~: UPower 1.91.5 already surfaces it as a device with a computed level (§3).
- ~~KDE Connect phone warnings~~: no level on the wire; the phone warns itself (D9).
- ~~No peripheral warnings at all~~, ~~KDE Connect thresholds of our own~~: user decision Q2 chose UPower peripherals only.
- ~~Sound, action buttons, `transient`/`resident` hints~~: no daemon capability or no need (D18).
- ~~gsd's "never warn that device again"~~: one per discharge is enough and recovers (D7).
- ~~Hosting the notifier in `glimpse-notifications`~~: it is a UI crate (GTK), and a user running another notification daemon would lose the warnings.
- ~~A typed `Power1` interface or any new `Settings1` member~~: no consumer (D21).
- ~~Hosting the `Session` service in the daemon for the lock state~~: it needs a `CompositorHandle` (D23).
- ~~Re-posting Low on unlock, or every card on every unlock~~: only hidden Critical and Action cards come back (D16).
- ~~`PrepareForSleep` subscription~~: UPower resyncs after resume; edge-triggered rules need none (D14).
- ~~Notification on `Notify` failure~~: the failing component is the notification service (D15).
- ~~A sticky Low popup~~: a permanent card for the whole low phase (user decision Q4).

## 13. Open questions

None. All four were put to the user and are closed:

- ~~**Q1 — Which process owns the warning?**~~ Decided: **(c) fold into `glimpse-settings-daemon`**, not a new `glimpse-power` provider and not `glimpse-idle` (D11, D17, D20, D21, §5.1, §8, §12).
- ~~**Q2 — Do peripherals warn by default?**~~ Decided: **(b) UPower peripherals warn at Low and Critical, default on, switch `peripheral-warnings`**; KDE Connect excluded (D7–D9, §6).
- ~~**Q3 — Is a warning posted while locked shown again on unlock?**~~ Decided: **(b) re-post Critical and Action on unlock** via a `LockedHint` follower (D16, D23, §7.1 rule 8, AC-15, §11 step 4, unconditional).
- ~~**Q4 — Low popup lifetime.**~~ Decided: **(a) 30 s, then history** (D5).

## 14. Review record

Draft by LowBatteryPlan, 2026-10-26.

Reviewed by the orchestrator, 2026-10-26: prose against code blocks (`Level`, `Subject`, `Notice`,
`Step`, `Slot`, `Warnings`, `Power`/`PowerConfig`, §7.3 texts) is consistent; D3/D4 rules match §7.1
rules 2-3 and the AC-3/AC-4 flows. No changes. Open questions go to the user.

Reworked by LowBatteryPlan, 2026-10-26, after the user's answers to Q1–Q4 (§13): owner moved to
`glimpse-settings-daemon` (D11, D17, D20–D23, §2, §5, §8, §9, §10, §11, §12); `Warnings` gained `hidden`
and `set_locked` and `posted` now takes the `Notice`; the locale rule is amended (D22). Self-consistency
walk redone: every type in the prose (`Level`, `Subject`, `Notice`, `Step`, `Slot`, `Warnings`,
`PowerConfig`/`Power`, `Runtime`, `dispatch`) is declared in §6–§7 and every field there is used in the
rules or flows; the walk found and fixed `ChargeState` (imported, unused in the shape, now dropped from
the imports) and the `posted(subject, level, id)` form in the old flows. Not re-reviewed by the
orchestrator.
