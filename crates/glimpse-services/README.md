# glimpse-services

The service framework and every service implementation.

A service is one tokio task owning a set of topics and a set of commands. The runtime owns the
select loop; a service implements handlers that run serially on `&mut self` and never see a raw
connection.

## Contents

- `service.rs`, `context.rs`, `subscription.rs`, `publisher.rs` — the framework
- `broker.rs` — `BrokerHandle`, the trait the daemon implements, with `MockBroker` beside it;
  `Responder` and the erased `Dispatch` a command travels through
- `services/` — one module per service; `tray/` will be a directory because it is the largest

## The geolocation service

Two providers behind one topic. `[geolocation] provider = "manual"` publishes the configured pair
directly; `"geoclue"` follows GeoClue's `Location` property. Either way `geolocation.status` is the
only thing downstream services see, which is what lets `solar` subscribe to it without knowing a
provider exists.

Three details are load-bearing:

- **The GeoClue watch is subscribed before `Start`**, because the first fix can arrive before that
  call returns.
- **`GCLUE_ACCURACY_LEVEL_CITY`, not exact.** Sunrise, sunset and weather are everything downstream
  of this service, and none of them is sharper than a city.
- **Authorization is a shipped file, not code.** `data/geoclue/conf.d/glimpse.conf` is what stops
  GeoClue deferring to an agent that either is not running or has nobody to answer it. Its section
  name and `DESKTOP_ID` must agree.

A missing fix, a refused request or coordinates outside their ranges all leave the service
`degraded` and publishing `None` — running, and honest about having nothing. A `manual` table
*missing* a coordinate is not among them: `[geolocation]` is a tagged enum, so that document never
loads.

## The solar service

`solar.status` carries one field, `phase`, and nothing else — no sunrise timestamps, no color
temperature. It follows `geolocation.status`, recomputes on every location it is handed and once a
minute after that, and declares its timer only while it holds coordinates.

Two details are load-bearing:

- **Above the polar circles a date has neither event**, so `sunrise` answers `None` for both and the
  phase falls back to the sign of the solar declination against the sign of the latitude — that
  hemisphere's own season is what decides between midnight sun and polar night.
- **Without a location it publishes nothing** and reports `degraded`. `SolarStatus.phase` is not
  optional and `Day` is not a safe guess to make at three in the morning.

## The compositor service

Mirrors `glimpse-compositors` onto four topics — `compositor.status`, `compositor.workspaces`,
`compositor.windows`, `compositor.outputs` — and passes eight commands straight through. It reads a
snapshot once, follows the event stream, and re-reads the whole snapshot on a `Resync`.

**There is no `compositor.focus` topic.** A focus change already mutates the `focused` flag inside
the workspace and window lists, so those republish anyway and a separate topic would be a second
answer to a question that already has one.

**The whole snapshot is re-read on a resync, not the named part.** `Snapshot` fetches every part
concurrently in one call, and `Publisher` drops a topic whose value did not change, so re-reading
everything costs one round trip and publishes only what actually moved. Per-part refetching would be
three code paths to keep in step with the event enum.

**A resync is a declared source keyed by an attempt counter**, the way geolocation's retry is. A
resync arriving mid-fetch bumps the key, which tears the in-flight read down and starts a current
one — that is the coalescing, and it needs no `fetching`/`pending` bookkeeping.

**Urgency is derived here so every client sees one answer.** A window is urgent when the compositor
says so; a workspace is urgent when the compositor says so *or* when any window on it is. The `or`
is what makes Hyprland work at all, since it never marks a workspace urgent. Deriving it rather than
caching it per workspace is also what keeps the two consistent when a window moves, which Hyprland
reports as a bare `Resync(Structure)`.

**A focused window's urgency is cleared locally.** Hyprland's `urgent>>address` only ever arrives as
"became urgent" — it drops its own `urgencyHint` on focus and says nothing on the socket. Without
the local clear the dot stays warm until the window closes. niri clears it itself, and the next
snapshot is authoritative on both.

**Workspaces are ordered here**, by output and then by `index` falling back to `id`. Only niri fills
`idx`; a Hyprland workspace's id is its number. Ordering once in the producer is what stops every
client from arriving at a different answer.

**`OutputInfo.label` is composed.** niri leaves `description` null and fills `make`/`model`;
Hyprland fills `description` and pads nothing. A popover row headed `Move to display` has nothing to
render unless one of them is turned into a label here.

**Commands are awaited inline rather than spawned.** A compositor command is a round trip on a local
socket, and a compositor that cannot answer has taken the session with it. Awaiting keeps a command
and the events it causes in the order they happened, which is worth more than isolating a hang that
cannot occur without the session already being over.

