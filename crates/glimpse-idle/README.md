# glimpse-idle

The idle daemon: it owns `[idle]`, drives `ext-idle-notify-v1` notifications per configured
listener, and runs each listener's `on_idle`/`on_resume` script. It follows AC/battery state from
UPower to choose between `profiles.ac` and `profiles.battery`. The default profiles blank the
screens through `glimpse-dpms off` (from `data/bin/`, installed to `/usr/bin`) and wake them with
`glimpse-dpms on`, which Hyprland needs and niri tolerates.

## Contents

- `main.rs` — `run(cli) -> anyhow::Result<()>`, with `main` turning the outcome into an `ExitCode`
- `errors.rs` — the exit codes and the single `downcast_ref` that maps an error onto one
- `cli.rs` — the argument surface, flattening the shared structs from `glimpse-utils`
- `services.rs` — the composition root: takes the `me.aresa.Glimpse.Idle` name and serves
  `Idle1Server` first, then starts every other task and wires config reloads into the actor
- `idle.rs` — the listener state machine: resolves `ActiveListener`s from the active profile, tracks
  which listeners are fired, and runs their commands
- `wayland_notify.rs` — the `ext-idle-notify-v1` client that turns `Idled`/`Resumed` events into
  actor events and reports a stalled compositor as `Degraded` health
- `inhibitors/registry.rs` — `Registry`, the shared capacity-bounded store every inhibitor D-Bus
  surface inserts into and reads from
- `inhibitors/health.rs` — `Health`, the one `InhibitorsHealth` every backend reports its own slot
  of, and the generation behind the `Idle1.Health` property
- `inhibitors/shared.rs` — `SharedRegistry`, the `Arc`+`Mutex`-wrapped `Registry` every D-Bus
  surface shares, plus the `watch::Receiver<bool>` the idle gate subscribes to
- `inhibitors/control.rs` — `Idle1Server`, serving `me.aresa.Glimpse.Idle1` (`Hold`/`Release`, the
  `Inhibitors`/`Health` properties)
- `inhibitors/screen_saver.rs` — `ScreenSaverServer`, serving `org.freedesktop.ScreenSaver`
  (`Inhibit`/`UnInhibit` only) at both `/org/freedesktop/ScreenSaver` and `/ScreenSaver`; also the
  `NameOwnerChanged` disconnect watcher that auto-releases a departed client's records
- `inhibitors/login1_observer.rs` — polls `Login1Manager.ListInhibitors` every 5s and mirrors
  external inhibitors (backup tools, package-manager hooks, `systemd-inhibit` itself) into the
  registry as read-only records
- `inhibitors/portal.rs` — `PortalInhibit`, serving the whole of
  `org.freedesktop.impl.portal.Inhibit` at `/org/freedesktop/portal/desktop`; `PortalRequest`, the
  per-call `Request` object that releases the record on `Close()`; `PortalSession`, the monitor
  session `CreateMonitor` hands back

## Design

**`org.freedesktop.ScreenSaver` is served as `Inhibit`/`UnInhibit` and nothing else.** The
interface has no published schema and the rest of it — `GetActive`, `SetActive`, `Lock`,
`SimulateUserActivity`, `GetActiveTime`, `GetSessionIdleTime` — describes a screensaver this
daemon does not own. An app that introspects finds the interface and gets `UnknownMethod` for
those; that is the deliberate trade, because the two implemented members are what browsers and
media players actually call.

**The portal interface is served whole, because `glimpse-portals.conf` claims it whole.** Declaring
`org.freedesktop.impl.portal.Inhibit=glimpse` routes `CreateMonitor` and `QueryEndResponse` here
too, so implementing only `Inhibit` would hand an error to every app that asks to monitor session
state. `CreateMonitor` serves a real `Session` object and emits one `StateChanged` carrying
`session-state: 1`. **`screensaver-active` is deliberately absent** — the spec makes every key
optional, and the lock screen belongs to `glimpse-lock`, so answering would be a guess.

**There is no rate limiter.** Capacity alone bounds the registry (32 per bus, 128 total) and a
disconnect sweeps a client's records, so throttling only bought smoothing — while a refused
`Inhibit` blanks the screen under a media player that re-inhibits as it plays. Refusing a release
was worse still: the record stranded and suppressed every listener until the client disconnected.

**The D-Bus name is taken before any other task starts.** `own_name` failing is fatal, matching
every other glimpse provider's primary name, and taking it first means a duplicate `glimpse-idle`
never touches the Wayland, UPower or registry resources the running one already holds. The system
bus, and so `login1`, is independently optional: only `Hold` needs it, so its absence degrades that
one method rather than the whole control interface.

