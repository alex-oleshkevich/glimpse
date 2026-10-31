# glimpse-services

## The framework

**A service says what state it starts in; nobody else gets to.** `Service::initial_state` is
required because a handle answers `snapshot()` before `run` is called — a deferred state would leak
an `Option` into every consumer. **`Running<S>` is one owned service — spawn, reconfigure, stop.** A
dropped `Running` cancels its service, so no root writes its own `Drop`.

**The unchanged-configuration gate belongs in `run`, never on the sender** — see
`.claude/rules/daemon.md`; a `try_send` onto a full inbox must not record a config that never
arrived. **`Degraded` is a running service** that keeps publishing what it can, so a consumer must not dim its
values. **`ServiceState::unavailable_reason` is the one mapping from health to what a consumer is
told**, so a new variant makes every provider fail to compile until it decides what to say.

### Sources

`SourceGuard` is `#[must_use]`: `ctx.spawn(...)` written as a statement drops the guard at the
semicolon and aborts the task before it runs. **`spawn_detached` is the one `Ctx` task handing back
no guard**, because a handler returns before the work is done and an abort-on-drop guard would cancel
the call it just deferred; shutdown still stops it through the cancellation token. A panic inside a
source is caught, logged and turned into `degraded`, since a source is the likeliest place to panic
and least visible otherwise, and `Sub::watch` reads a dependency's current value before waiting for
changes so a consumer gets a complete snapshot without a race.

### Subscriptions

**`Sub::deadline` waits on the wall clock, not on elapsed time.** A tokio timer runs on
`CLOCK_MONOTONIC`, which does not advance while the machine is suspended, so a sleep fires late by
however long the lid was shut. **Tearing a timer down does not unqueue an event it has already
emitted**, so the handler ignores an event whose deadline no longer matches.

## The services

**geolocation** — `manual` publishes the configured pair; `geoclue`
runs one transient client per attempt, subscribed to `Location` **before** `Start` so an existing fix
arrives as that subscription's first item. Until the first fix a client is held for minutes rather
than seconds, since GeoClue keeps trying while a client is active; a refresh is bounded tighter.
Coordinates are **last known, not current** — a lost bus degrades the service without clearing them,
and an injected cache file carries the last GeoClue fix across restarts, published as degraded until
GeoClue answers.

**night light** — its solar input is an injected watch receiver, supplied by the settings provider's
follower in sunset. Owner loss clears the observed phase and degrades automatic mode; manual
schedules remain independent. The tick period lives in its own subscription key, so crossing a
transition window rebuilds the timer at the new cadence while the ramp position still comes from the clock. Release is
decided on the solar phase, not the computed value, since a ramp can round to `DAY` without meaning
daylight — a manual override held at exactly `DAY` releases too, and `Off` glides to `DAY` first.
A pause holds neutral `DAY` until the existing night boundary; it never changes the manual
temperature override.

**battery** — the chip reads UPower's `DisplayDevice`; internals and
charge-threshold details come from the real `BAT*` objects, since the composite omits them. `TimeTo*`
`0`, `ChargeCycles` `<= 0` and empty serials are treated as absent. Other services contribute
peripherals as a `BatterySource` — a watched state and a projection wired in the composition root —
appended after UPower's in registration order; a source that stops contributes nothing.

**gamma** — `trait Gamma` is declared here and implemented in `glimpse-sunset`, since this crate is
linked into the panel and none may gain a Wayland dependency. It is **synchronous** and blocks with
`block_in_place`, which keeps it dyn-compatible so `NightLight` can hold `Box<dyn Gamma>`.

**clipboard** — `trait Selection` is `Gamma`'s counterpart, implemented in `glimpse-panel`.
Every `events()` call yields a fresh stream, since the runtime rebuilds a torn-down source.
History is **in memory only**, and an entry's `id` fingerprints `(kind, content)` rather than the
mime spelling, so dedup closes the echo loop. `is_sensitive` classifies an offer before its
content is read; secrets leave no history entry or tombstone.

**brightness** — `SysfsBacklight` reads `/sys/class/backlight` with `tokio::fs` and writes through
logind. `current` moves when a command is accepted, `confirmed` when the write lands, and a failed
write rolls `current` back unless a newer value is queued. Sources collapse to the highest-preference
controller per connector, and a `backlight` uevent names a device to re-read, never a value to trust.

