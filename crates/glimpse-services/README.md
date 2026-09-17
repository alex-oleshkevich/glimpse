# glimpse-services

The service framework and every service implementation.

A service is one Tokio task owning typed state and typed commands. The runtime owns the select loop
and handlers run serially on `&mut self`; cloneable handles give in-process consumers a snapshot, a
watch receiver, health and command methods.

`service.rs`, `context.rs`, `subscription.rs` and `publisher.rs` are the runtime, endpoint,
watch-backed state, sources, health and command plumbing; `services/` is one module per service.

## The framework

**A service says what state it starts in; nobody else gets to.** `Service::initial_state` is
required and `ServiceRuntime::new` takes the config rather than the state, because `new` builds the
handle and a handle answers `snapshot()` before `run` is called — a deferred state would leak an
`Option` into every consumer.

**`Running<S>` is one owned service — spawn, reconfigure, stop.** A composition root holds one per
service and a fixed list of calls, rather than a sender, a token and a task each. A dropped
`Running` cancels its service, so no root writes its own `Drop`. `Running::build` and
`Pending::start` are the halves of `spawn`, kept apart for `glimpse-sunset`, which takes the D-Bus
name and gamma control between them so a duplicate fails at the name rather than at the outputs.

**The unchanged-configuration gate belongs in `run`, never on `ServiceSender`** — see
`.claude/rules/daemon.md` for the gate itself. Senders are cloned and handed out before `run` is
spawned, and a `try_send` onto a full inbox must not record a config that never arrived.

Health is `Starting`, `Running`, `Degraded { reason }` or `Stopped { reason }`. **`Degraded` is a
running service** — it keeps publishing what it can, so a consumer must not dim its values. A
producer stopping altogether reaches a consumer as `Sub::watch`'s closed-producer event, not as a
predicate over health; a flag would be a second, lagging source of one fact.

**`ServiceState::unavailable_reason` is the one mapping from health to what a consumer is told.** It
answers `None` while serving and otherwise why not, totally, so a new variant makes every provider
fail to compile until it decides what to say. The strings are read over D-Bus, so `"starting"` and
`"stopped"` are contract rather than log text; a provider layers its own case with `.or(...)`.

Commands are ordinary Rust variants with typed arguments and command-specific oneshot senders. A
handle method offers one through `ServiceEndpoint::command` and awaits its typed result; a full or
closed inbox returns `CommandError::Unavailable`.

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
cancellation token.

A panic inside a source is caught, logged and turned into `degraded`. A source is where a backend's
data gets parsed, which makes it the likeliest place to panic and the least visible — uncaught, the
task stops and the service goes on believing it still has a source. `Sub::watch` reads a
dependency's current value before waiting for changes, so a consumer gets a complete initial
snapshot without a race, then maps a closed producer to an explicit unavailable event.

### Subscriptions

**`Sub::deadline` waits on the wall clock, not on elapsed time.** A tokio timer runs on
`CLOCK_MONOTONIC`, which does not advance while the machine is suspended, so one sleep of the whole
interval fires late by however long the lid was shut, and an NTP step does the same. The wait is
capped and the remaining time re-derived from `Utc::now()` each pass.

**Tearing a timer down does not unqueue an event it has already emitted**, so an event carries its
own deadline and the handler ignores one that no longer matches.

## The services

**geolocation** — two providers behind one state. `manual` publishes the configured pair, `geoclue`
follows GeoClue's `Location`, subscribed **before** `Start` because the first fix can arrive before
that call returns. Accuracy is `CITY`. Authorization is a shipped file,
`data/geoclue/conf.d/glimpse.conf`, whose section name and `DESKTOP_ID` must agree. A missing fix or
refused request leaves the service `degraded` publishing `None`.

