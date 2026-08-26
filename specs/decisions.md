# Decision log

Why the project went the way it did, across documents. Append-only, newest last.

A spec's Changelog records *what changed in that document*. This records *why the project changed
direction*, which is the part that gets lost when a decision spans several specs or reverses an
earlier one. When the two overlap, the Changelog line stays short and points here.

Add an entry when a decision changes direction, reverses an earlier decision, or is one somebody
would plausibly re-propose in six months. Do not add one for a routine choice that a spec's
*Alternatives considered* section already covers, or for anything readable from the code.

Never edit an entry. A reversal is a new entry that names the old one.

---

### 2026-08-20 · One `config.toml`, not one file per binary

Four per-binary files were proposed and rejected before shipping. Discoverability wins: a user
looking for a setting should not have to know which process reads it. The reload-granularity
objection is answered by per-service diffing — each service deserializes its own table, compares it
with `PartialEq`, and `apply` runs only on a difference, so editing `[panel]` cannot perturb the
night-light schedule. Specs 002, 010.

### 2026-08-20 · Drop-ins, not includes

`include = [...]` and a `config.d/` layer solve the same problem. Two mechanisms for one need is
worse than one, and drop-ins need no syntax, no cycle detection and no nesting limit. Spec 010.

### 2026-08-20 · `[appearance]` with `pack` and `scheme`, breaking

`theme` / `theme_mode` in `_old` conflated the icon-and-CSS bundle with light-versus-dark, and
`theme_mode` did not belong under `[theme]` at all. Renamed with no aliases: two spellings for one
key is a worse cost than one migration. Spec 010.

### 2026-08-20 · The `theme` service resolves against solar, not night light

`001_architecture.md` claimed `theme.mode` followed `night_light.state`. It never did — both
services hold their own solar handle and match on `solar::State` independently. Corrected by adding
an explicit `solar` service that both depend on. Spec 001.

### 2026-08-20 · No live wallpaper effects

Dropped entirely rather than deferred. Spec 005.

### 2026-08-20 · Wallpaper decode stays on the CPU

GPU scaling is already what happens at composite time and is free. Moving decode, resize and blur to
the GPU means uploading full-resolution textures and reading pixels back to write the cache, and the
readback costs more than the CPU resize for a one-shot operation. The cache exists so the cost is
paid once per distinct result, which is the case GPU acceleration would optimise away to nothing.
Spec 005.

### 2026-08-20 · The locker ignores logind's `Unlock`

`loginctl unlock-session` is polkit-gated only for *other* users' sessions; the per-session
`Session.Unlock()` method short-circuits polkit when the caller's uid owns the session, so any
process running as the user opens the screen with one unauthenticated D-Bus call. Verified against
`org.freedesktop.login1.policy` and by calling the method. Honouring it would make the PAM path
optional for anything already inside the session, and there is no external authentication agent here
to justify the cost. `Lock` is honoured — the asymmetry is that honouring `Lock` costs a screen the
user can open with their password. Spec 006.

### 2026-08-20 · No `--grace` on the locker

A flag existed with no behaviour specified behind it. A window in which the session is locked in
appearance only, with a guessed length, is how a security hole acquires a default value. Spec 006.

### 2026-08-20 · No `systemd-lock-handler`; the `power` service holds the sleep inhibitor

There is no user-level `sleep.target` on systemd 261, so unit ordering cannot put a locker up before
suspend without a helper daemon. Locking before sleep needs a *resident* process holding a logind
`delay` inhibitor, and the locker is on-demand — it is not running when the lid closes. The `power`
service already follows logind and takes the job. This costs invariant 5 for the suspend path
specifically: with `glimpsed` down an explicit lock still works and locking before suspend does not.
`InhibitDelayMaxSec` defaults to 5 s, which becomes the locker's deadline to reach `locked`.
Specs 006, 009.

### 2026-08-20 · `glimpse-proto` + `glimpse-client` → `glimpse-ipc`

The client and the server are two ends of one wire format. Split, the codec had two homes and the
only place a real client met a real server was an integration test over a temporary socket; merged,
a round trip is a unit test. The broker stays in `glimpsed`, so the boundary is transport against
routing rather than client against server.