`DdcBacklight` (under `brightness/`) talks DDC/CI over `/dev/i2c-*` for external monitors sysfs never
sees, through `ddc`/`ddc-i2c` rather than the `ddcutil` binary. **A connector's `ddc` symlink is not
where DisplayPort answers** — DDC/CI runs over the AUX channel, exposed as a connector-owned `i2c-N`
child whose name contains "aux". **A built-in panel connector (`eDP`, `LVDS`, `DSI`) is never probed
at all**, since one DDC/CI transaction there can freeze the panel on its last frame until the next
modeset; `[brightness] ddc` turns it off.

**compositor** — a command the provider's capabilities do not cover answers `Unsupported` locally, and disabling the
last enabled output is refused from the mirrored snapshot, neither costing a bus call. No owner
since the mirror started is not a fault: it publishes status `unsupported`, no sections and an
inactive privacy, stays running and answers every command `Unsupported`; an owner lost after it
served degrades the service and keeps the last state. **Urgency is derived here so every client
sees one answer** — a workspace is urgent when any window on it is. **keyboard** derives badges
unless labels override them. Layout memory follows `[keyboard] remember` (window, app or global),
uses ordered provider events, and resets when the provider disconnects. A layout change that lands
the index a restore asked for is published with `restored`; any other layout change is not.

**calendar** — a local path is watched, `http(s)` is polled, and a `file://` holding a one-line feed URL does both. **A failure
reason never contains the uri.** `failing` lists a source at once when its failure needs the user —
a missing file, a document that is not iCalendar, a 4xx — and a transport error, timeout, 408, 429 or
5xx only after three reads in a row, so a dropped network is not reported as a broken calendar.

**weather** — one state entry per place watched. Places are not configured, they are **leased**: a
consumer calls the typed watch method and the registration lasts thirty minutes unless renewed, and
not renewing is how you stop; a fresh install sends no outbound request otherwise.
`timeformat=unixtime` is load-bearing, since with `timezone=auto` the provider otherwise returns
naive local ISO strings with no offset, and `observed_at` freezes when the network dies rather than
reflecting the fetch clock.

**printing** — CUPS state follows system-bus notifier signals under a renewable `dbus://`
subscription, and `Cancel-Job`/`Release-Job` on an already-finished job are not failures.
Job commands to the local CUPS go over `/run/cups/cups.sock` with `Authorization: PeerCred`, because
`@OWNER` policies authenticate only there and refuse the same request over TCP with 401.

**mpris** — both sources subscribe before they read, so nothing is missed between the two, and there
is no progress timer since the payload carries `position_us`, `position_at` and `rate`.

**notifications** — `Store` holds no publisher and no connection, so bound handling and per-app
clearing are testable without either. **History is bounded in memory: `keep` read records plus a
200-record unread ceiling** — evict the oldest non-pending unread record, or reject the incoming record
when all are pending, with `CLOSED_EXPIRED`. **The default action does not spend one of the three button
slots** — it drives the card itself. **`NameTaken` is degraded, not fatal** — dunst, mako or a Plasma
session may already own the name. **`app_icon` is identity, never media**: only `image-data`,
`image-path` and legacy `icon_data` fill the image slot, and a path passed as `app_icon` is the
header icon. Every sender-chosen string is capped before it reaches a payload, and the body goes
through the markup sanitiser rather than `clean`, which would flatten the newline the common
sender shape depends on. `NotificationClosed` reasons are the specification's numbers, and so is
urgency — an invented value is read as normal rather than refused. Actions arrive as a flat list
of alternating key and label, an odd trailing entry is dropped, a replacement spends no id, and
the counter steps over zero, which the wire reserves for "new".

**tray** — glimpse takes `org.kde.StatusNotifierWatcher` when free and hosts on whoever holds it when
not; an empty item list is a normal state, never `degraded`. **A taken name is not a broken tray** —
a failed claim registers as a host with the incumbent and follows its registration signals instead.
**The claim lives inside the `NameOwnerChanged` source, not in `start`**, since claiming in `start`
would race the signal that recovers a lost name. **A fresh owner sweeps, because items register once
and never learn they were forgotten**: it announces itself first, then sweeps `ListNames`.

**bluetooth** — enumerate once per generation and never poll. **BlueZ's `ObjectManager` is at `/`, not
`/org/bluez`** — only `PropertiesChanged` and `Disconnected` use that namespace. **Match a device path
by shape, not by prefix** — connecting adds `dev_XX/fd0` and `sep1`…`sep6` as children a prefix match
would count as phantom devices. **`busy` is cleared by its own command's completion, never by the
property moving** — `Connect()` on an already-connected device returns `AlreadyConnected` with no
state change, which would leave that row spinning forever under a property-based clear. **Pairing ends
connected.**

