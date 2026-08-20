# glimpse-proto

The wire vocabulary. Frame types, the `Topic` trait, every payload type, error codes, and
`PROTOCOL_VERSION`.

Both sides of the socket compile against this crate, so a renamed topic or a changed field is a
compile error rather than a runtime surprise.

## Contents

- `frame.rs` — `Frame`, `Body`, `Status`: the newline-delimited JSON envelope
- `topic.rs` — `trait Topic` binding a name to a payload type, and pattern-matching rules
- `error.rs` — `CallError { code, message, retryable }`
- `topics/` — one module per domain, payload types only

## Rules

**serde and nothing else.** No tokio, no zbus, no GTK. This crate is the input to `schemars` for
generating the Python, TypeScript and Go SDK types; any other dependency closes that door.

Payloads derive `PartialEq`. That is the equality gate which stops a service republishing an
identical value, and it is why a 200-event volume drag does not become 200 frames.

`Frame.data` stays `serde_json::Value`. The broker routes topics it has no compile-time knowledge
of, such as `tray.item.{id}`, so typing happens one layer up at the `Topic` boundary.

Spec: [`specs/001_architecture.md`](../../specs/001_architecture.md)