Reverses the rule that `glimpse-proto` takes serde and nothing else. Its stated reason — that tokio
"closes the door" on `schemars` — was false: `schema_for!` compiles the crate and does not care what
else it links. The real costs were a UI binary compiling a socket server it never runs, which the
linker drops, and a crate name that would have stopped describing its contents, which the rename
fixes. Specs 001, 002.

### 2026-08-20 · The IPC transport is written, not taken from a crate — except the framing

Surveyed before writing. `jsonrpsee` has exactly the right subscription model and no Unix socket
transport at all — HTTP, WebSocket and WASM only — and moving the session bus to a TCP port to reach
a library is a worse trade than writing the transport. `tarpc` is strictly request-response, so
server-initiated `event` frames have no expression in it; its generic `serde_transport` would carry
a `UnixStream`, but the protocol shape is the mismatch, not the transport. `remoc` is a superset
with its own object model. `stubborn-io` reconnects underneath a `Framed` stream, which resumes
mid-frame, and cannot know to resubscribe.

What *is* taken: `tokio_util::codec::LinesCodec::new_with_max_length`, which is the line framing and
the length cap, already a workspace dependency and already exactly the specified behaviour. Only the
two policies on top are ours — an over-length or malformed line closes the connection rather than
recovering, because recovering leaves the two ends disagreeing about what was delivered. Spec 012.

### 2026-08-21 · Configuration inspection is `glimpsectl`'s alone

`glimpsed --check-config` and `--print-config` answered the same two questions as `glimpsectl
config validate` and `config show`, from a binary whose job is to keep running. One question with
two implementations drifts, and the daemon's was the weaker half: `config validate` and `config
path` read the stack from disk with nothing running, which is the situation a broken file produces.

The UI binaries keep their `--check-config`. It validates a stylesheet as well as a table, and
neither is the daemon's to check.

Removing the flags also empties the daemon's exit code 1: invalid configuration was already
specified not to exit, so `--check-config` was its only cause. Specs 003, 010.

### 2026-08-21 · `anyhow` in the binaries, `thiserror` in the libraries

The exit codes in `007` are decided almost entirely one crate down: four of the six come from a
transport failure, an unknown name, a timeout or a version mismatch, all of which `glimpse-ipc`
knows and `glimpsectl` only translates. A glimpsectl-local error enum would have been a
hand-written mirror of a type that has to exist anyway, kept in step by hand.

So the split is by who reacts. A library whose caller must branch — reconnect or not, `degraded`
for which reason, which exit code — declares a `thiserror` enum and the caller matches on it. A
binary, where every failure ends at one `eprintln!` and one exit code, takes `anyhow` and keeps the
context chain. `main` recovers the typed error with `downcast_ref` in one function, so adding
`.context(...)` anywhere in between cannot change the exit code.

`anyhow` was already in `[workspace.dependencies]`, used by nothing. `thiserror 2.0.20` is new; it
compiles to no runtime code and shares `syn` with the `clap` and `serde` derives already in the
build. Adding it is also what exposed `glimpse-ipc`'s "serde and tokio, nothing else" rule as
already false, so that rule is now stated as a test rather than a list. Specs 002, 007.

### 2026-08-21 · `glimpse-devtools` removed

A widget previewer earns its place when widgets are hard to reach any other way. These are not: a
widget in `glimpse-widgets` takes values and emits signals, so it can be built in a test with a
literal value and no daemon, and a `just nested` niri already gives a dev loop against the real
panel. What the previewer added on top was a second rendering path — its own fixture format, its own
CSS loading, its own watching of Blueprint output, all of which had to be kept in step with the
binaries or quietly stop resembling them.

The spec, `008_glimpse_devtools.md`, is deleted with the crate. The number is not reused and the
later specs are not renumbered: every cross-reference to `009` through `012` is a path, and
renumbering to close a gap would break all of them to save nothing. Specs 001, 002, 006, 011.

### 2026-08-21 · one schema crate, and no FIFO guard

Two reversals in `010`, taken together because both trade a property the project could not cash in.

The first is that the whole configuration schema now lives in `glimpse-config` and every binary
parses and validates every table. What that gives up is the rule that a reader never learns another
binary's schema, so the two version independently — but the four binaries are built from one
workspace at one version and released together, so there was never a moment at which one schema
could be older than another. The property was theoretical. What a single schema buys is real:
`--check-config` is exhaustive whichever binary runs it, `[appearance]` is one type rather than a
copy in the panel and a copy in the locker, and the closed set of top-level table names is
`deny_unknown_fields` on one struct rather than a second list kept in step by hand. The cost is that
a schema error anywhere now fails every binary's load, which is what a syntax error already did, and
which the load-failure rule already bounds.

The second is that a FIFO is no longer refused. The check was specified as open-then-inspect, but on
a FIFO the open is what blocks, so no inspection placed after it can help; refusing one needs
`O_NONBLOCK` in the open flags, and the flag's value comes from a C ABI crate. `libc` and `rustix`
were both weighed and both declined — a dependency on the whole of a platform-bindings crate for one
constant, in a crate that otherwise needs no unsafe and no bindings at all. A configuration file
symlinked at a FIFO now hangs the binary until a writer appears. Nobody writes that by accident, and
the regular-file check still refuses `/dev/zero`, a directory and a socket, which are the cases that
happen. Specs 002, 010.

### 2026-08-21 · drop-ins fail the whole load, like any other file

`010` originally gave a drop-in one exception the base file never got: a dangling symlink, an
oversized file, or one that resolved to something other than a regular file was skipped with a
warning rather than failing the load, on the theory that a stale link left by an uninstalled
package must not cost the user their session.

That theory was already covered by the load-failure rule one level up — a fresh start comes up on
defaults and a reload keeps the running configuration either way, whether the bad file is a drop-in
or the base file. The warn-and-skip path bought nothing the load-failure rule didn't already
provide, and cost a second code path (`Loaded.warnings`, a `Kind::Dropin` exception in two places)
to keep in step with it. A bad drop-in now fails the whole load, exactly like a bad base file. Spec
010.

### 2026-08-21 · missing is not broken

The previous entry treated "dangling" and "bad" as one case: any drop-in problem, symlink target
gone or not, failed the whole load. That conflated two different things. A file that is not there at
all is the same as a layer nobody wrote — the base file has always been allowed to be absent, and a
`--config <PATH>` that does not exist, or a drop-in whose symlink resolves to nothing, is exactly
that case with a different name. A file that exists and is wrong — a directory where a file belongs,
one over the size cap, one that fails to parse — is a different problem, and still fails the whole
load; being present and broken is not the same as never having been written. Spec 010.

### 2026-08-21 · a JSON Schema for editor tooling

TOML has no schema field of its own, but `taplo` — and the editors built on it, including Even
Better TOML — read one from a `#:schema <path>` header directive at the top of the document, or
from an out-of-band association. `schemars 1.2.2` derives that schema directly from the same
`Config` types `serde` already deserializes into, so the two can never describe different shapes.