**network** — enumerate once per generation. **Only a device a user can act on reaches the model** —
`Managed` and a user-facing `DeviceType`, since `Devices` and `AllDevices` both return bridges too.
**The connected beacon wins** an access-point row regardless of signal.

**audio** — the libpulse bridge (`services/audio/pulse.rs`) owns one OS thread: lock, create the
`Operation`, unlock, await the oneshot its callback completes, since a Pulse callback runs under that
lock. **A PA name is never capped, only displayed.**

**removable** — enumerate once and never poll.
**`remove` and `eject` unmount this drive's mounted volumes and lock its unlocked ones first**,
since `Drive.Eject` refuses with `DeviceBusy` otherwise; `remove` then ejects when the drive is
ejectable, else powers off when it can, else stops, and a refused unmount or lock stops it before
either. A drive command is refused while one of its volumes is busy, and the reverse. **Capacity is a `statvfs` sample** — `Filesystem.Size` is 0 for vfat/exfat.

**kdeconnect** — offer each action only while its plugin is loaded. **It never starts the daemon**: the owner is read with `GetNameOwner`, since every
call goes to that unique name, which the bus cannot activate. **The daemon emits no
`PropertiesChanged`**, so a match rule marks it stale instead.

**places** — reads four filesystem sources with no bus: `user-dirs.dirs`, `gtk-3.0/bookmarks`,
`$XDG_RUNTIME_DIR/gvfs`, `Trash/files`. **`user-dirs.dirs` is parsed, never read through
`glib::user_special_dir`**, since that enum is closed to eight entries. **`empty_trash` clears the
home trash only** — `files/`, `info/`, `expunged/` and `directorysizes`, leaving `.Trash-$UID` alone.

**session actions** — logind capabilities and inhibitors on their own subscription, window count from
the compositor. **Log out, reboot and power-off are admitted on the session bus by
`me.aresa.Glimpse.Session` and this service never runs them**; suspend, hibernate, and lock still
call logind.

**privacy** — camera, microphone, screen capture and location as one state, reporting rather than
enforcing. **The camera source watches inotify OPEN/CLOSE on each `/dev/video*` node, re-arms on
udev video4linux hotplug, and keeps a watch whose node has disappeared until the kernel drops it, so
a recreated node gets its own watch. It scans `/proc` fds on events, delaying closes by 500 ms, never
polling or using PipeWire**, since PipeWire is blind to raw V4L2 capture. **`app: None` is first-class on every
resource, and always the case for location**, since GeoClue has no per-client attribution; it takes
no commands at all.

**exec** — **the service never forks**: each start is a transient systemd user unit
`app-glimpse-<panel>-<applet>-<slot>-<pid>-<n>.service` started through the injected `Launcher`, and
systemd owns the process, its restart curve, cgroup, `MemoryMax` and logs. Applets run unconfined
with the panel's environment; there is no sandbox. One configured instance runs one process per
placement apart from output: the first slot at that placement owns the unit, listening on its
socket under the runtime dir, named in `GLIMPSE_APPLET_SOCKET`, one connection at a time; later
slots at that placement mirror its state and route commands to it, one taking over on detach. The
applet speaks first with `Hello`, and every `Hello` resets its tree and advances the generation, so
stale UI events cannot reach a reloaded applet.
`argv[0]` resolves against the merged environment's `PATH`; the working directory is
the entry's `Path`, or the desktop file's directory. Placement and options changes go over the
socket without a restart; a changed entry or `env` stops the unit and starts the next `<n>`. Every
panel startup sweeps leftover units and the sockets of dead panels; a clean stop stops each unit,
and a crashed panel closes the sockets so applets exit on EOF. An applet that cannot reach its panel
socket exits 69, and its unit is not restarted. Without the systemd user manager every
slot fails with a reason and the panel keeps running. A dev slot from AppletHost takes over the
same-id slot, has no automatic restart, and writes stderr to the caller. The host passes the applet's state
directory as `state` and its desktop entry's id, name and icon as `entry` in `hello`. A URI must
pass the scheme allowlist, and the panel refuses an executable file or a launcher type. Lines past
the rate cap are read later, never fatal; invalid JSON, oversized lines, invalid trees, more than
five `Hello`s in ten seconds and a full write queue kill only the offending unit.

## Rules

Concrete handles only — no broker, registry or string routing; see `.claude/rules/daemon.md`.
