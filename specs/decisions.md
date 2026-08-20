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