`data/config.schema.json` is generated, not hand-written, the same way `data/config.default.toml`
is: `json_schema_document()` renders it, `just gen-config-schema` writes it, and a test fails if the
checked-in file drifts from the types. `default_document()`'s header now carries a `#:schema
/usr/share/glimpse/config.schema.json` directive, the path `just install` places the schema at, so a
config file that started from the shipped default gets completion without the user doing anything.

`Applet.settings` — the one field with no fixed shape, since it belongs to the applet type rather
than to this schema — is described as an open object via `#[schemars(with = "...")]`, keeping the
actual `toml::Table` type and its `Deserialize` impl untouched.

One known gap: `schemars` does not reflect a `#[serde(alias = ...)]` in an enum's schema, so
`night_light.schedule = "manual"` still parses correctly but an editor validating against the schema
will flag it. Accepted rather than worked around — `010` already calls `manual` the legacy spelling,
so nudging toward `schedule` is the right default. Spec 010.

### 2026-08-21 · `config show` needs no daemon either

Scaffolded to ask the running daemon for its merged configuration, on the theory that a reload
could leave the two disagreeing. The divergence is real but narrow — the window between an edit
landing and the next reload — and `010` already specifies that a *failed* reload keeps the running
configuration rather than partially applying, so outside that window disk and daemon always agree.
Sourcing from the daemon bought detecting that one narrow window, at the cost of needing the
get/topic protocol plumbing that no command has yet, including working when the daemon won't start
at all — the case configuration inspection matters most for. `config show` now calls the same
`glimpse_config::load` as `config path` and `config validate`. If the narrow-window case turns out
to matter, it comes back later as a real daemon topic once `get` exists. Spec 007.

