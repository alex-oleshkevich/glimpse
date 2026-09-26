# glimpse-idle

The idle daemon: it owns `[idle]`, drives `ext-idle-notify-v1` notifications per configured
listener, and runs each listener's `on_idle`/`on_resume` script, following AC/battery state from
UPower to choose between `profiles.ac` and `profiles.battery`. The default profiles blank the
screens with `glimpse-dpms off` and wake them with `glimpse-dpms on`, which Hyprland needs and niri
tolerates.

## Contents

- `main.rs` — `run(cli) -> anyhow::Result<()>`; `main` maps the outcome to an `ExitCode`
- `errors.rs` — exit codes and the single `downcast_ref` mapping an error to one
- `cli.rs` — argument surface, flattening shared structs from `glimpse-utils`
- `services.rs` — composition root: takes `me.aresa.Glimpse.Idle`, serves `Idle1Server`, starts
  every other task, wires config reloads into the actor
- `idle.rs` — listener state machine: resolves `ActiveListener`s from the active profile, tracks
  fired listeners, runs their commands
- `wayland_notify.rs` — the `ext-idle-notify-v1` client; reports a stalled compositor as `Degraded`
- `inhibitors/registry.rs` — `Registry`, the shared capacity-bounded inhibitor store
- `inhibitors/health.rs` — `Health`, one `InhibitorsHealth` per backend slot, plus the generation
  behind `Idle1.Health`
- `inhibitors/shared.rs` — `SharedRegistry`, the `Arc`+`Mutex` wrapper every surface shares
- `inhibitors/control.rs` — `Idle1Server`, serving `me.aresa.Glimpse.Idle1` (`Hold`/`Release`,
  `Inhibitors`/`Health`)
- `inhibitors/screen_saver.rs` — `ScreenSaverServer`, serving `org.freedesktop.ScreenSaver`
  (`Inhibit`/`UnInhibit` only) and auto-releasing a departed client's records
- `inhibitors/login1_observer.rs` — polls `Login1Manager.ListInhibitors`, mirroring external
  inhibitors into the registry as read-only records
- `inhibitors/portal.rs` — `PortalInhibit` (`org.freedesktop.impl.portal.Inhibit`), `PortalRequest`
  (releases its record on `Close()`), `PortalSession` (the monitor session `CreateMonitor` returns)

## Design

**`org.freedesktop.ScreenSaver` serves only `Inhibit`/`UnInhibit`.** The rest of the interface —
`GetActive`, `Lock`, `SimulateUserActivity`, etc. — describes a screensaver this daemon doesn't own;
an app calling one of those gets `UnknownMethod`, since only the two implemented members are what
browsers and media players actually call.

**The portal `Inhibit` interface is served whole, because `glimpse-portals.conf` claims it whole.**
Declaring `org.freedesktop.impl.portal.Inhibit=glimpse` also routes `CreateMonitor` and
`QueryEndResponse` here, so implementing only `Inhibit` would error every app asking to monitor
session state. `CreateMonitor` emits one `StateChanged` with `session-state: 1`.
**`screensaver-active` is deliberately absent** — the key is optional and the lock screen owns that
state, so answering here would be a guess.

**The D-Bus name `me.aresa.Glimpse.Idle` is taken before any other task starts, and failing to
take it is fatal.** A duplicate `glimpse-idle` must never touch the Wayland, UPower or registry
resources the running one already holds. The system bus (and so `login1`) is independently
optional: only `Hold` needs it, so its absence degrades that one method rather than the whole
control interface. Acquiring `org.freedesktop.ScreenSaver` follows the same ordering but is
non-fatal: `screen_saver::start` runs only after the primary name lands, and records `Ready` or
`Degraded` into a shared health cell `Idle1Server::health()` reads on every call.

**A fired listener is sticky; a suppressed one is not.** `Idled` for a `respect_inhibitors == true`
listener that's currently inhibited goes into `suppressed` rather than firing `on_idle`; the
registry's idle-targeting flag dropping to `false` fires every still-suppressed listener at once,
with no new `Idled` needed. Only a real `Resumed` clears `fired`, so a later inhibitor never
un-fires a listener. Each event also carries the generation it was dispatched under, and any event
whose generation doesn't match the actor's current one is dropped; `replace_policy` clears both
`fired` and `suppressed` on every policy change, since listener ids are per-profile indices reused
across profiles and a stale flag or event would otherwise hit the wrong listener under a new
policy.

**A profile switch or config reload resumes every fired listener before adopting the new listener
set — but only when the resolved policy actually changed.** `Actor::replace_policy` compares the
newly-resolved listener set (and `enabled`, `power_source`) against what's active; a no-op resolve
never disturbs a listener's fired state or timer. A real change runs `on_resume` for everything in
`fired` against the *old* listeners, then clears `fired` and adopts the new set.

**No `glimpse-services` `Service` for AC/battery state** — a whole `Service`/`Ctx` graph for one
`bool` is disproportionate, so this crate talks to UPower directly through a `UPowerProxy`, and
subscribes to `on-battery` before reading its starting value, since zbus's `PropertyStream` doesn't
replay a value that changed before the subscription began. A machine with no
`org.freedesktop.UPower` falls back to the AC profile.

