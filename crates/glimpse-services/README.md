# glimpse-services

The service framework and every service implementation.

A service is one Tokio task owning typed state and typed commands. The runtime owns the select loop
and handlers run serially on `&mut self`; cloneable handles give in-process consumers a snapshot, a
watch receiver, health and command methods. `service.rs`, `context.rs`, `subscription.rs` and
`publisher.rs` are that runtime, endpoint, watch-backed state, sources, health and command plumbing;
`services/` is one module per service.

## The framework

**A service says what state it starts in; nobody else gets to.** `Service::initial_state` is
required because a handle answers `snapshot()` before `run` is called — a deferred state would leak
an `Option` into every consumer. **`Running<S>` is one owned service — spawn, reconfigure, stop.** A
dropped `Running` cancels its service, so no root writes its own `Drop`.

**The unchanged-configuration gate belongs in `run`, never on `ServiceSender`** — see
`.claude/rules/daemon.md`; a `try_send` onto a full inbox must not record a config that never
arrived. Health is `Starting`, `Running`, `Degraded { reason }` or `Stopped { reason }`, and
**`Degraded` is a running service** that keeps publishing what it can, so a consumer must not dim its
values. **`ServiceState::unavailable_reason` is the one mapping from health to what a consumer is
told**, so a new variant makes every provider fail to compile until it decides what to say.

### Sources

Everything reaching a handler arrives from a source, and every source is one `ctx` call returning a
`SourceGuard`; dropping it is the whole cancellation story.

| Source | Produces | For |
| -------------------- | --------------- | --------------------------------------------------- |
| `ctx.spawn`          | one event       | one unit of async work whose result is an event     |
| `ctx.spawn_detached` | nothing         | work with no result to report                       |
| `ctx.interval`       | an event a tick | polling, clocks; `at_interval` picks the first tick |
| `ctx.stream`         | many events     | a backend signal stream, a watch, a subscription    |
| `Sub::watch`         | many events     | another service's typed state                       |

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

**geolocation** — two providers behind one state. `manual` publishes the configured pair; `geoclue`
runs one transient client per attempt, subscribed to `Location` **before** `Start` so an existing fix
arrives as that subscription's first item. Coordinates are **last known, not current** — a lost bus
degrades the service without clearing the cache.

**night light** — the tick period lives in its own subscription key, so crossing a transition window
rebuilds the timer at the new cadence while the ramp position still comes from the clock. Release is
decided on the solar phase, not the computed value, since a ramp can round to `DAY` without meaning
daylight — a manual override held at exactly `DAY` is the one exception.

**battery** — mirrors UPower and power-profiles-daemon. The chip reads `DisplayDevice`; internals and
charge-threshold details come from the real `BAT*` objects, since the composite omits them. `TimeTo*`
`0`, `ChargeCycles` `<= 0` and empty serials are treated as absent.

**gamma** — `trait Gamma` is declared here and implemented in `glimpse-sunset`, since this crate is
linked into the panel and none may gain a Wayland dependency. It is **synchronous** and blocks with
`block_in_place`, which keeps it dyn-compatible so `NightLight` can hold `Box<dyn Gamma>`.

**clipboard** — `trait Selection` is `Gamma`'s counterpart, implemented in `glimpse-panel`. History is
**in memory only**, and an entry's `id` fingerprints `(kind, content)` rather than the mime spelling,
so dedup alone closes the echo loop with no suppression list or timer.

**color_picker** — runs the `glimpse-picker` command through an injected `Picker`, one pick at a
time, and copies through the same `Selection` the clipboard uses, since the panel outlives the
command.

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

**compositor** — mirrors `glimpse-compositors` into one aggregate state and ten typed commands.
Disabling the last enabled output is refused from the service's own snapshot rather than a round
trip. **Urgency is derived here so every client sees one answer** — a workspace is urgent when the
compositor says so *or* any window on it is, the only way Hyprland works. **keyboard** owns
compositor layouts, so switching one does not resync workspaces or windows.

**calendar** — every occurrence from every source becomes one sorted state value. A local path is
watched, `http(s)` is polled, and a `file://` holding a one-line feed URL does both. **A failure
reason never contains the uri.**

**weather** — one state entry per place watched. Places are not configured, they are **leased**: a
consumer calls the typed watch method and the registration lasts thirty minutes unless renewed, and
not renewing is how you stop; a fresh install sends no outbound request otherwise.
`timeformat=unixtime` is load-bearing, since with `timezone=auto` the provider otherwise returns
naive local ISO strings with no offset, and `observed_at` freezes when the network dies rather than
reflecting the fetch clock.

**printing** — CUPS over IPP, polled rather than mirrored, since the protocol has no subscription
surviving a client restart, and `Cancel-Job`/`Release-Job` on an already-finished job are not failures.

**mpris** — both sources subscribe before they read, so nothing is missed between the two, and there
is no progress timer since the payload carries `position_us`, `position_at` and `rate`.

**notifications** — `Store` holds no publisher and no connection, so bound handling and per-app
clearing are testable without either. **The default action does not spend one of the three button
slots** — it drives the card itself. **`NameTaken` is degraded, not fatal** — dunst, mako or a Plasma
session may already own the name.

