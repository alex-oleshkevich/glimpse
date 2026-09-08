# glimpse-services

The service framework and every service implementation.

A service is one tokio task owning a set of topics and a set of commands. The runtime owns the
select loop; a service implements handlers that run serially on `&mut self` and never see a raw
connection.

## Contents

- `service.rs`, `context.rs`, `subscription.rs`, `publisher.rs` — the framework
- `broker.rs` — `BrokerHandle`, the trait the daemon implements, with `MockBroker` beside it;
  `Responder` and the erased `Dispatch` a command travels through
- `services/` — one module per service; `weather/` is a directory because it carries two providers

## The geolocation service

Two providers behind one topic. `provider = "manual"` publishes the configured pair; `"geoclue"`
follows GeoClue's `Location` property. Either way `geolocation.status` is all downstream sees, which
lets `solar` subscribe without knowing a provider exists.

- **The GeoClue watch is subscribed before `Start`**, because the first fix can arrive before that
  call returns.
- **`GCLUE_ACCURACY_LEVEL_CITY`, not exact.** Nothing downstream is sharper than a city.
- **Authorization is a shipped file, not code.** `data/geoclue/conf.d/glimpse.conf` stops GeoClue
  deferring to an agent that either is not running or has nobody to answer it. Its section name and
  `DESKTOP_ID` must agree.

A missing fix, a refused request or out-of-range coordinates leave the service `degraded` and
publishing `None`. A `manual` table *missing* a coordinate is not among them — `[geolocation]` is a
tagged enum, so that document never loads.

## The solar service

`solar.status` carries one field, `phase` — no sunrise timestamps, no colour temperature. It follows
`geolocation.status`, recomputes on every location and once a minute after, and declares its timer
only while it holds coordinates.

- **Above the polar circles a date has neither event**, so the phase falls back to the sign of the
  solar declination against the sign of the latitude.
- **Without a location it publishes nothing** and reports `degraded`. `Day` is not a safe guess to
  make at three in the morning.

## The compositor service

Mirrors `glimpse-compositors` onto `compositor.status`, `.workspaces`, `.windows` and `.outputs`,
and passes eight commands through. It reads a snapshot once, follows the event stream, and re-reads
the whole snapshot on a `Resync`.

**There is no `compositor.focus` topic.** A focus change already mutates the `focused` flag inside
the workspace and window lists.

**The whole snapshot is re-read on a resync, not the named part.** `Snapshot` fetches every part
concurrently and `Publisher` drops a topic whose value did not change, so re-reading everything
costs one round trip and publishes only what moved. Per-part refetching would be three code paths to
keep in step with the event enum.

**A resync is a declared source keyed by an attempt counter.** A resync arriving mid-fetch bumps the
key, which tears the in-flight read down and starts a current one — that is the coalescing, and it
needs no `fetching`/`pending` bookkeeping.

**Urgency is derived here so every client sees one answer.** A workspace is urgent when the
compositor says so *or* when any window on it is; the `or` is what makes Hyprland work at all, since
it never marks a workspace urgent. **A focused window's urgency is cleared locally**, because
Hyprland's `urgent>>address` only ever arrives as "became urgent" — it drops its own `urgencyHint`
on focus and says nothing on the socket. niri clears it itself.

**Workspaces are ordered here**, by output then by `index` falling back to `id`. Only niri fills
`idx`; a Hyprland workspace's id is its number.

**`OutputInfo.label` is composed.** niri leaves `description` null and fills `make`/`model`;
Hyprland fills `description`.

**Commands are awaited inline rather than spawned.** A compositor command is a round trip on a local
socket, and a compositor that cannot answer has taken the session with it. Awaiting keeps a command
and the events it causes in order.

## The calendar service

One topic, `calendar.events`, carrying every occurrence from every configured source as one sorted
list; commands `calendar.refresh` and `calendar.set_range`. Each source declares its own sources —
a watch, a timer, or for a sidecar both — keyed on id, uri and a shared refresh counter, so editing
one source disturbs only that one.

