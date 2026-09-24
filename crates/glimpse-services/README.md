# glimpse-services

The service framework and every service implementation.

A service is one Tokio task owning typed state and typed commands. The runtime owns the select loop
and handlers run serially on `&mut self`; cloneable handles give in-process consumers a snapshot, a
watch receiver, health and command methods. `service.rs`, `context.rs`, `subscription.rs` and
`publisher.rs` are that runtime, endpoint, watch-backed state, sources, health and command
plumbing; `services/` is one module per service.

## The framework

**A service says what state it starts in; nobody else gets to.** `Service::initial_state` is
required and `ServiceRuntime::new` takes the config rather than the state, because `new` builds the
handle and a handle answers `snapshot()` before `run` is called — a deferred state would leak an
`Option` into every consumer. **`Running<S>` is one owned service — spawn, reconfigure, stop.** A
composition root holds one per service and a fixed list of calls, rather than a sender, a token and a
task each. A dropped `Running` cancels its service, so no root writes its own `Drop`. `Running::build`
and `Pending::start` are the halves of `spawn`, kept apart for `glimpse-sunset`, which takes the
D-Bus name and gamma control between them so a duplicate fails at the name rather than at the outputs.

**The unchanged-configuration gate belongs in `run`, never on `ServiceSender`** — see
`.claude/rules/daemon.md` for the gate itself. Senders are cloned and handed out before `run` is
spawned, and a `try_send` onto a full inbox must not record a config that never arrived. Health is
`Starting`, `Running`, `Degraded { reason }` or `Stopped { reason }`. **`Degraded` is a running
service** — it keeps publishing what it can, so a consumer must not dim its values. A producer
stopping altogether reaches a consumer as `Sub::watch`'s closed-producer event, not as a predicate
over health; a flag would be a second, lagging source of one fact.
**`ServiceState::unavailable_reason` is the one mapping from health to what a consumer is told.** It
answers `None` while serving and otherwise why not, totally, so a new variant makes every provider
fail to compile until it decides what to say. The strings are read over D-Bus, so `"starting"` and
`"stopped"` are contract rather than log text; a provider layers its own case with `.or(...)`. Commands
are ordinary Rust variants with typed arguments and command-specific oneshot senders: a handle method
offers one through `ServiceEndpoint::command` and awaits its typed result, and a full or closed inbox
returns `CommandError::Unavailable`.

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
no guard**, because it is not a source — a handler returns before the work is done, and an
abort-on-drop guard would cancel the call it just deferred. Shutdown still stops it through the
cancellation token. A panic inside a source is caught, logged and turned into `degraded`. A source is
where a backend's data gets parsed, which makes it the likeliest place to panic and the least
visible — uncaught, the task stops and the service goes on believing it still has a source.
`Sub::watch` reads a dependency's current value before waiting for changes, so a consumer gets a
complete initial snapshot without a race, then maps a closed producer to an explicit unavailable
event.

### Subscriptions

**`Sub::deadline` waits on the wall clock, not on elapsed time.** A tokio timer runs on
`CLOCK_MONOTONIC`, which does not advance while the machine is suspended, so one sleep of the whole
interval fires late by however long the lid was shut, and an NTP step does the same. The wait is
capped and the remaining time re-derived from `Utc::now()` each pass. **Tearing a timer down does
not unqueue an event it has already emitted**, so an event carries its own deadline and the handler
ignores one that no longer matches.

## The services