**`fire` takes `on_idle` from its caller rather than looking the listener up again.** Both call
sites already hold it, and every id reaching it names a listener in the current active set, because
`replace_policy` clears `fired` and `suppressed` on every path that changes that set.

**A fired listener is sticky, and a suppressed one is not.** The registry's idle-targeting flag
dropping to `false` fires every listener that was idle but suppressed pending that drop, with no new
`Idled` event needed; an inhibitor appearing afterwards never un-fires one, and only a real
`Resumed` clears `fired`. A listener event stamped with a generation older than the actor's current
one is dropped even when its id names a real listener under the new policy, or a stale `Idled` in
flight during a profile switch fires the wrong script.

**One task list, one cancellation token.** `IdleServices` holds `Vec<JoinHandle<()>>` and a single
`CancellationToken`: every task is cancelled together on shutdown, so there is no per-task token to
keep in step. `emit_changes` is the crate's only `Idle1` change emitter, for both `Inhibitors` and
`Health`; no caller emits one itself.

**No `glimpse-services` `Service`.** This crate talks to UPower directly through a `UPowerProxy`
rather than through a registered service, because a whole `Service`/`Ctx` graph for one `bool` is
disproportionate. If a second consumer needs AC/battery state, that is the point to build a shared
mirror in `glimpse-services` — not before.

**`services::start` subscribes to `on-battery` before reading its starting value.** zbus's
`PropertyStream` does not replay a value that changed before the subscription began, so reading
first and subscribing second leaves a window where a flip is silently missed until the *next* one.
One `UPowerProxy` is built, `receive_on_battery_changed()` is called first, then `on_battery()` reads
the starting value — and the same stream is handed to the watch task, rather than a second proxy
building a second subscription. A machine with no `org.freedesktop.UPower` running falls back to
the AC profile rather than failing to start.

**The actor never awaits a listener's command.** `Actor::spawn_command` fires a `tokio::spawn`ed
task guarded by a per-listener `tokio::sync::Mutex`, so a slow `on_idle` script serializes against
that same listener's own next command but never blocks the actor's event loop or any other
listener's transition.

**A profile switch or a config reload resumes every fired listener before adopting the new listener
set — but only when the active policy actually changed.** `Actor::replace_policy` first compares the
newly-resolved `Vec<ActiveListener>` (and `enabled`, and `power_source`) against what's currently
active; an edit that only touches the profile *not* in effect resolves to the same set and is a
no-op, so it never disturbs a listener's fired state or its running timer. Only a real change runs
`on_resume` for everything in `fired` against the *old* resolved listeners, then clears `fired` and
adopts the new set — otherwise a listener that fired under one policy would stay silently fired under
a policy that no longer names it.

**A listener event carries the generation it was registered under.** Listener ids are per-profile
indices, so AC's listener 0 and battery's listener 0 are different listeners with different scripts.
`Backend`'s generation counter is stamped onto every `Idled`/`Resumed` event as it is dispatched, and
`Actor::listener_idle`/`listener_resume` drop any event whose generation does not match the actor's
current one — otherwise a compositor event already in flight when a profile switch or config reload
lands would fire whatever listener now holds that same index under the new policy.

**Health is one value with one slot per backend, and the actor holds none of it.**
`inhibitors/health.rs`'s `Health` owns an `InhibitorsHealth` behind one lock plus the generation
that drives `Idle1.Health`; each backend writes only its own slot through `Backend`. The Wayland
slot is the load-bearing one: a compositor with no `ext-idle-notify-v1` fires no listener ever,
and without a slot of its own that reads on the bus exactly like a healthy daemon with nothing to
report. The panel's idle applet warns on it through `render::unusable`.

**The Wayland setup runs on a blocking worker under a timeout, and a stalled attempt is never
abandoned for a second one.** Connecting, binding the registry and roundtripping is blocking and
cannot be cancelled once started, so a compositor that accepts the socket but never answers leaves
that worker thread parked for good. `wayland_notify::run` threads the same `JoinHandle` through
every retry in `connecting: &mut Option<JoinHandle<..>>`: a timed-out attempt is re-awaited next
time rather than replaced by a fresh `spawn_blocking`, so at most one connect attempt is ever
outstanding. The Wayland backend still connects and retries even when `[idle] enabled = false` —
wasteful but bounded by the same rule, and not solved here. Shutdown against a permanently wedged
compositor cannot complete cleanly either, for the same reason: a `spawn_blocking` task cannot be
aborted once started, so that parked worker thread survives until the process itself is killed —
`TimeoutStopSec` on the systemd unit, once one ships for this crate in `data/`, is what actually
reaps it.