**Whether a source is watched or fetched follows the uri, not the kind.** `declares` is the whole
rule:

| source | watched | timer |
| --- | --- | --- |
| `directory`, any local path | yes | no |
| `ical`, a path or `file://` holding a calendar | yes | no |
| `ical`, `http(s)://` | no | yes |
| `ical`, `file://` holding a one-line feed URL | yes | yes |
| `directory` given a feed, `ical` with an unknown scheme | reports the mistake | |

The watch is `glimpse-config`'s: `watch(dir)` returns exactly the shape `Sub::stream` takes, so an
edited `.ics` re-reads that one source within the watcher's 250 ms debounce. A file is watched
through its parent directory, because that is what inotify gives you.

The last row is the sidecar, and it is why the rule cannot be read off the configuration alone: a
`file://` pointing at a one-line URL is a local file whose calendar is on the network. Which it is
becomes known only by reading the file, so `resolve` answers that before any fetch and
`Fetching::remote` is derived from its answer.

Three consequences follow from a watched source having no timer:

- **The stream opens with a read.** A watch only speaks when something changes, so without a leading
  `stream::once` a watched source would publish nothing until its first edit. That read is also what
  `calendar.refresh` triggers.
- **`Update::Unavailable` is a failure, not a warning.** With no timer there is no second reader to
  fall back on. So is a `directory` whose uri turns out to be a feed.
- **`poll-interval` only describes the network.** A setting that looks like it works and does not is
  worse than one documented as inapplicable.

**Fetching and expanding are two steps, and only the first touches the world.** `read` fetches and
parses, keeping the `icalendar::Calendar` behind an `Arc`; `expanding` turns those into events for
one `Window`. So `calendar.set_range` changes what is published without a single request — measured,
a panel stepping to a month a year out gets its answer off calendars already in memory. Parsing at
fetch is what keeps an unparseable document a reported failure rather than a source that silently
expands to nothing on every window change.

**Re-expansion is a subscription, not work done in the handler.** Anything invalidating the list
bumps `generation`, which is a `SubKey`, so the framework retires the old expansion and starts a new
one. The expansion runs on `spawn_blocking`, because a crowded calendar over a wide window is real
CPU work. An `Expanded` event carrying a stale generation is dropped by the same straggler guard the
fetches use.

**Every instant arriving from a client is added to with `checked_add_signed`.** `DateTime +
TimeDelta` panics on overflow, a panicking handler stops its service until the daemon restarts, and
`DateTime<Utc>`'s serde accepts an extended year — `+262142-06-01T00:00:00Z` deserializes, and
`Window::asked` then added `SPAN` days to it. One `calendar.set_range` took the calendar down. The
same applies to `start + length`.

**The window is asked for, not assumed.** `Window::around` is the near window a fresh service
publishes so a client that never asks still sees something — `BACK` days behind to `AHEAD` ahead,
re-anchored on every poll so it cannot drift. `Window::asked` is what `calendar.set_range` sets,
clipped to `SPAN` days and to a non-negative length. A fixed window is what made a December nobody
had fetched look like a December with nothing in it.

**A per-entity topic cannot be declared, so one topic carries the collection.** `TOPICS` is
`&'static [&'static str]` and the broker drops a publish to a name nothing declared, so there is no
`calendar.source.{id}.events`. The cost is honest: one source changing republishes every event. It
is also the shape both calendar applets want — the earliest entry across all sources is the first
element of a list already sorted by start.

**Truncation is reported, not silent.** The merged list is capped at `EVENTS`, applied after
sorting, so a crowded calendar loses the tail of the window rather than a random slice — measured
collapsing a 69-day window to 10 days with nothing on the wire to say so, which a surface cannot
tell apart from a quiet month. `expanding` sends the start of the first entry it dropped as
`truncated_from`. **The cap belongs to the merged payload, not to a source**: capping each source
first would publish more than the cap, capping only the first would drop a whole calendar.

