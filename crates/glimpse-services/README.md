# glimpse-services

The service framework and every service implementation.

A service is one Tokio task owning typed state and typed commands. The runtime owns the select loop;
handlers run serially on `&mut self`, while cloneable handles expose snapshots, watch receivers,
health, and command methods to in-process consumers.

## Contents

- `service.rs`, `context.rs`, `subscription.rs`, `publisher.rs` — the typed runtime, endpoint,
  watch-backed state, dependency sources, health, and command plumbing
- `services/` — one module per service; `weather/` is a directory because it carries two providers

## The framework

**A service says what state it starts in; nobody else gets to.**
`Service::initial_state(&Self::Config)` is required, and `ServiceRuntime::new` takes the config
rather than the state; `run` then takes only the dependencies. The ordering is why the config moves
up rather than the state moving down: `new` builds the handle, and a handle answers `snapshot()`
before `run` is called, so a deferred state would leak an `Option` into every consumer.

**`Running<S>` is one owned service — spawn, reconfigure, stop.** A composition root holds one per
service and a fixed list of calls, rather than a sender, a token and a task each. A dropped
`Running` cancels its service, so no root writes its own `Drop`.

**`Running::build` exists because one binary needs the two halves apart.** `glimpse-sunset` builds
every service, takes the D-Bus name, takes gamma control, and only then starts them, so a duplicate
fails at the name before it touches the outputs the running instance holds. `Pending::start` is the
second half and `spawn` is the two together, so the ordering lives in the types rather than a comment.

**An unchanged configuration never reaches a handler.** `ServiceRuntime::run` keeps the config in
force and skips an `Input::Config` equal to it — no handler call, no `subscriptions()` rebuild, no
`Live::reconcile` diff. Every process reloads the whole document and hands each service its own
slice, so without the gate a service whose table had not moved still wakes, and a service that then
grows its own comparison has put the same decision in two places.

**The gate belongs in `run`, not on `ServiceSender`.** Senders are cloned and handed out *before*
`run` is spawned, so a sender-side record must be seeded against a reload that beat startup, and a
`try_send` failing on a full inbox must not record a config that never arrived.

A service's health is `Starting`, `Running`, `Degraded { reason }` or `Stopped { reason }`.
**`Degraded` is a running service** — it keeps publishing what it can, so a consumer must not dim
its values. That a producer has stopped altogether reaches a consumer as `Sub::watch`'s
closed-producer event rather than as a predicate over health. Do not add a flag-shaped answer to the
same question: it gives consumers a second, lagging source of one fact.

**`ServiceState::unavailable_reason` is the one mapping from health to what a consumer is told.** It
answers `None` while serving and otherwise why not. The match is total, so a new variant makes every
provider fail to compile until it decides what to say. The strings are read over D-Bus, so
`"starting"` and `"stopped"` are contract rather than log text; a provider with a case of its own
layers it with `.or(...)`, which keeps the health reason winning when both apply.

Commands are ordinary Rust variants with typed arguments and command-specific oneshot senders. A
handle method offers the command through `ServiceEndpoint::command` and awaits its typed result;
full or closed inboxes return `CommandError::Unavailable`.

### Sources

Everything reaching a handler arrives from a source, and every source is one `ctx` call returning a
`SourceGuard`. Dropping the guard is the whole cancellation story.

| Source | Produces | For |
| -------------------- | ---------------- | --------------------------------------------------- |
| `ctx.spawn`          | one event        | one unit of async work whose result is an event     |
| `ctx.spawn_detached` | nothing          | work with no result to report                       |
| `ctx.interval`       | an event a tick  | polling, clocks; `at_interval` picks the first tick |
| `ctx.stream`         | many events      | a backend signal stream, a watch, a subscription    |
| `Sub::watch`         | many events      | another service's typed state                       |

`SourceGuard` is `#[must_use]`: `ctx.spawn(...)` written as a statement drops the guard at the
semicolon and aborts the task before it runs. **`spawn_detached` is the one `Ctx` task handing back
no guard**, because it is not a source — a handler returns before the work is done, and an
abort-on-drop guard would cancel the very call it just deferred. Shutdown still stops it through the
cancellation token every spawned task selects against.

