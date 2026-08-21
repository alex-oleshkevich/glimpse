# Interfaces glimpse Talks To

Signatures below were captured with `busctl introspect` against a live session on 2026-08-20, not
copied from documentation. Where a remote interface has drifted from its spec, the machine is right.

Re-verify before trusting any line here:

```bash
busctl --system introspect org.freedesktop.NetworkManager /org/freedesktop/NetworkManager
busctl --user   introspect org.kde.StatusNotifierWatcher  /StatusNotifierWatcher
busctl --user   monitor    org.freedesktop.Notifications
```

The `FLAGS` column is the part people skip. `emits-change` is the promise that makes proxy property
caching safe; its absence means the value can change without any signal.

---

## NetworkManager — system bus

Destination `org.freedesktop.NetworkManager`.

**`/org/freedesktop/NetworkManager`, interface `org.freedesktop.NetworkManager`**

| Member | Signature | Note |
| --- | --- | --- |
| `GetDevices` | `→ ao` | or read the `Devices` property |
| `ActivateConnection` | `ooo → o` | connection, device, specific object |
| `AddAndActivateConnection` | `a{sa{sv}}oo → oo` | settings dict, device, specific object |
| `DeactivateConnection` | `o →` | |
| `Devices` | `ao` | emits-change |
| `ActiveConnections` | `ao` | emits-change |
| `PrimaryConnection` | `o` | emits-change |
| `PrimaryConnectionType` | `s` | emits-change; e.g. `"802-11-wireless"` |
| `State` | `u` | emits-change; `70` = connected-global |
| `Connectivity` | `u` | emits-change |
| `WirelessEnabled` | `b` | emits-change, **writable** — the rfkill toggle |
| `DeviceAdded` / `DeviceRemoved` | signal `o` | |
| `StateChanged` | signal `u` | |

**A device, interface `org.freedesktop.NetworkManager.Device.Wireless`**

| Member | Signature | Note |
| --- | --- | --- |
| `GetAllAccessPoints` | `→ ao` | |
| `RequestScan` | `a{sv} →` | pass an empty dict; rate-limited by NM |
| `AccessPoints` | `ao` | emits-change |
| `ActiveAccessPoint` | `o` | emits-change |
| `LastScan` | `x` | emits-change; CLOCK_BOOTTIME **milliseconds**, `-1` if never |
| `AccessPointAdded` / `AccessPointRemoved` | signal `o` | |

**An access point, interface `org.freedesktop.NetworkManager.AccessPoint`**

| Member | Signature | Trap |
| --- | --- | --- |
| `Ssid` | `ay` | **bytes, not a string.** Need not be valid UTF-8. Attacker-controlled and unbounded — cap and sanitize |
| `Strength` | `y` | `u8`, 0–100. Not `u32` |
| `Frequency` | `u` | MHz |
| `Flags` / `WpaFlags` / `RsnFlags` | `u` | security capabilities; `RsnFlags != 0` means WPA2/3 |
| `HwAddress` | `s` | BSSID |
| `Mode` | `u` | |
| `LastSeen` | `i` | `i32`, CLOCK_BOOTTIME **seconds**. Different unit *and* sign from `LastScan` |

NetworkManager exposes **access points, not networks**. Several APs share one SSID; grouping them
into a single network row is client-side work. Group on `Ssid` bytes, keep the strongest member, and
do not assume the set is stable between scans.

---

## BlueZ — system bus

Destination `org.bluez`. BlueZ has **no manager object with a device list** — enumeration goes
through `org.freedesktop.DBus.ObjectManager` at `/`. See `references/proxies.md` → Object Trees.

**`/org/bluez/hciN`, interface `org.bluez.Adapter1`**

| Member | Signature | Note |
| --- | --- | --- |
| `StartDiscovery` / `StopDiscovery` | `→` | |
| `RemoveDevice` | `o →` | this is "forget", not "disconnect" |
| `Powered` | `b` | emits-change, writable |
| `Discovering` | `b` | emits-change — the authoritative scanning state, never a local flag |
| `Discoverable` | `b` | emits-change, writable |
| `DiscoverableTimeout` | `u` | emits-change, writable |

`Discovering` is what the UI renders. Setting a local "scanning" boolean when `StartDiscovery`
returns will drift: BlueZ stops discovery on its own timeout, and other clients start and stop it
too.

Pairing needs an `org.bluez.Agent1` registered with `AgentManager1`. Whether glimpsed registers one
is still open in `specs/001_architecture.md` — do not add it without settling that.

---

## logind — system bus

Destination `org.freedesktop.login1`.

**`/org/freedesktop/login1`, interface `org.freedesktop.login1.Manager`**

| Member | Signature | Note |
| --- | --- | --- |
| `Inhibit` | `ssss → h` | what, who, why, mode → **a file descriptor**. The inhibitor lasts exactly as long as the fd is open |
| `ListSessions` | `→ a(susso)` | id, uid, user, seat, path |
| `LockSession` / `UnlockSession` | `s →` | by session id |
| `PrepareForSleep` | signal `b` | `true` before sleeping, `false` after waking |
| `PrepareForShutdown` | signal `b` | |
| `SessionNew` / `SessionRemoved` | signal `so` | |
| `InhibitDelayMaxUSec` | `t` | const; the deadline for delay inhibitors — 5 s on this machine |
| `BlockInhibited` / `DelayInhibited` | `s` | emits-change; colon-separated list of what is held |

