# glimpse-proto

The wire vocabulary. Frame types, the `Topic` trait, every payload type, error codes, and
`PROTOCOL_VERSION`.

Both sides of the socket compile against this crate, so a renamed topic or a changed field is a
compile error rather than a runtime surprise.

## Contents

- `lib.rs` — re-exports, `PROTOCOL_VERSION`, `SOCKET_RELATIVE_PATH`, `socket_path`
- `frame.rs` — `Frame`, `Body`, `Status`: the NDJSON envelope *(pending)*
- `codec.rs` — frame to line, line to frame *(pending)*
- `topic.rs` — `trait Topic` binding a name to a payload type, plus matching rules *(pending)*
- `error.rs` — `CallError { code, message, retryable }` *(pending)*
- `topics/` — one module per domain, payload types only *(pending)*

## Rules

**serde and nothing else.** No tokio, no zbus, no GTK. This crate is the input to `schemars` for
generating the Python, TypeScript and Go SDK types; any other dependency closes that door.

Payloads derive `PartialEq`. That is the equality gate which stops a service republishing an
identical value, and it is why a 200-event volume drag does not become 200 frames.

`Frame.data` stays `serde_json::Value`. The broker routes topics it has no compile-time knowledge
of, such as `tray.item.{id}`, so typing happens one layer up at the `Topic` boundary.

`socket_path` and `SOCKET_RELATIVE_PATH` are here for the same reason as `PROTOCOL_VERSION`: both
ends must agree on them exactly, `glimpse-client` may not depend on `glimpsed`, and `glimpsed`
depending on a client crate is backwards. The alternative is the same string literal in two places
with nothing enforcing that they match.

It takes the runtime directory rather than reading `XDG_RUNTIME_DIR`, which keeps the serde-only
rule intact — resolving the directory needs `dirs`, and that belongs to the caller.

Note for SDK generation: `schemars` emits types, not constants, so this is the single source of
truth for Rust only. A Python, TypeScript or Go generator has to be told the same string.

`PROTOCOL_VERSION` is checked once at handshake, and a mismatch refuses the connection rather than
negotiating down: one clear message at connect time beats a payload that deserializes into the wrong
shape somewhere later. `glimpsed` static-asserts the number against its `--version` string, so a
bump cannot land with stale output.

Spec: [`specs/001_architecture.md`](../../specs/001_architecture.md)
