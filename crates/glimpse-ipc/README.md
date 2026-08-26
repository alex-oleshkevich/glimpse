# glimpse-ipc

The wire between `glimpsed` and everything else: the frame format, the codec, and both ends of the
transport.

Both sides of the socket compile against this crate, so a renamed topic or a changed field is a
compile error rather than a runtime surprise — and the client and the server cannot drift, because
there is only one of each.

## Contents

- `lib.rs` — re-exports, `PROTOCOL_VERSION`, `VERSION`, `SOCKET_RELATIVE_PATH`, `socket_path`
- `frame.rs` — `Frame`, `Body`, `Status`, `Event`, `CallError`, `ErrorCode`: the NDJSON envelope
- `codec.rs` — `LinesCodec` for the bytes, `serde_json` for the frame, and the two policies on top
- `pattern.rs` — `matches`, the `*` and `**` rules a subscription is resolved against
- `client.rs` — connect, handshake, reconnect with backoff, resubscribe, request deadlines
- `server.rs` — listener, `trait Handler`, one task per client, handshake
- `topics/` — one module per domain, payload types only _(pending)_

Typed access — `Topic`, `Command`, `Cached<T>` — waits on where payload types live. The reconnect
and resubscribe unit tests wait on a fake server; nothing in `client.rs` or `server.rs` is covered
yet, which is the largest hole in this crate.

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
holds, so 32 abandoned calls lock the client out. `Client::connect` takes the timeout and the
connection task expires entries against it.

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

The handshake is answered with `hello_ack` even when the versions differ, so the client can name
both numbers before the daemon closes the connection; refusing silently would leave every mismatch
looking like a dead daemon.

`PROTOCOL_VERSION` is checked once at handshake, and a mismatch refuses the connection rather than
negotiating down: one clear message at connect time beats a payload that deserializes into the wrong
shape somewhere later. `glimpsed` static-asserts the number against its `--version` string, so a
bump cannot land with stale output.

Note for SDK generation: `schemars` emits types, not constants, so this is the single source of
truth for Rust only. A Python, TypeScript or Go generator has to be told the same socket name.