**`webcal://` is rewritten before the url is parsed, not given a branch of its own.** It is what a
provider's Subscribe button hands out and is plain `https://` underneath. `Url::set_scheme` cannot
do it — the `url` crate refuses a non-special to special change — so it is a string swap, matched
case-insensitively through `str::get` so a multi-byte first character cannot panic the slice.
`sidecar` runs it too, because the sidecar file exists to hold the link a provider gave you.

**An entry's length is capped, because a surface walks the days it covers one at a time.** The clock
popover draws a dot per day, so a `DURATION` of `P9999Y` — or a `DTEND` in the year 9999 — is three
and a half million iterations in the GTK main loop per update. `length` clips to `SPAN` days, which
no window can exceed anyway. `iso8601` accepts years and months, which RFC 5545 forbids in a
duration; the cap is what makes that harmless rather than a reason to hand-roll the grammar.

**An entry may carry `DURATION` instead of `DTEND`, and `icalendar` does not surface it.**
`get_end()` returns `None` for one, which read as a zero-length event — Apple and several CalDAV
exporters write them, and every such entry rendered as an instant. `length` falls back to
`property_value("DURATION")` parsed by `iso8601`, which `icalendar` already depends on. `DTEND`
still wins where an exporter writes both, which RFC 5545 forbids anyway.

**A dead watch never overwrites a failed read.** Both failures are true and both name the source,
but only the read says the path is wrong — and `Update::Unavailable` arrived second, so a typo in
`uri` reported "its directory is not being watched" and nothing about the path. The handler inserts
`Event::Unwatched` with `entry().or_insert()`, so it fills in only when no read has failed.

**Text off a feed is cleaned against bidi, not only against control characters.**
`char::is_control` is the Cc category alone, so the overrides `U+202A..=U+202E` and the isolates
`U+2066..=U+2069` pass it — and Pango honours both, which lets a summary reorder the row it lands
in. `clean` names those ranges beside `is_control` and lives in `glimpse-utils`, because weather's
alerts need the same gate; `glimpse-compositors` carries the same predicate for window titles. It also collapses whitespace, turns a control character into a separator rather
than dropping it (dropping one splices two words together), and ellipsizes on a character boundary.

**A failing source degrades the service and keeps its last events.** The events fetched before it
broke stay published and `system.services` names which source failed and why. There is no retry loop
on top of the poll interval — the next tick is the retry.

**A failure reason never contains the uri.** A provider's iCalendar URL is a bearer token: whoever
holds it reads the calendar without signing in. A transport failure goes through
`reqwest::Error::without_url`, a filesystem failure reports `io::ErrorKind` rather than the path, and
the reason names the source's `id`. A test asserts the uri is absent, because this leak is invisible
until someone pastes a health report into a bug.

**`file://` is read twice over.** The schema offers a sidecar so the secret URL never enters
`config.toml`, and also calls `file://` a feed. Both are honoured by looking at the content: a single
line under `SIDECAR` bytes that parses as an `http(s)` URL is a sidecar and is fetched; anything else
is parsed as iCalendar. A calendar document is never one line. Any other scheme is an error rather
than a path, because falling through to the filesystem would report "cannot read the file" for
something that was never a file.

**Recurrence is `icalendar`'s, not ours.** `get_recurrence` builds an `rrule::RRuleSet` out of
`DTSTART`, `RRULE`, `RDATE` and `EXDATE`, and a component with no `RRULE` still yields its `DTSTART`
as one occurrence — so a single entry and a weekly standup take the same code path. Occurrences are
capped at `OCCURRENCES` per series.

**The poll interval has a floor of sixty seconds.** `Duration::from_secs(0)` makes
`tokio::time::interval` panic, so `poll-interval = 0` would take the service down on the first
declaration; every value under the floor is also a request the provider would answer by
rate-limiting us. The clamp is in the `From<&Config>` impl, with the duplicate-id filter beside it —
two sources sharing an id would share a subscription key, so the second would never run while
silently overwriting the first's events.