### 2026-08-21 · applet settings are flat, not nested under `.settings`

`010` documented `[applets.<name>.settings]` as a nested sub-table, on the assumption that was where
`_old/` put applet-specific keys. Checking `_old/glimpse-core/src/config/panels.rs` shows otherwise:
`AppletConfig.settings` was `#[serde(flatten)]`, which folds every key besides `extends` into the
settings bucket straight from `[applets.<name>]` — a literal `[applets.<name>.settings]` sub-table
flattens to a *key* named `settings` one level too deep, not the fields inside it, confirmed by
probing both forms against the derive. A real config on this machine, written before this rewrite,
confirms the flat form is what was actually shipped; `config validate` reporting an "unknown field"
on it is what surfaced the spec's claim as wrong.

`Applet.settings` is now `#[serde(flatten)] toml::Table`, matching `_old/`'s mechanism.
`#[serde(deny_unknown_fields)]` cannot combine with `flatten`, so it comes off `Applet`.

A second real config on this machine also had no `extends` at all — `[applets.clock]` with only
`timezones`, relying on the section name itself naming the type. `004_panel.md` already documents
this exactly: "An applet name resolves to an `[applets.<name>]` entry, or to a built-in type of the
same name." `_old/`'s `AppletConfig.extends` was `Option<AppletType>` for the same reason, resolved
by the panel/zone builder (`resolve_applet` in `_old/glimpse-shell/src/panels/applets.rs`), not by
the generic config layer. `Applet.extends` is now `Option<Kind>` here for the same split: this crate
accepts either form without judging which type `<name>` names, because that resolution — and the
skip-with-warning `_old/` did when neither `extends` nor the name matches a type — belongs to
whatever builds applets from `Config`, which is `004`'s job, not `010`'s. `extends`, when given, is
still checked against `Kind`; nothing yet validates the contents of `settings` itself — that arrives
once an applet owns a typed settings struct, the same second pass `_old/` also had. Specs 004, 010.

### 2026-08-26 · an unreachable bus degrades services, it does not stop the daemon

`Buses` carries `Result<Connection, String>` per bus rather than two live connections, so `glimpsed`
starts with no D-Bus at all. A service that needs a bus reports its own `degraded` with the connect
error as the reason, which is what puts it on `system.services` where `glimpsectl doctor` can read
it. Rejected: failing `Daemon::run`, which trades the tray, notifications and wallpaper for a
missing system bus that only costs network, bluetooth and battery. The `WAYLAND_DISPLAY` row in
`003`'s environment table already sets this posture one line above; the D-Bus row now matches it.

The error is kept as a `String` rather than a `BusError` because `Buses` is cloned into every
service and the only consumer of the value is a `degraded` message. Specs 001, 003.

### 2026-08-26 · `glimpsectl` has no JSON output mode

`--json` is gone. It never earned its keep: the flag toggled compact against pretty JSON and no
human format existed at all, so every command emitted JSON either way and the "formatted output"
the flags table promised was never written. Rather than build a second output mode beside the one
that was missing, the tool now renders for people only.

Scripting keeps working through `get --field`, which prints one bare scalar, and through `watch`'s
one-line-per-event form. A consumer that genuinely wants frames should speak the protocol — that is
what `012_ipc.md` specifies and what the Python, TypeScript and Go SDKs are for. Reversing this
means writing the human renderer anyway, so nothing is foreclosed. Specs 007.

### 2026-08-26 · `--json` returns, as a passthrough on two commands

Reverses "`glimpsectl` has no JSON output mode" from earlier today. That entry was right that a
global flag toggling compact against pretty JSON was worth deleting, and wrong to conclude the
capability itself was. Removing it left no way to see what actually crossed the socket, which is
what the tool is for when the wire is the thing under suspicion.

The distinction that makes it work this time: `--json` is not an output format sitting beside the
rendered one, it is a passthrough. `get --json` prints the payload, `watch --json` prints the frame,
neither is re-serialized into a second shape, and neither is a global flag that every command has to
answer for. `topics` and `services` do not take it because they render topics a caller can read
directly with `get --json`. Specs 007.