**tray** — glimpse takes `org.kde.StatusNotifierWatcher` when free and hosts on whoever holds it when
not; an empty item list is a normal state, never `degraded`. **A taken name is not a broken tray** —
a failed claim registers as a host with the incumbent and follows its registration signals instead.
**The claim lives inside the `NameOwnerChanged` source, not in `start`**, since claiming in `start`
would race the signal that recovers a lost name. **A fresh owner sweeps, because items register once
and never learn they were forgotten**: it announces itself first, then sweeps `ListNames`.

**bluetooth** — one adapter, its devices, a pairing prompt and a confirmation in one state value,
enumerated once per generation and never polled. **BlueZ's `ObjectManager` is at `/`, not
`/org/bluez`** — only `PropertiesChanged` and `Disconnected` use that namespace. **Match a device path
by shape, not by prefix** — connecting adds `dev_XX/fd0` and `sep1`…`sep6` as children a prefix match
would count as phantom devices. **`busy` is cleared by its own command's completion, never by the
property moving** — `Connect()` on an already-connected device returns `AlreadyConnected` with no
state change, which would leave that row spinning forever under a property-based clear. **Pairing ends
connected.**

**network** — devices, access points, saved profiles, active connections and the secret prompt in one
state value, enumerated once per generation. **Only a device a user can act on reaches the model** —
`Managed` and a user-facing `DeviceType`, since `Devices` and `AllDevices` both return bridges too.
**The connected beacon wins** an access-point row regardless of signal, and **NetworkManager stores
every secret and glimpse stores none** — a password typed before a join travels in the profile
`AddAndActivateConnection2` creates, and **`REQUEST_NEW` implies interaction is allowed**, so
`ALLOW_INTERACTION` alone drops every retry.

**audio** — the libpulse bridge (`services/audio/pulse.rs`) owns one OS thread: lock, create the
`Operation`, unlock, await the oneshot its callback completes, since a Pulse callback runs under that
lock. **A PA name is never capped, only displayed.**

**removable** — mirrors UDisks2 into one drive-grouped state, enumerated once and never polled.
**`eject` unmounts this drive's mounted volumes first**, since `Drive.Eject` refuses with
`DeviceBusy` otherwise. **Capacity is a `statvfs` sample** — `Filesystem.Size` is 0 for vfat/exfat.

**kdeconnect** — mirrors `kdeconnectd` into a device list; each action is offered only while its
plugin is loaded. **It never starts the daemon**: the owner is read with `GetNameOwner`, since every
call goes to that unique name, which the bus cannot activate. **The daemon emits no
`PropertiesChanged`**, so a match rule marks it stale instead.

**places** — reads four filesystem sources with no bus: `user-dirs.dirs`, `gtk-3.0/bookmarks`,
`$XDG_RUNTIME_DIR/gvfs`, `Trash/files`. **`user-dirs.dirs` is parsed, never read through
`glib::user_special_dir`**, since that enum is closed to eight entries. **`empty_trash` clears the
home trash only** — `files/`, `info/`, `expunged/` and `directorysizes`, leaving `.Trash-$UID` alone.

**system-monitor** — CPU, memory, swap, disk, network, load average, uptime and (amdgpu only) GPU.
**`Config.enabled` reflects panel placement, not table presence**, and **`subscriptions()` returns
nothing at all while disabled**, discovery included, since a service with no consumer should do zero
work. **GPU memory reads GTT on an integrated card, VRAM on a discrete one**, classified once at
discovery.

**session actions** — logind capabilities and inhibitors on their own subscription, window count from
the compositor.

**privacy** — camera, microphone, screen capture and location as one state, reporting rather than
enforcing. **The camera source is a `/proc` fd scan gated on the `uvcvideo` refcount, never
PipeWire**, since PipeWire is blind to raw V4L2 capture. **`app: None` is first-class on every
resource, and always the case for location**, since GeoClue has no per-client attribution; it takes
no commands at all.

**exec** — an attached slot owns one external process and one tree. The child speaks first with
`Hello`; every `Hello` resets its tree and advances the generation, so stale UI events cannot reach
a reloaded applet. Placement and options changes go over its stdin without respawning it. A child
that exits before `Hello` stays `Failed` until its catalog entry or instance config changes; a missing entry is checked every five seconds, and one that spoke restarts after
bounded backoff. The source resolves desktop entries off the async worker, spawns on that worker for
PDEATHSIG, and adopts the pid into a transient scope before waiting. **Only a user event marked as a
gesture opens a two-second gate** for copy, URI, session and close-popover requests; notifications
bypass it and are limited to one per second. Invalid JSON, oversized lines, invalid trees, Hello or Commit
floods and a full stdin queue stop only the offending child. Stderr is capped at 20 lines per second
and 512 bytes per line under `$XDG_RUNTIME_DIR/glimpse/applets/<id>.<slot>.<output>.<zone>.<epoch>.<pid>-<start>.log`, rotated to `.1` at
one MiB and deleted on detach or applet ID change.

## Rules

Concrete handles only — no broker, registry or string routing; see `.claude/rules/daemon.md`.