## The weather service

One topic, `weather.status`, holding one entry per place being watched, and two commands:
`weather.watch` and `weather.refresh`.

**One file per provider.** `weather/mod.rs` holds the service — leases, the poll, `absorb`, and the
two gates every reading passes on its way to a payload, `sunlit` and `sanitized`. `open_meteo.rs`
and `met_no.rs` hold one provider each: its endpoints, its wire structs, its decode into `Reading`,
and its own tests. `Provider::fetch` is the only place that names both.

What stays in `mod.rs` is what more than one provider needs — `Ask`, `Reading`, `fetch_json`,
`hour_floor`, `percent` and `bearing` — and the two providers do not see each other at all. A helper
that migrates out of `mod.rs` into a provider file is the signal that the other provider stopped
needing it; one that migrates the other way is a rule that turned out to be about weather rather
than about a source.

The shared ones are rules the *payload* imposes rather than answers either source gave: the window
hours are cut to, the percentage a humidity or a chance is squeezed into, the compass degree a
bearing wraps onto, and the one round trip that turns a built URL into a decoded body. Every
provider's own numbers — met.no's Celsius and metres per second, Open-Meteo's WMO codes — convert in
its own file, because those are facts about the source.

**Every list is cut in `absorb`, not in the provider that filled it.** `forecast_days` is asked for
in the query and nothing obliges a source to honour the answer, so the day list is cut where
`sunlit` fills the sun times and `sanitized` caps the alerts — one gate, on the readings every
provider produces, and a third provider inherits all three by reaching the payload the same way.
Hours are the exception, capped inside each provider, because `HOURS` is a module constant a new
provider imports and the compiler shows it; the ask is runtime configuration that nothing would
show it.

**Places are not configured; they are leased.** `[weather]` holds `provider`, `units`,
`poll-interval` and `forecast-days` and nothing else. A consumer calls `weather.watch` naming either
`here` or a coordinate pair, and that registration is honoured for thirty minutes unless it is asked
for again — the panel renews on the tick it already has. Nothing tells this service that a client
went away: `BrokerHandle` carries no subscriber count and `Responder` no client identity, so a
registration that never expired would pin a place and keep fetching for it until the daemon
restarted. There is deliberately no `weather.forget`; not renewing is how you stop.

**With nothing leased, nothing happens.** `subscriptions` declares the geolocation subscription only
while something watches `here`, and the poll only when there is at least one coordinate to ask
about. So a fresh install issues no outbound request at all, and the user's coordinates never leave
the machine until a consumer asks for weather. That property is structural rather than a default
someone can flip, which is why `[weather]` has no `follow-location` key: turning weather on is an act
by a consumer, not a line in a document.

It does **not** extend to GeoClue. The `geolocation` service opens its own GeoClue stream whenever
`geolocation = "geoclue"`, whoever is or is not subscribed, and `solar` subscribes to the topic
unconditionally. Declaring `Watch::Location` conditionally saves an idle topic subscription inside
the daemon, not a device wake — an earlier draft of this section claimed otherwise and was wrong.

**Leases are swept on a watch as well as on a fetch.** A renewal is the one event that still arrives
when nothing is being fetched, and both no-fetch states are reachable: an `http` client that would
not build, and a lone `here` lease with no fix yet. Sweeping only on `Fetched` left those leases
immortal, which pinned the geolocation subscription and — because the `MOST_WATCHED` cap counts
entries rather than live ones — let eight dead registrations refuse every later `weather.watch`.

**A renewal must not restart the poll.** `Sub::interval` builds on `ctx.interval`, which starts at
`Instant::now()`, so a rebuilt subscription fetches immediately. `generation` — the only thing in
`Watch::Poll` — is bumped when the *resolved coordinate set* changes, never when a command merely
arrives. Bumping it per `weather.watch` would fetch at the renewal cadence instead of the configured
one, which against a shared free tier is a silent fifteenfold overspend visible only in a running
shell.

