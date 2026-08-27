# glimpse-ipc

The wire between `glimpsed` and everything else: the frame format, the codec, and both ends of the
transport.

Both sides of the socket compile against this crate, so a renamed topic or a changed field is a
compile error rather than a runtime surprise — and the client and the server cannot drift, because
there is only one of each.

## Contents

- `lib.rs` — re-exports, `VERSION`, `SOCKET_RELATIVE_PATH`, `socket_path`
- `frame.rs` — `Frame`, `Body`, `Status`, `Event`, `CallError`, `ErrorCode`: the NDJSON envelope
- `codec.rs` — `LinesCodec` for the bytes, `serde_json` for the frame, and the two policies on top
- `pattern.rs` — `matches`, the `*` and `**` rules a subscription is resolved against
- `client.rs` — connect, handshake, reconnect with backoff, resubscribe, request deadlines
- `server.rs` — listener, `trait Handler`, one task per client, handshake
- `topics/` — one module per domain, payload types only _(pending)_

Typed access — `Topic`, `Command`, `Cached<T>` — waits on where payload types live. `client.rs`
covers the dial: an absent daemon, a live one, and a socket that accepts without answering. The
reconnect and resubscribe paths still wait on a fake server, and `server.rs` is uncovered, which is
the largest hole left in this crate.

## Rules

**A dependency belongs here only if both ends of the socket need it**, plus `tracing` for
diagnostics. No zbus, no GTK, and no backend type in `topics/` — a payload that names one cannot be
generated for Python, TypeScript or Go. `topics/` and `frame.rs` are the input to `schemars` for the
Python, TypeScript and Go SDK types; a generator reads those modules, not the whole crate.

**Errors are `thiserror` enums, not `anyhow`.** A caller has to branch on them: the panel reconnects
after a transport failure but not after a daemon `CallError`, and `glimpsectl` maps four of them
onto its own exit codes. `CallError` is the exception in the other
direction — it crosses the wire, so it is a serde payload and a `schemars` input, not an error
type.

**Transport lives here, routing lives in `glimpsed`.** Framing, reconnect and per-client writer
tasks are all "how bytes cross the socket" and both ends must agree on them. Deciding _which_ client
receives _which_ value is the broker's job and stays in the daemon.

**The request deadline belongs to the connection, not the caller.** A caller that wraps its own
future in `tokio::time::timeout` gives up without releasing the in-flight slot its request still
holds, so 32 abandoned calls lock the client out. `REQUEST_TIMEOUT` is a constant of this module and
the connection task expires entries against it — no caller passes one, because the deadline exists
to release that slot and nobody outside the connection can hold a useful opinion about when.

It bounds the handshake too. A daemon that accepts the connection and then answers nothing leaves
`dial` waiting on a frame that never arrives — and `dial` is called from inside the reconnect loop,
so an unbounded wait there stops the reconnection rather than delaying it: the loop never reaches
its backoff and never tries again. `glimpsectl` had the same hole from the other side, since the
request deadline only ever covered requests and a wedged daemon hung the command outright.

**`connect` fails on a missing daemon, `open` waits for one.** Both dial once before returning; they
differ only in what a failed dial means. `connect` returns `NotListening`, which a one-shot caller
such as `glimpsectl` maps to an exit code. `open` keeps the failure and returns a client anyway,
leaving the reconnect loop to reach the daemon whenever it appears — which is what the UI binaries
need. `glimpse-panel`, `glimpse-wallpaper` and `glimpse-sunset` carry `Wants=glimpsed.service` and
never `Requires=`, and `glimpse-lock` carries no relation to it at all, so every one of them can
start before the daemon and that is ordinary rather than a failure.

Both dial eagerly because the loop's first act is to back off. A client handed `None` waits a full
`BACKOFF_MIN` doubling before its first attempt, so a panel restarted against a daemon that is
already up would sit disconnected for half a second with nothing wrong.

The connect is logged at `info` only after an absence — a one-shot CLI would otherwise print a line
on every invocation, and a client that reached the daemon on its first dial has nothing to diagnose.
Every reconnect after a loss is logged, which is the case worth seeing.

**One cap on in-flight requests, held as a semaphore permit.** The permit is taken in `Client::ask`
and travels with the request until its reply settles, so a request cannot be outstanding without
holding one. A second bound — a queue in front of the cap — would not be a cap at all: it turns the
`LimitExceeded` a caller can act on into a wait it cannot see.

**A slow reader loses intermediate values and never a current one.** Both ends coalesce newest-wins
per topic: `Outbox` on the server, `Mailbox` on the client. A bounded channel does the opposite —
full means the value being handed over is dropped, which is the newest — and a subscriber that falls
behind then renders a stale value forever rather than skipping to the current one.

**A `get` or a `call` is answered in its own task.** Awaiting a handler in the read loop would stop
the server taking the client's next frame off the socket, so one command waiting on a wedged backend
would hold up every unrelated request on that connection. `Frame.id` correlates and the outbox
orders responses independently, so completing out of order is already legal.

**Dropping a `Subscription` releases its pattern.** The daemon caps patterns per connection, so a
caller that subscribes and drops in a loop would otherwise walk into that cap holding subscriptions
nothing reads. The release is automatic, which is the only way it does not get forgotten.

A round trip — client encodes, server decodes, server responds, client decodes — is a unit test in
this crate. That is the reason the two ends are not separate crates: split, the only place they met
was an integration test over a temporary socket.

Payloads derive `PartialEq`. That is the equality gate which stops a service republishing an
identical value, and it is why a 200-event volume drag does not become 200 frames.

`Frame.data` stays `serde_json::Value`. The broker routes topics it has no compile-time knowledge
of, such as `tray.item.{id}`, so typing happens one layer up at the `Topic` boundary.

**The wire is not versioned.** There is no `PROTOCOL_VERSION` and no compatibility check, because
the only thing a version number ever did here was refuse a working client whose binary was built at
a different time from the daemon's — a failure the check created rather than caught. Payload types
accept unknown fields, which is what actually carries a mixed-age pair of binaries.

`hello` survives, carrying nothing. It is what tells a client that the path it was pointed at is a
glimpse daemon rather than some other program's socket, which `--socket` makes reachable and nothing
else can check; without it `connect` succeeds against anything and the first symptom is a timeout on
an unrelated request. It must still be the first frame — anything else closes the connection.

On the wire that is `{"type":"hello","data":{}}`, answered with
`{"type":"hello_ack","data":{"daemon_version":"…"}}`. The empty `data` object is load-bearing: it is
what lets a client built before the version was dropped still connect, since its
`{"protocol":1}` is read as an unknown field and ignored. Bare `{"type":"hello"}` is refused, which
matters only when hand-typing one into `socat`.

Note for SDK generation: `schemars` emits types, not constants, so this is the single source of
truth for Rust only. A Python, TypeScript or Go generator has to be told the same socket name.