## The calendar service

One topic, `calendar.events`, carrying every occurrence from every configured source as one sorted
list, and one command, `calendar.refresh`. Each `[[calendar.sources]]` entry declares its own
sources — a watch, a timer, or for a sidecar both — keyed on the source's id, uri and the shared
refresh counter, so editing one source disturbs only that one and `calendar.refresh` restarts all
of them by bumping the counter the way geolocation's retry does.

**Whether a source is watched or fetched follows the uri, not the kind.** `declares` is the whole
rule and is one function for exactly that reason:

| source | watched | timer |
| --- | --- | --- |
| `directory`, any local path | yes | no |
| `ical`, a path or `file://` holding a calendar | yes | no |
| `ical`, `http(s)://` | no | yes |
| `ical`, `file://` holding a one-line feed URL | yes | yes |
| `directory` given a feed, `ical` with an unknown scheme | reports the mistake | |

The watch is `glimpse-config`'s. `watch(dir)` returns `impl Stream<Item = Update> + Send + 'static`,
which is exactly the shape `Sub::stream` takes, so an edited `.ics` re-reads that one source within
the watcher's 250 ms debounce. A file is watched through its parent directory, because that is what
inotify gives you.

The last row is the sidecar, and it is the reason the rule cannot be read off the configuration
alone: a `file://` pointing at a one-line URL is a local file whose calendar is on the network, so
it needs both — the watch reports the pointer changing, the timer re-fetches what it points at.
Which it is only becomes known by reading the file, so `resolve` answers that question before any
fetch happens and `Fetching::remote` is derived from its answer rather than written out again
beside the request. The service remembers the ids that came back remote and declares their timers
on the next reconcile.

Three consequences follow from a watched source having no timer:

- **The stream opens with a read.** A watch only speaks when something changes, so without a
  leading `stream::once` a watched source would publish nothing until its first edit. That read is
  also what `calendar.refresh` triggers, because the refresh counter is in the watch's key and a
  restarted stream leads with it again.
- **`Update::Unavailable` is a failure, not a warning.** With no timer there is no second reader to
  fall back on, so an unarmable watch degrades the service and names the source. So does a
  `directory` whose uri turns out to be a feed — silence would be the only other answer, and it is
  the wrong one.
- **`poll-interval` only describes the network.** Both the shared value and a source's own are read
  by sources that are fetched; the schema says so. A setting that looks like it works and does not
  is worse than one documented as inapplicable.

**`read` is handed its clock rather than reading one.** The occurrence window is anchored on the
instant passed in, which is `Utc::now()` at both call sites and a fixed instant in tests. A `read`
that called `Utc::now()` itself made every fixture-dated test expire sixty-two days after it was
written.

**A per-entity topic cannot be declared, so one topic carries the collection.** `TOPICS` is
`&'static [&'static str]` and the broker drops a publish to a name nothing declared, so there is no
`calendar.source.{id}.events`. The cost is honest: one source changing republishes every event. The
shape is what `next-event` will read later — it wants the earliest entry across all sources, which
is the first element of a list already sorted by start.

**A failing source degrades the service and keeps its last events.** A feed that 404s or a directory
that disappears lands in a failure map; the events fetched before it broke stay published, and
`system.services` names which source failed and why. There is no retry loop on top of the poll
interval — the next tick is the retry.

**The reason a failure reports never contains the uri.** A provider's iCalendar URL is a bearer
token: whoever holds it reads the calendar without signing in. So a transport failure goes through
`reqwest::Error::without_url`, a filesystem failure reports `io::ErrorKind` rather than the path, and
the degraded reason names the source's `id`. A test asserts the uri is absent, because this is the
kind of leak that is invisible until someone pastes a health report into a bug.

**`file://` is read twice over.** The schema offers a sidecar file so the secret URL never enters
`config.toml`, and also calls `file://` a feed. Both are honored by looking at the content: a single
line under 2 KiB that parses as an `http(s)` URL is a sidecar and is fetched; anything else is
parsed as iCalendar. A calendar document is never one line, so the two cannot be confused. Any other
scheme — `webcal://` included — is an error rather than a path, because falling through to the
filesystem would report "cannot read the file" for something that was never a file.