**A fix has to move a kilometre to count**, measured against the fix last *accepted* rather than the
one last seen, so drift below the threshold never accumulates into a refetch and a fix that jitters
by metres does not refetch for ever. Below a kilometre the provider answers out of the same grid
cell anyway.

**One request covers every place.** Open-Meteo takes comma-separated coordinates and answers with an
object for one location and an array for several — the single-watch case is the common one, so the
response is decoded through an untagged enum covering both. `timeformat=unixtime` is load-bearing:
with `timezone=auto` the provider otherwise returns naive local ISO strings with no offset, which is
what made the previous generation compare timestamps as text and open its hourly strip an hour late.
Each place carries `utc_offset_seconds`, without which a renderer cannot label a time for a place in
another timezone; a bare `timezone=auto` repeated once per location resolves each of them
separately, verified against a live pair in opposite hemispheres.

**`observed_at` is the provider's own validity time, not our fetch clock.** It moves at most every
fifteen minutes, so it cannot defeat `Publisher`'s equality gate, and when the network dies it
freezes — which is the truthful thing for a popover reading "updated N minutes ago" to say.

**A failed fetch keeps the last reading and degrades.** The previous generation discarded its
snapshot and blanked the bar; a degraded service is a running one, and its numbers are still the
newest anyone has. There is no retry loop: the next tick is the retry.

**A failure reason never quotes the request.** The query string carries the user's latitude and
longitude, so a reason naming the URL is a location leak wherever it is pasted — a stronger version
of the calendar's bearer-token rule. Transport failures go through `reqwest::Error::without_url`.
Every string this service formats itself, from a number the provider sent — with one exception,
below.

**The poll interval has a floor of ten minutes and defaults to fifteen.** Open-Meteo recomputes
current conditions every fifteen minutes, so a shorter interval asks again for data that provably
has not moved. The floor is applied in the `From<&Config>` impl and again at the declaration site,
because `Duration::from_secs(0)` panics `tokio::time::interval`.

**An alert is the one piece of third-party prose weather carries, and it is sanitised like a
calendar summary.** `headline`, `description` and `source` come off a national alert feed
unbounded and unescaped, so they go through `clean` — cap **and** bidi strip — not `cap`, which
only truncates. The list is bounded by `MOST_ALERTS` as well, because a count is as unbounded as a
length. Both happen in `sanitized`, called from `absorb`, which is the single path every provider's
readings take into a payload — so a source that starts answering with alerts is cleaned without
having to remember to be.