**solar** — `phase` and `next_change`, no color temperature (the night light's to decide).
`next_change` is always still ahead, so a consumer needs no midnight special case. Above the polar
circles a date has neither event: the phase falls back to the sign of the solar declination against
the latitude, and `next_change` is `None`. Without a location it publishes nothing and degrades.

**night light** — the tick's period is **in its own subscription key**, so crossing into a
transition window tears the slow timer down and builds the fast one; the ramp position comes from
the clock either way and the cadence only decides how often it is sampled. One tick a minute is
correct and looks wrong — a 15-minute transition moves in steps of about 150 K — but each tick is a
gamma *apply*, so the two constants are pinned against each other by a test. A `transition-minutes`
of zero never asks for the faster tick. Every reader goes through `effective()` —
`forced.unwrap_or(config.schedule)` — so `SetSchedule` is complete rather than cosmetic; `forced` is
not persisted and clears only when `[night-light]` changes.

**gamma** — `trait Gamma` is declared here and implemented in `glimpse-sunset`, because this crate
is linked into the panel and every provider and none may gain a Wayland dependency. It is
**synchronous**: the real implementation blocks and says so with `block_in_place`, and a synchronous
signature is dyn-compatible, which lets `NightLight` take `Box<dyn Gamma>`. `FakeGamma` sits beside
it rather than behind `#[cfg(test)]`, because `glimpse-sunset`'s tests are a separate unit.

**compositor** — mirrors `glimpse-compositors` into one aggregate state and passes eight typed
commands. There is no separate focus state: a focus change mutates the `focused` flag inside the
lists. **The whole snapshot is re-read on a resync, not the named part** — `Snapshot` fetches every
part concurrently and `Publisher::update` drops an unchanged aggregate, so it costs one round trip
and publishes only what moved. A resync is a declared source keyed by an attempt counter, so one
arriving mid-fetch tears the in-flight read down; that is the coalescing, and it needs no
`fetching`/`pending` bookkeeping. `start` reads the backend out of the environment and so cannot be
used from a test; `with_backend` takes one, which is what a headless test calls so its assertions do
not depend on the machine's compositor.

**Urgency is derived here so every client sees one answer**: a workspace is urgent when the
compositor says so *or* when any window on it is, which is what makes Hyprland work at all. A
focused window's urgency is cleared locally, because Hyprland's `urgent>>address` only ever arrives
as "became urgent". Workspaces order by output then `index`, falling back to `id`.

**`WindowRef::Pid` is resolved here, not by a backend**, because the snapshot is the only place
holding a pid-bearing window list and niri has no focus-by-pid action at all. Several windows
resolve to the lowest id, the list being edited in place; a pid with no window is `InvalidArgs`,
which does not invite a retry.

**Commands are awaited inline rather than spawned** in the compositor and keyboard services, which
keeps a command and the events it causes in order.

**keyboard** — owns compositor layouts so a layout switch does not resync workspaces and windows.
`[keyboard] remember` is honoured here, in-memory, and a window with no memory inherits the current
layout.

**calendar** — every occurrence from every source as one sorted state value. **Whether a source is
watched or fetched follows the uri, not the kind**: a local path is watched, `http(s)` is polled,
and a `file://` holding a one-line feed URL is both, which is why `resolve` reads the file.

A watched source has no timer: the stream opens with a read, `Update::Unavailable` is a failure not
a warning, and `poll-interval` describes only the network.

- **Fetching and expanding are two steps, and only the first touches the world**, so `set_range`
  changes what is published without a request, and an unparseable document is a reported failure
  rather than a source that expands to nothing. **Re-expansion is a subscription**, keyed on a
  `generation` and run on `spawn_blocking`.
- **Every instant from a client is added to with `checked_add_signed`** — `DateTime + TimeDelta`
  panics on overflow — and **an entry's length is capped**, because a surface walks the days it
  covers one at a time and a `DURATION` of `P9999Y` is millions of GTK main-loop iterations.
- **An entry may carry `DURATION` instead of `DTEND` and `icalendar` does not surface it** —
  `get_end()` returns `None`, reading as a zero-length event. `DTEND` wins where both appear.
- **Truncation is reported, not silent**, and the cap belongs to the merged payload: capping each
  source would publish more than the cap. **`webcal://` is rewritten before the url is parsed**, as
  a case-insensitive string swap — `Url::set_scheme` refuses a non-special to special change.
- **A dead watch never overwrites a failed read** — `Unwatched` arrives second and fills in with
  `entry().or_insert()`. **A failure reason never contains the uri**: an iCalendar URL is a token.
- **Text off a feed is cleaned against bidi, not only control characters.** `char::is_control` is Cc
  alone, so `U+202A..=U+202E` and `U+2066..=U+2069` pass it, and Pango honours both.
- **The poll interval has a floor of sixty seconds**, because `Duration::from_secs(0)` panics
  `tokio::time::interval`. Two sources sharing an id share a subscription key, so a duplicate id is
  filtered rather than left to overwrite the first's events.

**weather** — one state entry per place watched. **Places are not configured; they are leased**: a
consumer calls the typed watch method and the registration is honoured for thirty minutes unless
asked for again. There is no `forget`; not renewing is how you stop.

- **With nothing leased, nothing happens.** A fresh install issues no outbound request and the
  user's coordinates never leave the machine until a consumer asks. That is structural rather than a
  default someone can flip, which is why `[weather]` has no `follow-location` key.
- **Leases are swept on a watch as well as on a fetch**, because a renewal is the one event that
  still arrives while nothing is being fetched.
- **A renewal must not restart the poll.** `Sub::interval` starts at `Instant::now()`, so a rebuilt
  subscription fetches immediately; `generation` is bumped when the resolved coordinate set changes,
  never when a command merely arrives.
- **A fix has to move a kilometre to count**, measured against the fix last *accepted*, so drift
  never accumulates into a refetch.
- **`timeformat=unixtime` is load-bearing**: with `timezone=auto` the provider otherwise returns
  naive local ISO strings with no offset. Each place carries `utc_offset_seconds`, without which a
  renderer cannot label a time for a place in another timezone.
- **`observed_at` is the provider's own validity time, not our fetch clock**, so it freezes when the
  network dies — the truthful thing for "updated N minutes ago" to say.
- **A failed fetch keeps the last reading and degrades.** There is no retry loop: the next tick is
  the retry. **A failure reason never quotes the request**, because the query string carries the
  user's coordinates.
- **Every list is cut in `absorb`, not in the provider that filled it** — `sunlit` fills the sun
  times and `sanitized` caps and bidi-strips the alerts, so a third provider inherits all three.
  Hours are the exception, capped inside each provider against the module constant `HOURS`.
- **Sun times are computed, never taken from a provider**, or one fact arrives two ways and
  disagrees at the edges. `crate::sun::events` is shared with solar and returns a nested `Option` on
  purpose: the outer is coordinates not on Earth, the inner a day the sun did not cross the horizon.
- **Conditions are provider-neutral** — a closed `Condition` enum rather than a raw WMO code, tagged
  `#[serde(other)]` so an older panel reads an unknown as `Unknown`, with no `_` arm in any renderer
  so the compiler names every site that must decide. It grew a variant rather than mapping met.no's
  sleet onto freezing rain.
- **Alerts are in the shared model before any provider fills them**, as `Vec` under
  `#[serde(default)]`: two spellings of "nothing to report" is one too many. **A failed alerts
  request is not a failed forecast.**
- **The poll interval has a floor of ten minutes**, because Open-Meteo recomputes every fifteen.

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

**do not disturb** — `until` is honoured by a subscription rather than a check at read time: while
it is on **and** carries an expiry, one `Sub::deadline` at that instant delivers `DoNotDisturbLapsed`
and clears both fields, so a reader that only looks at `enabled` sees it turn itself off. The expiry
is in the subscription key; keying on a bare marker would lapse at the wrong instant.

**tray** — glimpse takes `org.kde.StatusNotifierWatcher` when it is free and hosts on whoever holds
it when it is not. Its state is every item in registration order; no bus is `degraded` publishing an
empty list, never a failure to start.

**A taken name is not a broken tray.** A failed claim registers `org.kde.StatusNotifierHost-<pid>`
with the incumbent, reads its `RegisteredStatusNotifierItems` and follows its two registration
signals; only a watcher refusing us as a host is `degraded`. Entries resolve to their unique owner —
the spelling `NameOwnerChanged` reports — and that list is read whole on each signal, never
reconciled entry by entry.

**The claim lives inside the `NameOwnerChanged` source, not in `start`.** A source is installed only
*after* `start` returns, so claiming there races the signal that recovers a lost one. All three
triggers call the same `claim`, off the handler and one at a time; `NameTaken` *by us* is success.

**A fresh owner sweeps, because items register once and never learn they were forgotten.** Announce
`StatusNotifierHostRegistered` *first* — Qt and libayatana clients re-register on it — then sweep
`ListNames`, the canonical key collapsing the two arrivals.

**One `Watch::Item(key)` per item, and no teardown code** — a key stops appearing, its guard drops,
its match rules go. A menu follower is keyed by item *and path*, and an item that stops answering is
dropped, not retried.

**Every command is answered off the handler under a five-second deadline.** `AboutToShow` reports
whether the layout changed and **must be awaited**; `Event` is `no_reply` and must not be. Menus
load on pointer-enter, naming every submenu id first.

**bluetooth** — one adapter, its devices, a pairing prompt and a confirmation in one state value,
enumerated with one `GetManagedObjects` per generation and never polled. **BlueZ's `ObjectManager`
is at `/`, not `/org/bluez`** — the root answers `UnknownMethod` and its signals come from `/`, so
only `PropertiesChanged` and `Disconnected` take the `/org/bluez` namespace. **One
`PropertiesChanged` stream serves every device**, and every decoder returns an all-`Option` partial
the service merges. A signal arriving after a generation bump but before its enumeration is dropped,
or the previous owner's queue edits the new snapshot.

**Match a device path by shape, not by prefix.** Connecting adds `dev_XX/fd0` and `sep1`…`sep6` as
children; a prefix match invents seven phantom devices. `fd0` is the `MediaTransport1` the codec.

**`busy` is cleared by its own command's completion, never by the property moving.** `Connect()` on
an already-connected device returns `AlreadyConnected` with no state change, so a property-based
clear leaves that row spinning forever; `Settled` names the `Busy` it answers, so a superseded
command cannot clear a newer one. Five BlueZ errors are not failures at all — `AlreadyConnected`,
`AlreadyExists`, `InProgress` on a scan, `DoesNotExist`, a stop's `Failed: No discovery started` —
and the rest reach the caller **typed**, as `BluetoothError::Failed(Failure)`: as a string the panel
cannot word it. `failure.rs` matches the name, then the token by its **suffix** (BlueZ spells each
reason once per transport), and `settle` logs both. **Pairing ends connected.**

**A scan starts two ways and stops on six** — `Hold::Timed` takes `scan_timeout`, `Hold::Held` none,
and a bluez restart clears it, the session having died with the daemon. `SetDiscoveryFilter` carries
`Transport` alone: any filter disables the RSSI delta-threshold. **The agent is per-connection**;
**a confirmation is a refusal carrying a request**.

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
radio write publishes once taken** and **busy is cleared by `Settled`**.

**audio** — the libpulse bridge (`services/audio/pulse.rs`) owns one OS thread: lock, create the
`Operation`, unlock, await the oneshot its callback completes off the lock, since a Pulse callback
runs under that lock. **A PA name is never capped**, only displayed — it is a `Device`'s key and
the literal `set_default_sink`/`set_default_source` argument.

## Rules

Concrete handles only — no broker, registry or string routing; see `.claude/rules/daemon.md`.