**Only `respect_inhibitors == false` on a v2 `ext_idle_notifier_v1` uses
`get_input_idle_notification`.** Every other listener uses `get_idle_notification`, which already
respects a native Wayland idle-inhibitor by protocol `MUST` — nothing else is needed for that case.
A `respect_inhibitors == false` listener against a v1-only compositor falls back to
`get_idle_notification` with a logged warning, since the hard-cutoff request does not exist there.

**The software gate consults the registry through `suppressed`, kept separate from `fired`.** An
`Idled` event for a `respect_inhibitors == true` listener runs `on_idle` immediately when
`any_idle_target` is false, exactly as before D-Bus inhibitors existed. When it's true, the id goes
into `Actor::suppressed` instead — the event was received, not dropped, but `on_idle` has not run
and the listener is not `fired`. `Actor::set_any_idle_target(false)` then fires every still-active
suppressed id at once, with no new `Idled` required. `fired` stays sticky regardless: a new
inhibitor appearing after a listener has already fired never un-fires it, matching `_old`'s design.
`replace_policy` clears `suppressed` alongside `fired` — listener ids are per-profile indices reused
across profiles, so a suppression left over from the old generation could otherwise fire the new
policy's same-index listener the moment the registry flag next drops, without that listener ever
having received its own `Idled`.