**Alerts are on the wire before any provider fills them.** Open-Meteo has no alerts endpoint
(open-meteo/open-meteo #183, still open), which is a fact about one provider; the payload is
provider-neutral by the same argument that made `Condition` a closed set. So `PlaceWeather.alerts`
exists now, Open-Meteo answers with an empty list, and the next source fills it with no contract
change and no second topic. It is `Vec` under `#[serde(default)]`, never `Option<Vec>`: two
spellings of "nothing to report" is one more than a renderer should branch on, and the default is
what keeps an older daemon's payload decoding in a newer panel.

**Sun times are computed, never taken from a provider.** Open-Meteo will send `sunrise` and
`sunset` and met.no cannot, so reading them off the payload made one fact arrive two ways and
disagree at the edges. `sunlit` fills them in `absorb`, which is the single path every reading takes
into a payload — the same argument that puts `sanitized` there — so a source added later gets them
without remembering to ask, and the Open-Meteo query is two fields shorter. The date used is the day
in the *place's* own zone, which is what `DayForecast.start` already is.

`crate::sun::events` is the one implementation, shared with the solar service. It returns
`Option<(Option, Option)>` on purpose: the outer `None` is coordinates that are not on Earth, the
inner ones are a day on which the sun did not cross the horizon. Collapsing them would leave
`solar` unable to tell a bad fix from a polar day, which is the distinction its `polar_phase`
fallback turns on.

**met.no is a second provider, and it supplies three things Open-Meteo hands over for free.** It
answers one place per request rather than parallel lists, always in Celsius, metres per second and
millimetres whatever is asked of it, with no daily block and — the one that matters — no
UTC offset. So `met_no` converts the units itself, aggregates days out of the timeseries, and looks
the place's zone up from its coordinates with `tzf-rs`. Without that last one every hour label would read in the
panel's zone rather than the place's, which is wrong for any place but the one you are standing in.

**A day is named by the weather in the middle of it.** Aggregating a timeseries has to pick one
symbol for the day; taking the first would let the small hours name a day nobody is awake for, and
the last would name it after the night that follows.

**A failed alerts request is not a failed forecast.** met.no publishes CAP warnings on a second
endpoint, so a place is two requests. The numbers are still true when the second one fails, so it
degrades to no warnings rather than to no weather — and it is the only source that fills `alerts` at
all, which is why the field existed with no producer until it landed.

**`Condition` grew a variant rather than lying.** met.no reports sleet and WMO 4677 has no code for
it; mapping it onto freezing rain would print "Freezing rain" for wet snow. The enum is tagged with
`#[serde(other)]` so an older panel reads it as `Unknown`, and every renderer's match has no `_` arm,
so the compiler names each site that has to decide.

**Conditions are provider-neutral.** The wire carries a closed `Condition` enum rather than a raw
WMO code, so a second provider with its own vocabulary maps into the same set instead of being made
to lie in WMO. `Provider` has one variant and no `_` arm anywhere, which makes adding one a compile
error at every site that has to change. Turning a condition into words or an icon name is the
renderer's job and happens in the panel.

## Rules

The dependency arrow points from `glimpsed` to here and never back. Anything the framework needs
from the daemon is a trait declared in this crate.

Mirror services (network, bluetooth, audio, battery, mpris, brightness) enumerate once then follow
change signals. The backend is right when they disagree, and no decision the backend already makes
gets reimplemented here.

A handler that can block moves its `Responder` into `ctx.spawn`; handlers run serially, so one slow
D-Bus call otherwise freezes the service. Such a task usually returns the event saying the command
finished; one with nothing to report uses `ctx.spawn_detached`.

A `Responder` dropped unanswered answers `Unavailable` from its `Drop` impl and logs, rather than
leaving the caller to wait out its timeout with nothing said anywhere.

Commands are declared the way topics are: `METHODS` lists the names and `decode` turns one plus its
JSON arguments into the service's `Command`. Nothing makes them agree at compile time, so every
service carries one test calling `assert_declarations::<Self>()`, which checks both lists against
`ALL_TOPICS` / `ALL_COMMANDS` and that every declared method reaches an arm of `decode`. A command
reaches the inbox through `ServiceSender::dispatch`, which offers rather than queues — the caller is
the broker, and the broker must never await.

Everything reaching a handler arrives from a **source**, and every source is one `ctx` call
returning a `SourceGuard`. Dropping the guard is the whole cancellation story.

| Source | Produces | For |
| -------------------- | ---------------- | --------------------------------------------------- |
| `ctx.spawn`          | one event        | one unit of async work whose result is an event     |
| `ctx.spawn_detached` | nothing          | work with no result to report                       |
| `ctx.interval`       | an event a tick  | polling, clocks; `at_interval` picks the first tick |
| `ctx.stream`         | many events      | a backend signal stream, a watch, a subscription    |
| `ctx.subscribe::<T>` | many events      | another service's topic                             |

`SourceGuard` is `#[must_use]`: `ctx.spawn(...)` written as a statement drops the guard at the
semicolon and aborts the task before it runs.

A panic inside a source is caught, logged and turned into `degraded`. A source is where the
backend's own data gets parsed, which makes it both the likeliest place to panic and the least
visible — uncaught, the task stops and the service goes on believing it still has a source.

`spawn`, `interval` and `stream` each take an async closure receiving a `Ctx` of its own, so a task
reaches the buses, the publishers and `degraded` without threading them through arguments.
`stream`'s closure is async because building a source usually is.

`stream` does the delivering: `spawn` is a stream of one item and `interval` a stream of ticks, so a
closed inbox is answered in one place. `subscribe` is the exception, having a broker subscription to
release as well as a task to abort. Its sink parks the newest payload in a `tokio::sync::watch` cell
and a pump task delivers it — the broker must never be made to wait, and newest-wins is what a
bounded channel cannot give, since a full one drops whatever it is handed, which is always the
newest.

## Subscriptions

A source that should live as long as the service says so is **declared**, not started.
`subscriptions` returns what ought to be running, and the runtime diffs that against what is running
after `start` and after every input:

```rust
type SubKey = Watch;

fn subscriptions(&self) -> Vec<Sub<Self>> {
    match self.provider {
        Provider::Geoclue => vec![Sub::stream(Watch::Geoclue { attempt: self.attempt }, geoclue)],
        Provider::Manual(_) => Vec::new(),
    }
}
```

Switching geolocation to `manual` releases GeoClue because the key stops being named, not because a
handler remembered to drop a guard.

`SubKey` is the identity a boxed closure cannot supply: **whatever must force a restart belongs in
the key, and whatever must not must stay out.** Heartbeat keys its timer on `period_ms`, so
`heartbeat.set_interval` restarts it by assigning a field; geolocation keys on an `attempt` counter
carrying nothing but its own difference, because `geolocation.refresh` has no parameter to change. A
key too coarse silently ignores a change; a key holding something that moves per event silently
rebuilds the source every time. Two declarations sharing a key is a bug — the second is dropped and
warned about once.

Both kinds of source come back current after a restart. `Sub::stream` re-reads its backend, and
`Sub::topic` is handed the topic's stored value the moment it subscribes. Without that replay the
publisher's equality gate would leave a resubscribed topic blank until the upstream value happened
to change — and for a one-shot producer, never.

This is `Sub` against `Cmd`, the split Elm draws: `subscriptions` for sources whose lifetime the
model decides, `ctx.spawn` for an effect that fires once. A service with no declared sources writes
`type SubKey = ();`.

`subscriptions` runs inside the same `catch_unwind` as the handler.

A service declares `type Config` and receives it as `Input::Config`. The projection from the whole
document is `From<&glimpse_config::Config>`, implemented beside the slice rather than on the service,
so whatever it validates stays private to the module. A service reading no configuration writes
`type Config = NoConfig;` — `()` will not do, because `From<&Config> for ()` is a foreign trait on a
foreign type. `S::Config: PartialEq` narrows a reload to the services whose own table moved.

Events, commands and configuration all arrive on **one** inbox, so a command and the event that
follows it reach the handler in the order they were produced — which two channels raced in a
`select!` could not promise. The cost is a shared budget: a service flooding its own inbox makes
`dispatch` refuse commands with `Unavailable`.

A service publishes through a `Publisher` taken from `ctx.publisher::<T>()` in `start` and kept for
its lifetime. It holds the last value sent and drops a `set` that matches, so an unchanged payload is
never serialized and never reaches the broker — the equality gate the whole topic design rests on,
which a publisher rebuilt per call would defeat. `seq`, `ts` and `stale` are the broker's to assign.

A service reaches D-Bus through `ctx.session_bus()` / `ctx.system_bus()`, never by opening its own.
Both return `Result<&zbus::Connection, &str>`; a service that needs a bus and gets `Err` calls
`ctx.degraded(...)` with that reason and carries on.

`just test-crate glimpse-services` runs every service against the mocks, with no display, no session
bus and no broker. `Buses::unavailable("...")` is the no-bus case, `MockBroker` the no-broker one.