**A session, interface `org.freedesktop.login1.Session`**

| Member | Signature | Note |
| --- | --- | --- |
| `Lock` / `Unlock` | signal | **this is how the locker is asked to appear** |
| `SetLockedHint` | `b →` | tell logind the screen is locked, once it actually is |
| `SetIdleHint` | `b →` | |
| `Active` | `b` | emits-change; false when another VT has the seat |
| `LockedHint` / `IdleHint` | `b` | emits-change |

`Inhibit` returning an fd is the part that is easy to get wrong: dropping the fd releases the
inhibitor. Hold it in the service that took it, and close it explicitly when the reason ends. Miss
the `InhibitDelayMaxUSec` deadline and logind sleeps anyway.

`specs/009_systemd.md` prefers `systemd-lock-handler` over the daemon for lock-before-sleep,
because that keeps locking working when glimpsed is down. Follow the spec; do not quietly move it.

---

## UPower — system bus

Destination `org.freedesktop.UPower`. Read `/org/freedesktop/UPower/devices/DisplayDevice` — the
aggregate UPower computes — rather than summing batteries yourself.

| Member | Signature | Note |
| --- | --- | --- |
| `Percentage` | `d` | emits-change |
| `State` | `u` | emits-change; `1` charging, `2` discharging, `4` fully charged |
| `TimeToEmpty` / `TimeToFull` | `x` | seconds; `0` means unknown, not zero |
| `IconName` | `s` | emits-change; a themed icon name UPower picks for you |
| `WarningLevel` | `u` | emits-change |
| `IsPresent` | `b` | emits-change |
| `Type` | `u` | emits-change; `2` = battery |

`TimeToEmpty == 0` is "not yet known", which happens for a minute or so after a state change.
Rendering it as `0:00` is wrong; render nothing.

---

## MPRIS — session bus

Any name matching `org.mpris.MediaPlayer2.*`, object `/org/mpris/MediaPlayer2`. Discover players by
listing names and filtering the prefix; track appearance and disappearance with `NameOwnerChanged`.

**`org.mpris.MediaPlayer2`** — `Identity` (`s`), `DesktopEntry` (`s`), `CanRaise`/`CanQuit` (`b`),
`Raise`, `Quit`.

**`org.mpris.MediaPlayer2.Player`**

| Member | Signature | Note |
| --- | --- | --- |
| `PlayPause` / `Play` / `Pause` / `Stop` / `Next` / `Previous` | `→` | |
| `Seek` | `x →` | relative, microseconds |
| `SetPosition` | `ox →` | track id + absolute microseconds |
| `PlaybackStatus` | `s` | emits-change; `Playing` / `Paused` / `Stopped` |
| `Metadata` | `a{sv}` | emits-change |
| `Volume` | `d` | emits-change, writable |
| `Position` | `x` | **no emits-change** — see below |
| `CanControl` | `b` | **no emits-change on some players**, observed on a live WebKit player |
| `Seeked` | signal `x` | absolute position, microseconds |

`Position` deliberately does not emit `PropertiesChanged`; the spec says so, and the introspection
confirms it. Do not cache it and do not poll it at frame rate. Read it once when playback state
changes, take `Seeked` when it arrives, and interpolate locally from a monotonic clock.

`Metadata` keys are open and players disagree:

| Key | Signature | Trap |
| --- | --- | --- |
| `xesam:title` | `s` | untrusted, unbounded |
| `xesam:artist` | `as` | an **array**, not a string |
| `mpris:length` | `x` | microseconds |
| `mpris:trackid` | `o` | an object path; some players send `s` in violation of the spec |
| `mpris:artUrl` | `s` | often `file:///tmp/...` — an ephemeral file that may vanish before you load it, and a path you must not trust |

Read the keys you know, ignore the rest, and treat a missing key as normal rather than an error.

---

## StatusNotifierItem — session bus

**`org.kde.StatusNotifierWatcher` at `/StatusNotifierWatcher`** — the name glimpsed owns.

| Member | Signature |
| --- | --- |
| `RegisterStatusNotifierItem` | `s →` |
| `RegisterStatusNotifierHost` | `s →` |
| `UnregisterStatusNotifierItem` | `s →` |
| `RegisteredStatusNotifierItems` | `as`, emits-change |
| `IsStatusNotifierHostRegistered` | `b`, emits-change |
| `ProtocolVersion` | `i`, emits-change |
| `StatusNotifierHostRegistered` / `...Unregistered` | signal, no arguments |

The registered-items strings are `":1.15968/StatusNotifierItem"` — unique name concatenated with
object path, no separator beyond the `/`. Items register with either a bus name or a path, so the
sender from `#[zbus(header)]` is the authoritative half.

**`org.kde.StatusNotifierItem`** — the item, at whatever path it registered.

