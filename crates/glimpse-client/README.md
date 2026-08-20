# glimpse-client

Async client for the `glimpsed` socket. Every UI binary and `glimpsectl` uses it; nothing talks to
the socket by hand.

## What it does

- Connects to `$XDG_RUNTIME_DIR/glimpse/glimpsed.sock` and reconnects with backoff
- Restores subscriptions after a reconnect, so callers never handle it
- Caches the latest value per topic and hands out typed values via `Topic`
- Correlates `call` requests with their results by frame id

This crate hands out a connection, never a path. A caller that wants a different socket passes one
in; a caller that does not never learns where the default is. The default comes from
`glimpse_proto::socket_path`, which is also what `glimpsed` binds.

Nothing is implemented yet.

Because topics are state cells, reconnecting is the same as subscribing: a fresh snapshot arrives
and the caller's view is correct again. There is no replay path and no missed-event handling.

Spec: [`specs/001_architecture.md`](../../specs/001_architecture.md)