**geolocation** — two providers behind one state. `manual` publishes the configured pair. `geoclue`
runs one **transient** client per attempt: created, subscribed to `Location` **before** `Start`
(a client GeoClue already had a fix for delivers it as that subscription's own first item, so
nothing here reads `Location` up front), then always stopped and deleted again — on a fix, an
unreadable change, `FIX_TIMEOUT` elapsing, or the attempt being torn down early by a config change
or the next attempt starting. `Manager.InUse` is a global, session-wide flag, and this is what keeps
it from outliving a single attempt. A tick retries the attempt: `NO_FIX_RETRY` while no fix has ever
landed, `FIX_REFRESH` once one has, so a moving user's fix is retaken without polling while idle.
Coordinates mean **last known**, not **current** — an unreadable change or a lost bus marks the
service `degraded` without touching the cache; only a manual table or a fresh `geoclue` selection
clears it. Accuracy is `CITY`; authorization is a shipped file, `data/geoclue/conf.d/glimpse.conf`,
whose section name and `DESKTOP_ID` must agree. **solar** publishes `phase` and
`next_change`, no color temperature (the night light's to decide); `next_change` is always still
ahead, so a consumer needs no midnight special case. Above the polar circles a date has neither
event: the phase falls back to the sign of the solar declination against the latitude, and
`next_change` is `None`. Without a location it publishes nothing and degrades.

**night light** — the tick's period is **in its own subscription key**, so crossing into a
transition window tears the slow timer down and builds the fast one; the ramp position comes from
the clock either way and the cadence only decides how often it is sampled. One tick a minute is
correct and looks wrong — a 15-minute transition moves in steps of about 150 K. Daylight hands
gamma control back; while serving a non-daylight schedule, each tick reapplies its selected
temperature so a stolen output returns. Release is decided on the solar phase itself, not on the
computed value, since a night ramp can round to the same number as daylight and must not release
early — a manual override is the one exception, since a value pinned by hand at exactly `DAY` is
still a deliberate hold. A `transition-minutes` of zero never asks for the faster tick. Every reader
goes through `effective()` — `forced.unwrap_or(config.schedule)` — so `SetSchedule` is complete
rather than cosmetic; `forced` is not persisted and clears only when `[night-light]` changes.

**battery** — mirrors UPower and power-profiles-daemon. The chip reads `DisplayDevice`; internals
and charge-threshold details come from the real `BAT*` objects, because the composite omits them.
Peripherals are other present UPower devices, not a second BlueZ walk. `TimeTo*` `0`,
`ChargeCycles` `<= 0` and empty serials are absent. No per-app wattage. Commands are `set_profile`
and `EnableChargeThreshold`.

**gamma** — `trait Gamma` is declared here and implemented in `glimpse-sunset`, because this crate
is linked into the panel and every provider and none may gain a Wayland dependency. It is
**synchronous**: the real implementation blocks and says so with `block_in_place`, and a synchronous
signature is dyn-compatible, which lets `NightLight` take `Box<dyn Gamma>`. `FakeGamma` sits beside
it rather than behind `#[cfg(test)]`, because `glimpse-sunset`'s tests are a separate unit.

**clipboard** — `trait Selection` is `Gamma`'s counterpart, implemented in `glimpse-panel`; **every `events()`
call must yield a fresh stream**, because the runtime rebuilds a torn-down source. The history is
**in memory only** and an entry's `id` fingerprints `(kind, content)`, not the mime spelling — so
**dedup closes the echo loop**, with no suppression list and no timer. `is_sensitive` is the whole
password-manager rule, called before content is read; nothing is tombstoned, which would say a
password was copied and when. Both budgets count only unpinned entries, or pins eat the cap.

**color_picker** — runs the `glimpse-picker` command through an injected `Picker`, one pick at a
time, keeps the palette **in memory**, and copies through the same `Selection` the clipboard uses:
the panel is resident, so the copy outlives the command. A cancel is not an error. It reads
`[color-picker]` once, at start, and takes no reload.

**brightness** — `SysfsBacklight` reads `/sys/class/backlight` with `tokio::fs` and writes through
logind. **`current` moves when a command is accepted, `confirmed` when the write lands**; a failed
write rolls `current` back unless a newer value is queued. **`floor` is published**, or a dragged
fader snaps back at the edge. Sources collapse to the highest-preference controller per connector.
A `backlight` uevent names a device to re-read, never a value to trust. The keyboard is
`Kind::Keyboard` via UPower introspect, then `/sys/class/leds`; writes never go to sysfs. Keyboard
ignores `[brightness] minimum`.

`DdcBacklight` (under `brightness/`) talks DDC/CI natively over `/dev/i2c-*` for external monitors
sysfs never sees — VCP 0x10 through `ddc`/`ddc-i2c`, never the `ddcutil` binary, which glimpse
depends on only for the udev rule it installs. **A connector's `ddc` symlink is not where DDC/CI
answers.** Measured on this machine's amdgpu: every DisplayPort connector carries DDC/CI over its
AUX channel, exposed as a connector-owned `i2c-N` child directory whose device name contains
"aux" — the `ddc` symlink instead names a legacy pin-based bus that answers nothing on a real DP
link. Both are structural, neither is a guess, so both are tried, aux first since it is the one
measured to work; a connector with no aux child (an older VGA/DVI/HDMI bus) only ever had the one.
Nothing here is a probe of every `/dev/i2c-*` node, so an SMBus is never touched. **A built-in
panel connector (`eDP`, `LVDS`, `DSI`) is never probed at all**: its brightness belongs to the
backlight, and a single DDC/CI transaction on an eDP link's bus freezes an amdgpu OLED panel on
its last frame until the next modeset. Every call is a
blocking ioctl with protocol-mandated delays and runs on `spawn_blocking`; there is no change signal
for an out-of-band edit (a monitor's own buttons), only an explicit `brightness.refresh`.
`CompositeBacklight` merges it with `SysfsBacklight` behind one `Arc<dyn Backlight>`, routing by a
static `ddc:` id prefix — the two backends own disjoint id namespaces by construction — and its
`type_name` sorts lowest in the
same-connector preference, so a sysfs entry (from `ddcci-backlight`, say) always wins. `[brightness]
ddc` turns it off.

**compositor** — mirrors `glimpse-compositors` into one aggregate state and passes ten typed
commands. **Disabling the last enabled output is refused from the service's own snapshot, not a
round trip** — a disabled output stays listed so it can be switched back on. There is no separate
focus state: a focus change mutates the `focused` flag inside the lists. **The whole snapshot is
re-read on a resync, not the named part** — `Snapshot` fetches every part concurrently and
`Publisher::update` drops an unchanged aggregate, costing one round trip and publishing only what
moved. A resync is a declared source keyed by an attempt counter, so one arriving mid-fetch tears
the in-flight read down, needing no `fetching`/`pending` bookkeeping. `start` reads the backend
from the environment and cannot be tested; `with_backend` takes one so a headless test doesn't
depend on the machine's compositor. **Urgency is derived here so every client sees one answer**: a
workspace is urgent when the compositor says so *or* when any window on it is — the only way
Hyprland works. A focused window's urgency clears locally, since Hyprland's `urgent>>address` only
ever arrives as "became urgent". Workspaces order by output then `index`, falling back to `id`.
**`WindowRef::Pid` is resolved here, not by a backend**, since the snapshot is the only pid-bearing
window list and niri has no focus-by-pid action; several windows resolve to the lowest id, and a
pid with no window is `InvalidArgs`, not worth retrying. **Commands are awaited inline, not
spawned,** in the compositor and keyboard services, keeping a command and its events in order.
**keyboard** owns compositor layouts, so switching one does not resync workspaces or windows;
`[keyboard] remember` is honoured in-memory, and a memoryless window inherits the current layout.

**calendar** — every occurrence from every source as one sorted state value. A local path is
watched, `http(s)` is polled, and a `file://` holding a one-line feed URL is both. Fetching and
expanding are two steps; `set_range` re-expands without a request. `CANCELLED` is dropped.
`source` is the configured id; `calendar` is `name`, or the id when unset. Location and the first
description line stay separate. A join URL is the first `http`/`https` of
`X-GOOGLE-CONFERENCE`, `X-MICROSOFT-SKYPETEAMSMEETINGURL`, `URL`, a Meet/Zoom/Teams/Webex
`LOCATION`, then the same hosts inside `DESCRIPTION`. Guests publish only when there are two
attendees. **A failure reason never contains the uri.** Length is capped; `DURATION` is read when
`DTEND` is absent; `webcal://` is `https://`; the poll floor is sixty seconds.

**weather** — one state entry per place watched. **Places are not configured; they are leased**: a
consumer calls the typed watch method and the registration is honoured for thirty minutes unless
asked for again. There is no `forget`; not renewing is how you stop.

- **With nothing leased, nothing happens.** A fresh install sends no outbound request, and a
  user's coordinates never leave the machine until asked — structural, not a default someone can
  flip, which is why `[weather]` has no `follow-location` key.
- **Leases are swept on a watch as well as on a fetch**, since a renewal is the one event that
  still arrives while nothing is being fetched, and **a renewal must not restart the poll**:
  `Sub::interval` starts at `Instant::now()`, so a rebuilt subscription fetches immediately, and
  `generation` bumps only when the resolved coordinates change, never when a command merely arrives.
- **A fix has to move a kilometre to count**, measured against the fix last *accepted*, so drift
  never accumulates into a refetch.
- **`timeformat=unixtime` is load-bearing**: with `timezone=auto` the provider otherwise returns
  naive local ISO strings with no offset, so each place carries `utc_offset_seconds` for a renderer
  to label a time in another timezone. **`observed_at` is the provider's own validity time**, not
  our fetch clock, so it freezes when the network dies — the truthful thing for "updated N minutes
  ago" to say.
- **A failed fetch keeps the last reading and degrades**, and the next tick is the retry rather
  than a loop of its own. **A failure reason never quotes the request**, since the query string
  carries the user's coordinates.
- **Every list is cut in `absorb`, not in the provider that filled it** — `sunlit` fills the sun
  times and `sanitized` caps and bidi-strips the alerts, so a third provider inherits all three;
  hours are the exception, capped inside each provider against the module constant `HOURS`.
- **Sun times are computed, never taken from a provider**, or one fact arrives two ways and
  disagrees at the edges — `crate::sun::events` is shared with solar and returns a nested `Option`,
  the outer for coordinates not on Earth, the inner for a day the sun never crossed the horizon.
- **Conditions are provider-neutral** — a closed `Condition` enum rather than a raw WMO code,
  tagged `#[serde(other)]` so an older panel reads an unknown as `Unknown`, with no `_` arm in any
  renderer, so it grew a variant rather than mapping met.no's sleet onto freezing rain.
- **Alerts are in the shared model before any provider fills them**, as `Vec` under
  `#[serde(default)]` — two spellings of "nothing to report" is one too many, and **a failed
  alerts request is not a failed forecast.** The poll floor is ten minutes, since Open-Meteo
  recomputes every fifteen.

**printing** — CUPS over IPP, polled rather than mirrored, since the protocol has no subscription
surviving a client restart. `Watch::Poll`'s key carries the cadence, its period and a generation
bumped on every config change, so a `server_url` or interval reload rebuilds a source immediately.
`org.cups.cupsd.Notifier` is read only as an undecoded, debounced wake hint — its D-Bus argument
shape is undocumented anywhere in this tree, and nothing here creates a subscription of its own.
`Cancel-Job` on an already-finished job, and `Release-Job` on an already-released one, are not
failures, since a 2s poll racing the job's own completion is the common case; CUPS unreachable
degrades like bluetooth without an adapter, logged once at `debug!`, never per poll or `warn!`.

**mpris** — both sources subscribe before they read, so nothing is missed between the two. **There
is no progress timer**: the payload carries `position_us`, `position_at` and `rate`, and a renderer
interpolates. `last_active` moves when the playback state changes, not when a property does, which
is what makes it a stable tie-break. Ranking is playing before paused before stopped, then
last-active, then a stable id, with ghost suppression and mirror dedup for `playerctld` and
`kdeconnect`. `ignore` is compiled by the service rather than the loader, and a reload that changes
it bumps `attempt` to restart the name watch. **An `Updated` for a bus no longer in `known` is
dropped.** The command surface mirrors MPRIS, not the panel.

**notifications** — `[notifications].suppress` is a daemon-side regex list. `Store` holds no
publisher and no connection, so the bound, `replaces_id` and per-app clearing are testable without
either. A close moves a live notification into history; the list is newest-first and the bound
applies only to read history. **The default action does not spend one of the three button slots** —
it drives the card itself. Progress is clamped at the state boundary, so a GTK progress bar never
sees an invalid fraction. **`NameTaken` is degraded, not fatal**: dunst, mako or a Plasma session
may already own the name. **Signals are emitted from inside the handler rather than a spawn**, and
`invoked` resolves the interface once and emits both from that handle. **An activation token is
rejected, never shortened.** `notify` holds `&mut self`, which is what keeps ids and events in the
same order, and the sender pid is captured because `hdr.sender()` exists only inside the call.
**Do-not-disturb's `until`** is honoured by a subscription rather than a check at read time: while it
is on **and** carries an expiry, one `Sub::deadline` at that instant delivers `DoNotDisturbLapsed` and
clears both fields, so a reader that only looks at `enabled` sees it turn itself off — the expiry is
in the subscription key, because keying on a bare marker would lapse at the wrong instant.

**tray** — glimpse takes `org.kde.StatusNotifierWatcher` when it is free and hosts on whoever holds
it when it is not. Its state is every item in registration order; no bus is `degraded` publishing an
empty list, never a failure to start. **A taken name is not a broken tray.** A failed claim registers
`org.kde.StatusNotifierHost-<pid>` with the incumbent, reads its `RegisteredStatusNotifierItems` and
follows its two registration signals; only a watcher refusing us as a host is `degraded`. Entries
resolve to their unique owner — the spelling `NameOwnerChanged` reports — and that list is read whole
on each signal, never reconciled entry by entry. **The claim lives inside the `NameOwnerChanged`
source, not in `start`**, because a source is installed only *after* `start` returns, so claiming
there races the signal that recovers a lost one; all three triggers call the same `claim`, off the
handler and one at a time, and `NameTaken` *by us* is success. **A fresh owner sweeps, because
items register once and never learn they were forgotten**: announce `StatusNotifierHostRegistered`
*first* — Qt and libayatana clients re-register on it — then sweep `ListNames`, the canonical key
collapsing the two arrivals. **One `Watch::Item(key)` per item, and no teardown code** — a key
stops appearing, its guard drops, its match rules go. A menu follower is keyed by item *and path*,
and an item that stops answering is dropped, not retried. **Every command is answered off the
handler under a five-second deadline.** `AboutToShow` reports whether the layout changed and
**must be awaited**; `Event` is `no_reply` and must not be. Menus load on pointer-enter, naming
every submenu id first.

**bluetooth** — one adapter, its devices, a pairing prompt and a confirmation in one state value,
enumerated with one `GetManagedObjects` per generation and never polled. **BlueZ's `ObjectManager`
is at `/`, not `/org/bluez`** — the root answers `UnknownMethod` and its signals come from `/`, so
only `PropertiesChanged` and `Disconnected` take the `/org/bluez` namespace. **One
`PropertiesChanged` stream serves every device**, and every decoder returns an all-`Option` partial
the service merges. A signal arriving after a generation bump but before its enumeration is dropped,
or the previous owner's queue edits the new snapshot. **Match a device path by shape, not by
prefix** — connecting adds `dev_XX/fd0` and `sep1`…`sep6` as children, and a prefix match invents
seven phantom devices; `fd0` is the `MediaTransport1` the codec. **`busy` is cleared by its own
command's completion, never by the property moving** — `Connect()` on an already-connected device
returns `AlreadyConnected` with no state change, so a property-based clear leaves that row
spinning forever, and `Settled` names the `Busy` it answers, so a superseded command cannot clear
a newer one. Five BlueZ errors are not failures at all — `AlreadyConnected`, `AlreadyExists`,
`InProgress` on a scan, `DoesNotExist`, a stop's `Failed: No discovery started` — and the rest
reach the caller **typed**, as `BluetoothError::Failed(Failure)`: as a string the panel cannot
word it. `failure.rs` matches the name, then the token by its **suffix** (BlueZ spells each reason
once per transport), and `settle` logs both. **Pairing ends connected.** **A scan starts two ways
and stops on six** — `Hold::Timed` takes `scan_timeout`, `Hold::Held` none, and a bluez restart
clears it, the session having died with the daemon. `SetDiscoveryFilter` carries `Transport`
alone: any filter disables the RSSI delta-threshold. **The agent is per-connection**; **a
confirmation is a refusal carrying a request**.

**network** — devices, access points, saved profiles, active connections and the secret prompt in
one state value, enumerated once per generation. **Only a device a user can act on reaches the
model** — `Managed` and a user-facing `DeviceType`, since `Devices` and `AllDevices` return one list
with the bridges and veths in it. **An allowlist decides what wakes the service.** **Access points
dedup by SSID to the strongest whole one**, never merging fields, but **the connected beacon wins**.
**A state reason is cached, read at teardown and evicted with the connection**, because the useful
one arrives before state 4 and a neutral one with it; **a failure is filed under the SSID** a beacon
row reads, and **a reason `failure.rs` does not recognise is `Unknown`, never `Ok`**.
**NetworkManager stores every secret and glimpse stores none.** A password typed before a join
travels in the profile `AddAndActivateConnection2` creates, because NetworkManager drops the working
connection the moment activation is requested. The agent answers the rest **in the shape each
setting expects** — `vpn.secrets` is `a{ss}` keyed by the hint, a WEP key is `wep-key0` — and
**`REQUEST_NEW` implies interaction is allowed**, so `ALLOW_INTERACTION` alone drops every retry.
Only `802-11-wireless-security` and `vpn` are serviced, **one prompt is open at a time**, **a
cancellation names its connection and setting**, and **`SaveSecrets` and `DeleteSecrets` are
refused**: NetworkManager then offers them to an agent that has a store.
**A saved profile answers for a beacon only when its `key-mgmt` can join it**, then by a seen BSSID,
then `timestamp`, then lowest path: the wrong one fails without asking for a password. `owe` needs
no secret and **802.1X is refused rather than a PSK profile**. **Commands go through the adapter
carrying the connection**, **an active connection answers for its devices** so a wired row
disconnects, and **a wired row activates by device**, NetworkManager choosing the profile. **A VPN
reads its state under the active path**, **an address comes from the device's `IP4Config`**, **a
radio write publishes once taken** and **busy is cleared by `Settled`**. **Both take
`Dependencies { agent }`: `agent: false` is mirror-only mode, and neither agent ever registers.**

**audio** — the libpulse bridge (`services/audio/pulse.rs`) owns one OS thread: lock, create the
`Operation`, unlock, await the oneshot its callback completes off the lock, since a Pulse callback
runs under that lock. **A PA name is never capped**, only displayed — it is a `Device`'s key and
the literal `set_default_sink`/`set_default_source` argument. **The service never calls
`pulse::connect()` from `start`** — the connection is `Sub::stream(Watch::Pulse(generation), …)`,
so a `Gone` bumps the generation and the runtime swaps in a fresh source; bumping only the
receiver would leave the dead bridge thread in place and audio deaf until a restart. A command
resolves its `DeviceId`/`AppId` against the service's own last snapshot, never the backend, and an
id that has just disappeared is `Refused`, not a panic. `set_app_volume` fans out through
`Role::scaled` rather than one absolute write, so a group's streams keep their relative mix.

**removable** — mirrors UDisks2 into one drive-grouped state, enumerated once with
`GetManagedObjects` and never polled; `mount`, `unmount` and `power_off` are thin pass-throughs.
**`eject` is the one that is not: it unmounts this drive's mounted volumes first**, because
`Drive.Eject` refuses with `DeviceBusy` while any filesystem is mounted and takes no option to
unmount, so the pass-through fails on exactly the drive the user has finished with. A failed unmount
is reported as itself and the eject never runs; `NotMounted` and `AlreadyUnmounting` are races and
are stepped over. **Capacity is a `statvfs` sample, not a UDisks2 property** — `Filesystem.Size` is 0
for vfat and exfat, the two commonest removable filesystems, so free space comes from
`rustix::fs::statvfs` in `spawn_blocking`, on an interval declared only while something is mounted.

**kdeconnect** — mirrors `kdeconnectd` into a device list; ring, ping, send-clipboard, share, browse
(`sftp.startBrowsing`, whose `false` is a failure), open-SMS, pair, unpair and discover are one call
each, and each is offered only while its plugin is loaded. **It never starts the daemon**: the owner is read with
`GetNameOwner`, and every call — `GetAll` included — goes to that unique name, which the bus cannot
activate. **The daemon emits no `PropertiesChanged`**, only its own Qt signals, so one match rule
on the owner under `/modules/kdeconnect` marks a device stale and a list signal marks the list stale.
**That stream never awaits a round trip**: a zbus match queue that fills stops the shared
connection reading, so the source only classifies signals and the service does the fetching — one
fetch per device and one list at a time, with a signal arriving mid-fetch queuing exactly one more.
Every result carries its generation, and one from a daemon that has since gone is dropped. No
daemon is `running` with `running: false` in the state, not `degraded`: most users have none. A device's plugins, battery and actions exist only while it is
paired and reachable; `send_clipboard` is offered only while the daemon's own clipboard sync is off.

**places** — reads four filesystem sources with no bus at all: `user-dirs.dirs`,
`gtk-3.0/bookmarks`, `$XDG_RUNTIME_DIR/gvfs`, `Trash/files`. **`user-dirs.dirs` is parsed, never
read through `glib::user_special_dir`** — the key set is open, the enum is closed to eight, and the
GLib function caches besides. **A source that fails to read publishes nothing for its section and
reports through `ctx.degraded`** — health is orthogonal to state, and nothing downstream renders a
degraded section differently from an absent one.

**system-monitor** — CPU, memory, swap, disk, network, load average, uptime and (amdgpu only) GPU.
**`Config.enabled` reflects panel placement, not table presence** — `glimpse_config::placed_kinds`
resolves every zone entry the same table-then-`Applet::from_name` way a panel itself does, so `right
= ["system-monitor"]` with no `[applets.system-monitor]` table still counts as demand and a table
nobody placed does not. **`subscriptions()` returns nothing at all while disabled**, discovery
included — a GPU and CPU-temp probe are themselves sysfs reads, and the whole point of demand gating
is that a service with no consumer does zero work. **Sampling runs inside the `Sub::interval` tick
itself**, one `spawn_blocking` doing every `/proc` and sysfs read with `std::fs`, never behind
`ctx.spawn_detached` — that would let a sample already in flight publish after the service goes
disabled, and ten small `tokio::fs` reads would cost ten blocking-pool hops against this one.
Disk sampling is its own interval, keyed on `(period, paths)` so either changing restarts it, and a
hung network mount only stalls disk tiles. **GPU and CPU-temp discovery is lazy and cached**, run
once through a generation-keyed one-shot stream on the disabled→enabled transition (and again on a
live `gpu` flip), its result shared with the already-running sample interval through
`Arc<Mutex<Discovery>>` so a discovery that lands after the interval was built still reaches the very
next tick. **CPU and network read `None` until a delta exists** — the state published before the
first tick since (re)enabling has no rate to report, never a fabricated zero. Every counter
subtraction is `checked_sub`; a decrease (a reset, a vanished interface) yields no rate that tick,
never a panic or an underflow wrap. **GPU memory reads GTT on an integrated card, VRAM on a discrete
one** — classified once at discovery by comparing `mem_info_vram_total` against
`mem_info_gtt_total`; an APU's VRAM is a small carve-out that reads misleadingly full at idle.
`gpu_busy_percent` is skipped while `power/runtime_status` says `suspended`, so polling never itself
wakes a runtime-suspended discrete GPU.

**session actions** — logind capabilities, same-seat sessions and inhibitors on their own
subscription, window count from the compositor, PackageKit updates only on `UpdatesChanged`. A
window appearing does not re-query the package manager. Capability reasons are an enum; the applet
formats them.

**privacy** — camera, microphone, screen capture and location as one state, reporting rather than
enforcing. **The camera source is a `/proc` fd scan gated on the `uvcvideo` refcount, never
PipeWire** — PipeWire is blind to raw V4L2 capture. **The mic source excludes a corked capture, a
monitor source and an app on the volume-control blocklist by app id only**, since a name is
attacker-controlled. **`app: None` is first-class on every resource, and always the case for
location** — GeoClue exposes no per-client attribution. **The service takes no commands at all**
(`type Command = Infallible`): it reports who is using a device and never intervenes. Muting a
microphone belongs to the audio applet, which owns that state; a camera cannot be taken back from
the process holding it, and a `WlrScreencopy` cast carries no session id to stop.

## Rules

Concrete handles only — no broker, registry or string routing; see `.claude/rules/daemon.md`.