**Recurrence is `icalendar`'s, not ours.** `CalendarEvent::get_recurrence` builds an
`rrule::RRuleSet` out of `DTSTART`, `RRULE`, `RDATE` and `EXDATE`, and a component with no `RRULE`
still yields its `DTSTART` as one occurrence — so a single entry and a weekly standup take the same
code path. Occurrences are queried over a window of seven days back to sixty-two days ahead, capped
at 512 per series and 512 per source. The window is re-anchored on every poll, which is what keeps
it from drifting as days pass.

**A `DURATION` with no `DTEND` reads as a zero-length entry.** `icalendar` exposes `DTEND` and not
`DURATION`, and the entries Google and Nextcloud emit always carry `DTEND`. This is a known gap, not
a decision.

**The poll interval has a floor of sixty seconds.** `Duration::from_secs(0)` makes
`tokio::time::interval` panic, so `poll-interval = 0` would take the service down on the first
declaration rather than at some later edge; every value under the floor is also a request the
provider would answer by rate-limiting us. The clamp is in the `From<&Config>` impl, with the
duplicate-id filter beside it — two sources sharing an id would share a subscription key, so the
second would never run while silently overwriting the first's events.

**Summaries and locations are capped before they are published.** A feed is another application's
text: unbounded, and attacker-controlled when the feed is shared. `clean` collapses whitespace,
turns a control character into a separator rather than dropping it — dropping one splices two words
together — and ellipsizes on a character boundary.

## Rules

The dependency arrow points from `glimpsed` to here and never back. Anything the framework needs
from the daemon is a trait declared in this crate.

Mirror services (network, bluetooth, audio, battery, mpris, brightness) enumerate once then follow
change signals. The backend is right when they disagree, and no decision the backend already makes
gets reimplemented here.

A handler that can block moves its `Responder` into `ctx.spawn`. Handlers run serially, so one slow
D-Bus call otherwise freezes the whole service. Such a task usually returns the event saying the
command finished, which the handler wants anyway; one with nothing to report uses
`ctx.spawn_detached` rather than inventing an event for the handler to ignore.

A `Responder` that is dropped unanswered — queued when the service stopped, lost to a panicking
handler, or simply forgotten — answers `Unavailable` from its `Drop` impl and logs, rather than
leaving the caller to wait out its whole timeout with nothing said anywhere.

Commands are declared the way topics are: `METHODS` lists the names, `decode` turns one plus its
JSON arguments into the service's own `Command` type, and the two must agree — a name in `METHODS`
that `decode` refuses is a command the broker will route and the service will then reject. Nothing
makes them agree at compile time, so every service carries one test calling
`assert_declarations::<Self>()`, which checks both lists against `ALL_TOPICS` / `ALL_COMMANDS` and
that every declared method reaches an arm of `decode`. The
default `decode` refuses everything, which is right for a service that declares no methods. A
command reaches the inbox through `ServiceSender::dispatch`, which offers rather than queues:
the caller is the broker, and the broker must never await.

Everything that reaches a handler arrives as an event from a **source**, and every source is one
`ctx` call returning a `SourceGuard`. Dropping the guard is the whole cancellation story — it aborts
the task or drops the subscription, so there is no token to remember and no shutdown path to write.

| Source | Produces | For |
| -------------------- | ---------------- | --------------------------------------------------- |
| `ctx.spawn`          | one event        | one unit of async work whose result is an event     |
| `ctx.spawn_detached` | nothing          | work with no result to report — see below           |
| `ctx.interval`       | an event a tick  | polling, clocks; `at_interval` picks the first tick |
| `ctx.stream`         | many events      | a backend signal stream, a watch, a subscription    |
| `ctx.subscribe::<T>` | many events      | another service's topic                             |

`SourceGuard` is `#[must_use]`, because `ctx.spawn(...)` written as a statement drops the guard at
the semicolon and aborts the task before it runs — a call that looks right and does nothing.

A panic inside a source is caught, logged and turned into `degraded` on the owning service. A source
is where the backend's own data gets parsed, which makes it both the likeliest place to panic and
the least visible: uncaught, the task stops and the service goes on believing it still has a source.

`spawn`, `interval` and `stream` each take an async closure receiving a `Ctx` of its own, so a task
reaches the buses, the publishers and `degraded` without any of them being threaded through its
arguments — `Ctx` is cheap to clone and its `degraded` flag is shared, so a task that degrades the
service is visible to the runtime. `stream`'s closure is async because building a source usually is:
a D-Bus signal stream has to be requested before it can be read.

`stream` is also the one that does the delivering: `spawn` is a stream of one item and `interval` a
stream of ticks, so a closed inbox is answered in a single place rather than once per constructor.
`subscribe` is the exception, because it has a broker subscription to release as well as a task to
abort. Its sink parks the newest payload in a `tokio::sync::watch` cell and a pump task delivers it
— the broker is called from its own task and must never be made to wait, and newest-wins is what a
bounded channel cannot give, since a full one drops whatever it is handed, which is always the
newest.