A panic inside a source is caught, logged and turned into `degraded`. A source is where a backend's
data gets parsed, which makes it both the likeliest place to panic and the least visible — uncaught,
the task stops and the service goes on believing it still has a source.

`Sub::watch` reads a dependency's current value before waiting for changes, so a consumer gets a
complete initial snapshot without a race, then maps a closed producer to an explicit unavailable
event.

### Subscriptions

A source that should live as long as the service says so is **declared**, not started.
`subscriptions` returns what ought to be running and the runtime diffs it against what is, after
`start` and after every input. The `SubKey` is what restarts a source: put in it everything whose
change should tear the old one down, and nothing whose change should not.

**`Sub::deadline` waits on the wall clock, not on elapsed time.** A tokio timer runs on
`CLOCK_MONOTONIC`, which does not advance while the machine is suspended, so one sleep of the whole
interval fires late by however long the lid was shut; an NTP step does the same. The wait is capped
and the remaining time re-derived from `Utc::now()` each pass. This is the framework's mechanism
rather than any one service's, so the next deadline need not rediscover it.

**Tearing a timer down does not unqueue an event it has already emitted**, so an event carries its
own deadline and the handler ignores one that no longer matches.

## The services

**geolocation** — two providers behind one state. `manual` publishes the configured pair, `geoclue`
follows GeoClue's `Location`. The GeoClue watch is subscribed **before** `Start`, because the first
fix can arrive before that call returns. Accuracy is `CITY`: nothing downstream is sharper.
Authorization is a shipped file, `data/geoclue/conf.d/glimpse.conf`, whose section name and
`DESKTOP_ID` must agree. A missing fix or refused request leaves the service `degraded` publishing
`None`.