**The actor never awaits a listener's command.** `Actor::spawn_command` fires a `tokio::spawn`ed
task guarded by a per-listener `tokio::sync::Mutex`, so a slow `on_idle` script serializes against
that listener's own next command but never blocks the event loop or any other listener.

**Health is one value with one slot per backend; the actor holds none of it, and the Wayland slot
is load-bearing** — a compositor with no `ext-idle-notify-v1` fires no listener ever, and without
its own health slot that reads exactly like a healthy daemon with nothing to report. Wayland setup
itself runs on a blocking worker under a timeout, and a stalled attempt is never abandoned for a
second one: connect/bind/roundtrip can't be cancelled, so a compositor that accepts the socket but
never answers parks that worker thread; `wayland_notify::run` threads the same `JoinHandle` through
every retry, re-awaiting it rather than replacing it with a fresh `spawn_blocking`. The backend
still connects and retries even when `[idle] enabled = false`.

**`inhibitors::Registry` is pure logic with no D-Bus surface; `SharedRegistry` is what every
surface actually holds.** `Idle1Server`, `ScreenSaverServer`, `login1_observer.rs` and
`portal.rs`'s `PortalInhibit` all share one `Arc<SharedRegistry>` rather than holding a `Registry`
each. `SharedRegistry::mutate` republishes `any_idle_target` with `send_if_modified`, so a mutation
that doesn't touch it never wakes the idle gate's subscriber for nothing.

**`SharedRegistry` also mirrors `Registry::version` as a `generation` signal**, because
`Inhibitors` (the D-Bus property) can change without `any_idle_target` flipping. `version` bumps
only on operations that change what a reader could observe (`insert`, `release_record`,
`set_process_name`) — never on a rejected `Inhibit` or an unknown-cookie `UnInhibit`, or every
session-bus disconnect would broadcast the full `Inhibitors` array. `services::emit_changes` is the
crate's only `Inhibitors` and `Health` `PropertiesChanged` emitter, driven by this signal and by
the login1 observer's health generation.

**A manual `Hold` records itself as `SourceKind::Login1` with `pid == std::process::id()`**, since
the wire contract freezes exactly three source kinds and `Hold` reuses `Login1` with the daemon's
own pid as a sentinel. **This is load-bearing for `login1_observer.rs`**, which polls rather than
subscribes — logind emits no add/remove signal for its inhibitor list — diffing `ListInhibitors`
every 5s keyed by `(pid, who, why)`, and filters out any entry whose pid matches its own before
diffing, or a manual hold would insert a second record for the same fd and show up twice in
`Inhibitors`. `mode = "delay"` is dropped before the diff (it postpones shutdown briefly, not
meaningful inhibition); a recycled pid, or two inhibitors from one process sharing a `who`/`why`
pair, alias to the same key. It never calls `check_capacity`, unlike `Hold` and
`ScreenSaver.Inhibit`: its records mirror inhibitors that already exist outside our control, and
dropping one for being over capacity would make `any_idle_target()` wrongly conclude nothing is
inhibiting the machine.

**`Idle1Server::hold` checks capacity, calls `login1.Inhibit`, then re-checks and inserts** — a
synchronous `mutate` closure can't wrap the outbound call, at the cost of a bounded overshoot equal
to the Holds concurrently in flight. **`PortalInhibit` identifies a caller by `app_id`, not by
D-Bus sender**, since every backend call arrives on xdg-desktop-portal's own connection where
`header.sender()` is identical for every sandboxed app; `check_capacity` matches a record by either
the bus name or the `app_id`. A missing `login1` proxy, or a failed `login1.Inhibit` call, degrades
a portal `Inhibit` to a trackless record rather than refusing it, since `Inhibit` has no return
value in the portal spec.

**`PortalRequest` is a per-call object registered at the caller's own `handle`, removed by its own
`Close()`.** `release_record` and `ObjectServer::remove` both return gracefully on an unknown id or
an already-removed path, so a double `Close()` — or one racing a removal — can't panic the server.
`ObjectServer::at` reports a path already in use as `Ok(false)`, not an `Err`, so `Inhibit` matches
on the bool rather than the result before trusting its own registration.

- `MAX_INHIBITORS_PER_BUS` (32) narrows `MAX_INHIBITORS_TOTAL` (128) per client; both bound only
  bus-identified records.
- `RATE_BURST` (5) / `RATE_REFILL_PER_SEC` (1.0) bound a token bucket per bus name against
  Inhibit/UnInhibit churn; a record with no bus name is never rate-limited.
- `clamp_label(text, cap)` truncates a caller-supplied `who`/`why` through `glimpse_utils::clean`,
  the same function the client side of this wire contract uses. The cap is per field (`IDENTIFIER`
  = 120, `REASON` = 240); a resolved `process_name` is clamped the same way, since
  `/proc/<pid>/comm` is attacker-controlled.
- `Registry::release_record` is the one path that removes a record from every secondary map and
  drops its `logind_fd`; every release trigger must go through it, or an outbound inhibit fd leaks.