**`inhibitors::Registry` is pure logic with no D-Bus surface of its own; `SharedRegistry` is what
every surface actually holds.** `Idle1Server` (this crate's own `me.aresa.Glimpse.Idle1`),
`ScreenSaverServer`, `login1_observer.rs` and `portal.rs`'s `PortalInhibit` all take the same
`Arc<SharedRegistry>` and mutate or read through it rather than holding a `Registry` of their own.
`SharedRegistry::mutate` republishes its `any_idle_target` `watch::Sender<bool>` with
`send_if_modified` so a mutation that doesn't touch it never wakes the idle gate's subscriber for
nothing; `services::watch_registry` is what turns that `watch::Receiver` into
`Event::RegistryChanged` for the actor.

**`SharedRegistry` carries a second signal, `generation`, that mirrors `Registry::version`.**
`any_idle_target` only fires when that one boolean flips, but `Inhibitors` (the D-Bus property) can
change without it — a second idle-targeting record inserted while one already exists, or a
`process_name` backfilled onto an existing record. `Registry::version` is bumped only by the
operations that change what a reader could observe — `insert`, `release_record`,
`set_process_name` — and by nothing else: `check_capacity`, `mint_id` and `mint_cookie` never
touch it, so a rejected `Inhibit`, an unknown-cookie `UnInhibit`, or a `NameOwnerChanged`
disconnect from a bus name holding nothing all correctly bump neither `version` nor `generation`.
Without that distinction every session-bus disconnect from any app — not just one holding an
inhibitor — would broadcast the full `Inhibitors` array.
`SharedRegistry::mutate` republishes `generation` with `send_if_modified` against the registry's
version, exactly like `any_idle_target`. `services::emit_changes` subscribes to it and, on
every real change, re-fetches the `Idle1Server` object off the connection's `ObjectServer` and
calls its generated `inhibitors_changed` — the crate's *only* `Inhibitors` `PropertiesChanged`
emission site. `screen_saver.rs`'s `Inhibit`, `UnInhibit` and process-name backfill ride it for
free, `control.rs`'s `Hold`/`Release`/auto-release emit nothing of their own any more,
`login1_observer.rs`'s insert/release/rename need no emission path either, and neither does
`portal.rs`'s `Inhibit`/`Request.Close`.

**`ScreenSaverServer` is `Clone` and the same value is registered at both
`/org/freedesktop/ScreenSaver` and `/ScreenSaver`.** Each `ObjectServer::at` call wraps its
argument in its own `Arc`, so the two registrations are independent objects that happen to share
the same `Arc<SharedRegistry>` and `Connection` — mutating through either path is indistinguishable
to a reader of the registry, which is all `Inhibit`/`UnInhibit` need.

**Acquiring `org.freedesktop.ScreenSaver` is non-fatal, unlike `me.aresa.Glimpse.Idle`, and runs
after it.** `services::start` takes `me.aresa.Glimpse.Idle` and serves `Idle1Server` first — same
invariant as before this task, a duplicate `glimpse-idle` must never touch a resource the running
one holds — and only then calls `screen_saver::start`, so a process that loses the race for its own
primary name never gets as far as registering a `ScreenSaver` object or contending for that name
either. `screen_saver::start` registers both objects regardless of whether the name lands, then
attempts `glimpse_dbus::own_name` and records the outcome — `Ready` or `Degraded{"Bus name already
owned"}` — into an `Arc<std::sync::Mutex<BackendHealth>>` built by `services::start` before either
`screen_saver::start` or `Idle1Server::new` run, and handed to both. `Idle1Server::health()` reads
that cell on every call rather than caching it. ScreenSaver and portal acquisition each settle once
during startup, while the login1 observer can transition later; its health generation wakes
`services::emit_changes`, the only `Health` `PropertiesChanged` emitter, so a cached
client refreshes on a logind failure or recovery.

**A manual `Hold` records itself as `SourceKind::Login1` with `pid == std::process::id()` — this
daemon's own pid, not a caller's.** The wire contract freezes exactly three source kinds
(`ScreenSaver`, `Portal`, `Login1`); a fourth for "manual hold" would be an unreviewed wire change,
so `Hold` reuses `Login1` and tags it with the daemon's own pid as a sentinel instead. **This is
load-bearing for `login1_observer.rs`:** it polls `ListInhibitors` and finds the very fd `Hold`
already took out, so it filters out any entry whose `pid == std::process::id()` before diffing —
skipping this would insert a second record for the same fd and every manual hold would show up
twice in `Inhibitors`. `who`/`why` on that record are `"glimpse-idle"`/`"Manual hold"` — matching,
byte for byte, what `Hold` actually passes to `login1.Inhibit`, so the same observer would see one
`who`/`why` pair for the same fd whichever side reported it, if it did not filter the pid out first.

**`login1_observer.rs` polls rather than subscribes, because logind emits no add/remove signal for
its inhibitor list.** Every 5 seconds it calls `Login1Manager.ListInhibitors`, diffs the result
against what it saw last time (keyed by `(pid, who, why)`, since logind hands out no stable id of
its own), and applies the difference to `SharedRegistry`: a new key is inserted with
`can_release = false` (this process never owns the fd, so it can never offer a release action for
one), a vanished key is released, and a still-present key has `/proc/<pid>/comm` re-read and
`set_process_name` called only when the value actually changed. `mode = "delay"` is filtered out
before it ever reaches the diff — a delay inhibitor postpones shutdown by a few seconds for
in-flight cleanup, not meaningful inhibition a user would want surfaced — while `block` and
`block-weak` both pass through unchanged. Every mutation goes through `SharedRegistry::mutate`, so
`Inhibitors`'s `PropertiesChanged` rides the existing `generation` mechanism for free. A failed
`ListInhibitors` degrades the `login1` health slot and retries on the next tick; the interval takes
`MissedTickBehavior::Delay`, so a hung poll delays the next tick rather than firing a backlog of
catch-up ticks the moment it returns. The first failure after a healthy run logs at `warn` and every
further consecutive one at `debug`, so a logind-less machine does not spam the journal forever.

**`login1_observer.rs` accepts three known blind spots.** Cancellation is checked only between
ticks, so `shutdown()` can block for one bus timeout on a hung logind. An inhibitor taken and
released inside one 5-second window is never observed — the cost of polling a backend with no
change signal. And the `(pid, who, why)` key mis-reads a recycled pid as the same inhibitor, and
collapses two inhibitors from one process that share a `who`/`why` pair to whichever
`ListInhibitors` lists first; the rest are dropped rather than overwriting each other in
`observed`, which would leak an untracked, permanently un-releasable record.

**`Idle1Server.login1` is `Option<Login1ManagerProxy>`, independently of the D-Bus name.** Taking
`me.aresa.Glimpse.Idle` is fatal on failure, same as every other glimpse provider's primary name —
`services::start` acquires it before spawning anything else, so a duplicate `glimpse-idle` never
touches the Wayland, UPower or registry resources the running one holds. A missing system bus (or a
`login1` proxy that fails to build) is not fatal: `Inhibitors` and `Release` don't need `login1` at
all, so only `Hold` returns `NotSupported` on a logind-less system, and `login1_observer::start`
degrades `InhibitorsHealth.login1` and returns rather than polling nothing forever — everything else
keeps working.

**`Idle1Server::hold` checks capacity before calling `login1.Inhibit`, then re-checks and inserts
after.** `mutate`'s closure is synchronous, so the outbound call cannot sit inside the same lock;
the reservation and the insert are two calls with it in between. A rejected Hold never reaches
logind, at the cost of a bounded overshoot equal to the Holds concurrently in flight.

**`PortalInhibit` identifies a caller by `app_id`, not by its D-Bus sender.** Every backend call
arrives on xdg-desktop-portal's own connection, so `header.sender()` is the same value for every
sandboxed app and would pool them all together. `check_capacity` counts a record through
`IdleInhibitorRecord::owned_by`, which matches either the bus name or the `app_id` — a portal
record's `bus_name` stays empty, because it has no bus-name identity to release-by-disconnect on
and `Request.Close()` is its only release path.

**A missing `login1` proxy, or a failed `login1.Inhibit` call, degrades to a trackless record rather
than refusing the portal call.** `Inhibit` has no return value in the xdg-desktop-portal spec, so
there is no error path a frontend expects; a suspend/shutdown-targeting inhibitor with no logind fd
behind it is a smaller failure than one silently absent from `Inhibitors` altogether.

**`PortalRequest` is a per-call object registered at the caller's own `handle` and removed by its
own `Close()`.** It stores `handle: OwnedObjectPath` as a field set at construction rather than
recovering it from the connection, since that needs no header/path-injection lookup at all.
`Close()` releases the record through the same `SharedRegistry::mutate` every other surface uses,
then removes itself via `#[zbus(object_server)]`; both `release_record` and `ObjectServer::remove`
already return gracefully on an unknown id or an already-removed path, so a double `Close()` — or
one racing a removal — cannot panic the server. `ObjectServer::at` reports a path already in use
by returning `Ok(false)`, not an `Err`, so a frontend reusing a `handle` is matched on the `bool`,
not the `Result`'s error arm; `Inhibit` releases the record it just inserted for that call rather
than leaving it with no `Request` object ever able to route a `Close()` to it.

- `MAX_INHIBITORS_PER_BUS` (32) caps how many inhibitors a single D-Bus client can hold at once, on
  top of the unconditional `MAX_INHIBITORS_TOTAL` (128) ceiling that bounds every record regardless
  of bus name — the per-bus check narrows the global one for a bus-identified record, it never
  replaces it. **`login1_observer.rs` never calls `check_capacity` at all, unlike `Hold` and
  `ScreenSaver.Inhibit`, and this is deliberate rather than an oversight**: its records mirror
  inhibitors that already exist outside our control, and dropping one because the count is full
  would make `any_idle_target()` — and so the whole software gate — wrongly conclude nothing is
  inhibiting the machine while something genuinely is. A silently *undercounted* mirror is a worse
  failure than an *uncapped* one, so a pathological number of external inhibitors is a cost accepted
  here rather than traded for a false "idle is safe" reading. It can still push `Registry` past
  `MAX_INHIBITORS_TOTAL` in the count a client reads back, which is the one place that number is no
  longer a hard ceiling.
- `RATE_BURST` (5) and `RATE_REFILL_PER_SEC` (1.0) bound a token bucket per bus name against
  Inhibit/UnInhibit churn from one client; a record with no bus name is never rate-limited, since
  nothing identifies it as one client's traffic.
- `clamp_label(text, cap)` truncates a caller-supplied `who`/`why` label through
  `glimpse_utils::clean`, never a hand-rolled byte or char slice — the same function
  `glimpse-dbus/src/clients/idle.rs` already uses to clean these fields on the client side of the
  same wire contract, which also strips control characters and bidi-override/isolate characters
  rather than just capping length. The cap is the caller's to choose per field (`glimpse-dbus`'s
  `IDENTIFIER` = 120 for `who`, `REASON` = 240 for `why`), not a single constant here —
  `inhibitors/mod.rs`'s `WHO_CAP`/`WHY_CAP` mirror that split and are shared by `screen_saver.rs`,
  `login1_observer.rs` and `portal.rs`, and a resolved `process_name` is clamped through the same
  `WHO_CAP` in both, since `/proc/<pid>/comm` is attacker-controlled text a process can set to
  anything via `prctl(PR_SET_NAME)`.
- `Registry::release_record` is the one path that removes a record from every secondary map
  (`cookie_to_id`, `portal_handle_to_id`, `bus_name_to_ids`) and drops its `logind_fd`. Every release
  trigger — UnInhibit, Request.Close, admin release, `NameOwnerChanged`, a login1 diff — must go
  through it, or an outbound logind inhibit fd leaks past the caller's lifetime.