**solar** — `phase` and `next_change`, no color temperature (that is the night light's to decide).
`next_change` is always still ahead, so after sunset it names tomorrow's sunrise and a consumer needs
no midnight special case. Above the polar circles a date has neither event, so the phase falls back
to the sign of the solar declination against the latitude and `next_change` is `None`. Without a
location it publishes nothing and degrades — `Day` is not a safe guess at three in the morning.

**night light** — the tick's period is **in its own subscription key**, so crossing into a
transition window tears the slow timer down and builds the fast one; the ramp position is computed
from the clock either way, and the cadence decides only how often it is sampled. One tick a minute
is correct and looks wrong: a 15-minute transition then moves in steps of about 150 K, which reads
as a staircase. Those are gamma *applies* — a ramp table and a blocking compositor roundtrip per
output — so the two constants are pinned against each other by a test. A `transition-minutes` of
zero never asks for the faster tick.

Every reader of the schedule goes through `effective()`, which is `forced.unwrap_or(config.schedule)`
— that is what makes `SetSchedule` complete rather than cosmetic, because `subscriptions` reads the
same answer everything else does. `forced` is not persisted and is cleared only when
`[night-light]` itself changes.

**gamma** — `trait Gamma` is declared here and implemented in `glimpse-sunset`, because this crate
is linked into the panel and every provider and none may gain a Wayland dependency. It is
**synchronous**: the one real implementation blocks and says so with `block_in_place`, and a
synchronous signature is dyn-compatible, which is what lets `NightLight` take `Box<dyn Gamma>`
rather than be generic. `FakeGamma` sits beside the declaration rather than behind `#[cfg(test)]`,
because `glimpse-sunset`'s tests are a separate compilation unit.

**compositor** — mirrors `glimpse-compositors` into one aggregate state and passes eight typed
commands. There is no separate focus state: a focus change mutates the `focused` flag inside the
lists. **The whole snapshot is re-read on a resync, not the named part** — `Snapshot` fetches every
part concurrently and `Publisher::update` drops an unchanged aggregate, so it costs one round trip
and publishes only what moved. A resync is a declared source keyed by an attempt counter, so one
arriving mid-fetch tears the in-flight read down; that is the coalescing, and it needs no
`fetching`/`pending` bookkeeping.

`start` reads the backend out of the environment, so it cannot be used from a test. It builds the
service through `with_backend`, which takes one, and that is what a headless test calls: assertions
about the handler's branches then do not depend on which compositor the machine happens to run.

**Urgency is derived here so every client sees one answer**: a workspace is urgent when the
compositor says so *or* when any window on it is, which is what makes Hyprland work at all. A
focused window's urgency is cleared locally, because Hyprland's `urgent>>address` only ever arrives
as "became urgent". Workspaces are ordered by output then `index` falling back to `id`.

**`WindowRef::Pid` is resolved here, not by a backend.** The snapshot is the only place holding a
pid-bearing window list, and niri has no focus-by-pid action at all while Hyprland's `focuswindow`
takes `pid:` natively — resolving here keeps the two behaving the same. Several windows resolve to
the lowest id, because the list is edited in place and taking the first match would raise a
different one at different moments. A pid with no window is `InvalidArgs`, which does not invite a
retry.

**Commands are awaited inline rather than spawned**, in both the compositor and keyboard services:
awaiting keeps a command and the events it causes in order.

**keyboard** — owns compositor layouts so a layout switch does not resync workspaces and windows;
the compositor service drops the layout events for that reason. `[keyboard] remember` is honoured
here, in-memory for the process, and a window with no memory inherits the current layout.

**calendar** — every occurrence from every source as one sorted state value. **Whether a source is
watched or fetched follows the uri, not the kind**: a local path is watched, `http(s)` is polled,
and a `file://` holding a one-line feed URL is both — which is why the rule cannot be read off the
configuration alone and `resolve` answers it by reading the file.

Three consequences follow from a watched source having no timer: the stream opens with a read, or
it would publish nothing until the first edit; `Update::Unavailable` is a failure rather than a
warning, because there is no second reader; and `poll-interval` describes only the network.

- **Fetching and expanding are two steps, and only the first touches the world**, so `set_range`
  changes what is published without a request. Parsing at fetch keeps an unparseable document a
  reported failure rather than a source that silently expands to nothing.
- **Re-expansion is a subscription**, keyed on a `generation` bumped by anything invalidating the
  list, and runs on `spawn_blocking`.
- **Every instant arriving from a client is added to with `checked_add_signed`.** `DateTime +
  TimeDelta` panics on overflow and `DateTime<Utc>`'s serde accepts an extended year.
- **An entry's length is capped**, because a surface walks the days it covers one at a time — a
  `DURATION` of `P9999Y` is three and a half million iterations in the GTK main loop per update.
- **An entry may carry `DURATION` instead of `DTEND`, and `icalendar` does not surface it.**
  `get_end()` returns `None`, which reads as a zero-length event; several CalDAV exporters write
  them. `DTEND` still wins where both appear.
- **Truncation is reported, not silent**, and the cap belongs to the merged payload: capping each
  source would publish more than the cap, capping only the first would drop a whole calendar.
- **`webcal://` is rewritten before the url is parsed.** `Url::set_scheme` refuses a non-special to
  special change, so it is a case-insensitive string swap through `str::get`.
- **A dead watch never overwrites a failed read** — only the read says the path is wrong, and
  `Unwatched` arrives second, so it fills in with `entry().or_insert()`.
- **Text off a feed is cleaned against bidi, not only against control characters.**
  `char::is_control` is Cc alone, so the overrides `U+202A..=U+202E` and isolates `U+2066..=U+2069`
  pass it — and Pango honours both, which lets a summary reorder the row it lands in.
- **A failure reason never contains the uri.** A provider's iCalendar URL is a bearer token.
- **The poll interval has a floor of sixty seconds**, because `Duration::from_secs(0)` panics
  `tokio::time::interval`. The duplicate-id filter sits beside the clamp: two sources sharing an id
  share a subscription key, so the second never runs while silently overwriting the first's events.

**weather** — one state entry per place being watched. **Places are not configured; they are
leased**: a consumer calls the typed watch method and the registration is honoured for thirty
minutes unless asked for again. There is deliberately no `forget`; not renewing is how you stop.

- **With nothing leased, nothing happens.** A fresh install issues no outbound request and the
  user's coordinates never leave the machine until a consumer asks. That is structural rather than a
  default someone can flip, which is why `[weather]` has no `follow-location` key.
- **Leases are swept on a watch as well as on a fetch**, because a renewal is the one event that
  still arrives when nothing is being fetched — sweeping only on `Fetched` left leases immortal and
  let dead registrations refuse every later watch.
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
  times and `sanitized` caps and bidi-strips the alerts, so a third provider inherits all three by
  reaching the payload the same way. Hours are the exception, capped inside each provider, because
  `HOURS` is a module constant the compiler shows a new provider.
- **Sun times are computed, never taken from a provider**, or one fact arrives two ways and
  disagrees at the edges. `crate::sun::events` is shared with solar and returns a nested `Option` on
  purpose: the outer is coordinates not on Earth, the inner a day the sun did not cross the horizon.
- **Conditions are provider-neutral** — a closed `Condition` enum rather than a raw WMO code, tagged
  `#[serde(other)]` so an older panel reads an unknown as `Unknown`, with no `_` arm in any renderer
  so the compiler names every site that must decide. It grew a variant rather than mapping met.no's
  sleet onto freezing rain.
- **Alerts are in the shared model before any provider fills them**, as `Vec` under
  `#[serde(default)]` — two spellings of "nothing to report" is one more than a renderer should
  branch on. **A failed alerts request is not a failed forecast.**
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
it is on **and** carries an expiry, the service declares one `Sub::deadline` at that instant, which
delivers `DoNotDisturbLapsed` and clears both fields. A reader that only ever looks at `enabled`
therefore sees it turn itself off. The expiry is in the subscription key, so moving it tears the old
timer down; keying on a bare marker would lapse at the wrong instant.

**tray** — glimpse *is* the `org.kde.StatusNotifierWatcher` under niri, because nothing else provides
one. Its state is every registered item in registration order; which of them a bar shows is the
applet's decision, and the three keys live in `[applets.tray]`. No bus is `degraded` publishing an
empty list, never a failure to start. Items decode from one `GetAll` each.

**The claim lives inside the `NameOwnerChanged` source, not in `start`.** A source is installed only
*after* `start` returns, so claiming there races the signal that recovers a lost one, and
`DoNotQueue` makes a failed attempt terminal. The source takes its match rule, then claims, then
follows; cold start, a name freeing and our own loss all call the same `claim`, off the handler and
one at a time — awaiting a name request, a `ListNames` and a probe per candidate on `&mut self` puts
every tray event behind one hung application. `NameTaken` *by us* is success, or a second trigger
degrades a working watcher against itself. The common case is not Plasma: it is the panel restarting
before the old process let go.

**A fresh owner sweeps, because items register once and never learn they were forgotten.** Announce
`StatusNotifierHostRegistered` *first* — Qt and libayatana clients re-register on it — then
`ListNames` for well-known `StatusNotifierItem-*` names, probe each and adopt it; the canonical key
collapses both into one. An item holding only a unique name is unrecoverable, by design.

**One `Watch::Item(key)` per item, and no teardown code** — a key stops appearing, its guard drops,
its match rules go. A menu follower is keyed by item *and path*, so an item that moves its menu stops
following the old one. Each follower takes `PropertiesChanged` **and** every `New*`; the equality
gate makes that safe, and an item that stops answering is dropped rather than retried.

**Every command is answered off the handler under a five-second deadline.** `AboutToShow` reports
whether the layout changed and **must be awaited**; `Event` is `no_reply` and must not be.
`ItemsPropertiesUpdated` carries no revision, so it drops the cache outright. Menus load on
pointer-enter and name every submenu id first, because that is where a lazy application fills one in.

## Rules

Services expose concrete handles; there is no daemon-owned trait, broker, registry, or string routing
layer. The handler, boundary and config rules are in `.claude/rules/daemon.md` and are not repeated
here.