| Member | Signature | Note |
| --- | --- | --- |
| `Activate` / `SecondaryActivate` | `ii →` | x, y screen coordinates |
| `ContextMenu` | `ii →` | |
| `Scroll` | `is →` | delta, orientation |
| `ProvideXdgActivationToken` | `s →` | newer; send before `Activate` so the app may raise itself |
| `Id` | `s` | emits-change — **item identity comes from here**, not the bus name |
| `Title` | `s` | emits-change; untrusted |
| `Status` | `s` | emits-change; `Active` / `Passive` / `NeedsAttention` |
| `Category` | `s` | emits-change |
| `IconName` | `s` | emits-change |
| `IconPixmap` | `a(iiay)` | emits-change; width, height, **ARGB32 network byte order** |
| `OverlayIconName` / `OverlayIconPixmap` | `s` / `a(iiay)` | |
| `AttentionIconName` / `AttentionIconPixmap` | `s` / `a(iiay)` | |
| `IconThemePath` | `s` | emits-change; a private theme dir to search first |
| `ItemIsMenu` | `b` | emits-change; when true, left click opens the menu instead of activating |
| `Menu` | `o` | emits-change; path to the dbusmenu, commonly `/MenuBar` |
| `ToolTip` | `(sa(iiay)ss)` | icon name, pixmap, title, body |
| `NewIcon`, `NewTitle`, `NewToolTip`, `NewMenu`, `NewAttentionIcon`, `NewOverlayIcon` | signal, **no arguments** | |
| `NewStatus` | signal `s` | the only one carrying a payload |

The `New*` signals carry nothing — they mean "re-read the property". Items are also inconsistent
about whether they emit `PropertiesChanged`, `New*`, or both, so handle both and let the equality
gate collapse the duplicate.

Pixmaps must not travel through the glimpse socket. Decode `a(iiay)` in the daemon, write
`$XDG_RUNTIME_DIR/glimpse/tray/<item>-<hash>.png`, and publish the path. The content hash is what
lets the equality gate suppress a no-op update.

---

## com.canonical.dbusmenu — session bus

At the path the item's `Menu` property names, on the item's own connection.

| Member | Signature | Note |
| --- | --- | --- |
| `GetLayout` | `iias → u(ia{sv}av)` | parent id, recursion depth (`-1` = all), property filter → **revision** + nested layout |
| `GetGroupProperties` | `aias → a(ia{sv})` | |
| `GetProperty` | `is → v` | |
| `AboutToShow` | `i → b` | **returns whether the layout changed — you must await it** |
| `AboutToShowGroup` | `ai → aiai` | |
| `Event` | `isvu →` | id, event id, data, timestamp. **Void — fire and forget** |
| `EventGroup` | `a(isvu) → ai` | |
| `Status` | `s`, emits-change | `normal` / `notice` |
| `IconThemePath` | `as`, emits-change | |
| `Version` | `u`, emits-change | `4` on current implementations |
| `LayoutUpdated` | signal `ui` | revision, parent id |
| `ItemsPropertiesUpdated` | signal `a(ia{sv})a(ias)` | updated, then removed |
| `ItemActivationRequested` | signal `iu` | |

The asymmetry is the thing to remember: `AboutToShow` returns a `bool` and must be awaited before
rendering, while `Event` returns nothing and should be sent `no_reply`. Getting these the wrong way
round produces either a stale menu or a handler that blocks on a click.

`GetLayout` returns a revision. Compare it with the revision from `LayoutUpdated` and skip the
re-fetch when it has not moved.

Fetch menus on pointer-enter, not on click — `specs/004_panel.md`. A menu fetch is two
round trips to a foreign process that may be slow or hostile.

---

## org.freedesktop.Notifications — session bus

The other name glimpsed owns, at `/org/freedesktop/Notifications`.

| Member | Signature |
| --- | --- |
| `Notify` | `susssasa{sv}i → u` |
| `CloseNotification` | `u →` |
| `GetCapabilities` | `→ as` |
| `GetServerInformation` | `→ ssss` |
| `NotificationClosed` | signal `uu` |
| `ActionInvoked` | signal `us` |
| `ActivationToken` | signal `us` |

`Notify` arguments in order: app_name `s`, replaces_id `u`, app_icon `s`, summary `s`, body `s`,
actions `as`, hints `a{sv}`, expire_timeout `i`. It returns the assigned id.

- `replaces_id` of `0` means "new"; anything else must reuse that id's slot.
- `actions` is a **flat list of pairs** — `[id, label, id, label, …]`. Odd length is malformed input,
  not a panic.
- `expire_timeout` of `-1` means "server decides", `0` means "never expire".
- `summary` and `body` are attacker-controlled. `body` may contain a small HTML-ish markup subset
  only if `GetCapabilities` advertises `body-markup` — so whether you must sanitize is a decision
  glimpsed makes and then has to honour.
- `NotificationClosed`'s second `u` is the reason: 1 expired, 2 dismissed, 3 closed by call, 4
  undefined.

`ActivationToken` is newer than the spec text most implementations were written against; emit it
before `ActionInvoked` so the receiving app can raise itself under Wayland.
