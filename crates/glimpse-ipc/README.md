# glimpse-ipc

The wire between `glimpsed` and everything else: the frame format, the codec, and both ends of the
transport.

Both sides of the socket compile against this crate, so a renamed topic or a changed field is a
compile error rather than a runtime surprise — and the client and the server cannot drift, because
there is only one of each.

## Contents

- `lib.rs` — re-exports, `PROTOCOL_VERSION`, `SOCKET_ENV`, `SOCKET_RELATIVE_PATH`, `socket_path`
- `frame.rs` — `Frame`, `Body`, `Status`: the NDJSON envelope *(pending)*
- `codec.rs` — frame to line, line to frame *(pending)*
- `topic.rs` — `trait Topic` binding a name to a payload type, plus matching rules *(pending)*
- `error.rs` — `CallError { code, message, retryable }` *(pending)*
- `client/` — connect, reconnect with backoff, resubscribe, typed topic cache *(pending)*
- `server/` — listener, per-client reader and writer tasks, byte caps *(pending)*
- `topics/` — one module per domain, payload types only *(pending)*

Only `lib.rs` exists so far.

## Rules

**serde and tokio, nothing else.** No zbus, no GTK. `topics/` and `frame.rs` are the input to
`schemars` for the Python, TypeScript and Go SDK types; a generator reads those modules, not the
whole crate.

**Transport lives here, routing lives in `glimpsed`.** Framing, reconnect and per-client writer
tasks are all "how bytes cross the socket" and both ends must agree on them. Deciding *which* client
receives *which* value is the broker's job and stays in the daemon.

A round trip — client encodes, server decodes, server responds, client decodes — is a unit test in
this crate. That is the reason the two ends are not separate crates: split, the only place they met
was an integration test over a temporary socket.

Payloads derive `PartialEq`. That is the equality gate which stops a service republishing an
identical value, and it is why a 200-event volume drag does not become 200 frames.

`Frame.data` stays `serde_json::Value`. The broker routes topics it has no compile-time knowledge
of, such as `tray.item.{id}`, so typing happens one layer up at the `Topic` boundary.

`socket_path`, `SOCKET_ENV` and `SOCKET_RELATIVE_PATH` are here for the same reason as
`PROTOCOL_VERSION`: both ends must agree on them exactly. `socket_path` *discovers* — it returns the
first candidate that is on disk, the one named by `SOCKET_ENV` before the default under the runtime
directory — so it answers for clients. The daemon binds instead, and builds the path itself: a
socket that already exists is its refusal to start, not its answer.

`PROTOCOL_VERSION` is checked once at handshake, and a mismatch refuses the connection rather than
negotiating down: one clear message at connect time beats a payload that deserializes into the wrong
shape somewhere later. `glimpsed` static-asserts the number against its `--version` string, so a
bump cannot land with stale output.

Note for SDK generation: `schemars` emits types, not constants, so this is the single source of
truth for Rust only. A Python, TypeScript or Go generator has to be told the same socket name.

Spec: [`specs/001_architecture.md`](../../specs/001_architecture.md)
