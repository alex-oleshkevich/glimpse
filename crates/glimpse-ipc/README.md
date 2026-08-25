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

Publishing is the next piece: `Server` has no `publisher()` yet, so `subscribe` succeeds and no
event ever arrives. Per-client writer tasks, newest-value coalescing and the buffered-byte cap land
with it, and the reconnect and resubscribe unit tests land with the fake server it makes possible.
Typed access — `Topic`, `Command`, `Cached<T>` — waits on where payload types live.

## Rules

**A dependency belongs here only if both ends of the socket need it**, plus `tracing` for
diagnostics. No zbus, no GTK, and no backend type in `topics/` — a payload that names one cannot be
generated for Python, TypeScript or Go. `topics/` and `frame.rs` are the input to `schemars` for the
Python, TypeScript and Go SDK types; a generator reads those modules, not the whole crate.

**Errors are `thiserror` enums, not `anyhow`.** A caller has to branch on them: the panel reconnects
after a transport failure but not after a daemon `CallError`, and `glimpsectl` maps four of them
onto the exit codes in `specs/007_glimpsectl.md`. `CallError` is the exception in the other
direction — it crosses the wire, so it is a serde payload and a `schemars` input, not an error
type.

**Transport lives here, routing lives in `glimpsed`.** Framing, reconnect and per-client writer
tasks are all "how bytes cross the socket" and both ends must agree on them. Deciding _which_ client
receives _which_ value is the broker's job and stays in the daemon.

**The request deadline belongs to the connection, not the caller.** A caller that wraps its own
future in `tokio::time::timeout` gives up without releasing the in-flight slot its request still
holds, so 32 abandoned calls lock the client out. `Client::connect` takes the timeout and the
connection task expires entries against it.

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

Spec: [`specs/001_architecture.md`](../../specs/001_architecture.md)