## Subscriptions

A source that should live as long as the service says so is **declared**, not started. `subscriptions`
returns what ought to be running, given the service as it stands, and the runtime diffs that against
what is running after `start` and after every input:

```rust
type SubKey = Watch;

fn subscriptions(&self) -> Vec<Sub<Self>> {
    match self.provider {
        Provider::Geoclue => vec![Sub::stream(Watch::Geoclue { attempt: self.attempt }, geoclue)],
        Provider::Manual(_) => Vec::new(),
    }
}
```

`Sub::stream`, `Sub::interval` and `Sub::topic::<T>` mirror the `ctx` constructors above; the runtime
calls one only for a key it is not already running. Switching geolocation to `manual` releases GeoClue
because the key stops being named, not because a handler remembered to drop a guard.

`SubKey` is the identity a boxed closure cannot supply, and the whole discipline follows from how it
is chosen: **whatever must force a restart belongs in the key, and whatever must not must stay out.**
Heartbeat keys its timer on `period_ms`, so `heartbeat.set_interval` restarts it by assigning a field.
Geolocation keys on an `attempt` counter that carries nothing but its own difference, because
`geolocation.refresh` has no parameter to change and an unmoved key would leave the watch running.
A key too coarse silently ignores a change; a key holding something that moves per event silently
rebuilds the source every time. Both fail quietly, which is why the key is worth choosing deliberately.
Two declarations sharing a key is a bug — the second is dropped, warned about once.

Both kinds of source come back current after a restart. `Sub::stream` re-reads its backend, and
`Sub::topic` is handed the topic's stored value the moment it subscribes — the broker replays it on
`Message::Subscribe`. Without that replay a publisher's equality gate, which never republishes an
unchanged value, would leave a resubscribed topic blank until the upstream value happened to
change, and for a one-shot producer that is never.

This is `Sub` against `Cmd`, and the split is the same one Elm draws: `subscriptions` is for sources
whose lifetime the model decides, and `ctx.spawn` / `ctx.spawn_detached` for an effect that fires once
and is never re-declared — a slow command that moved its `Responder` into a task. A service with no
declared sources writes `type SubKey = ();` and inherits the empty default, since associated type
defaults are still unstable.

`subscriptions` runs inside the same `catch_unwind` as the handler, so a panic while declaring stops
that one service rather than the runtime loop.

A service declares `type Config` and receives it as `Input::Config`. The projection from the whole
document down to that slice is `From<&glimpse_config::Config>`, implemented beside the slice rather
than on the service, so whatever it validates stays private to the module:

```rust
impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self { ... }
}
```

A service that reads no configuration writes `type Config = NoConfig;` and no impl. `()` will not do
— `From<&Config> for ()` is a foreign trait on a foreign type and the orphan rules refuse it, which
is the whole reason `NoConfig` exists. `S::Config: PartialEq` is what narrows a reload to the
services whose own table moved.

Events, commands and configuration all arrive on **one** inbox. One channel means one order: a
command and the event that follows it reach the handler in the order they were produced, which two
channels raced against each other in a `select!` could not promise. The cost is a shared budget —
a service flooding its own inbox with events makes `dispatch` refuse commands with `Unavailable`,
which is the honest answer but a coarse one.

A service publishes through a `Publisher` it takes from `ctx.publisher::<T>()` in `start` and keeps
for its lifetime. The publisher holds the last value it sent and drops a `set` that matches it, so
an unchanged payload is never serialized and never reaches the broker — this is the equality gate
the whole topic design rests on, and a publisher rebuilt per call would defeat it by starting from
no last value every time. `seq`, `ts` and `stale` are the broker's to assign; a publisher hands over
a topic name and a value and knows nothing about any of the three.

A service reaches D-Bus through `ctx.session_bus()` and `ctx.system_bus()`, never by opening a
connection of its own. Both return `Result<&zbus::Connection, &str>`: the daemon connects once
before any service starts, and the `Err` is why there is no connection. A service that needs a bus
and gets `Err` calls `ctx.degraded(...)` with that reason and carries on — a missing bus costs it
its backend, not its life, and `system.services` is where anyone finds out which.

`just test-crate glimpse-services` runs every service against the mocks, with no display, no
session bus and no broker. `Buses::unavailable("...")` is the no-bus case a test injects, the way
`MockBroker` is the no-broker one.
