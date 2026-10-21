---
name: zbus
description: D-Bus clients and services in Rust with zbus 5. Use for any code in glimpse-services or a provider binary that consumes NetworkManager, BlueZ, logind, UPower, MPRIS, StatusNotifierItem or com.canonical.dbusmenu, or that exports org.freedesktop.Notifications or org.kde.StatusNotifierWatcher. Covers crate features and the tokio integration, #[proxy] and #[interface], property caching, signal streams and the wake/events subscription helpers, a connection that stops answering, bus name ownership, and compile traps keyed by error text. Trigger on the dependency, not the wording — if a file imports zbus, this applies.
---

# zbus

D-Bus in `glimpse-services` and the provider binaries. Every mirror service is a zbus client; the owned
services are zbus servers.

**The workspace is locked to zbus 5.19.0 / zbus_macros 5.19.0** — check `Cargo.lock` rather than
this line, which has been stale once. The signatures in
`references/interfaces.md` were captured by introspecting a live session bus and system bus, not
copied from documentation.

**Core principle:** the backend owns the state. A proxy is how a service *observes* state; it is
never where state lives. When a proxy and a service disagree, the backend is right — re-read, do
not reconcile locally.

## Verify Signatures Before Asserting Them

zbus churns hard across majors. The `#[dbus_proxy]`/`#[dbus_interface]` aliases that still existed
in 4.x are gone, and `SignalContext` survives only as a deprecated alias for `SignalEmitter` — which
`just lint` turns into a build failure. Any example older than zbus 5 needs rewriting, not
adapting. Trust the installed crate source over memory:

```bash
grep -rn "pub async fn request_name_with_flags" ~/.cargo/registry/src/*/zbus-5.*/src/connection/mod.rs
grep -rn "pub enum CacheProperties" -A14 ~/.cargo/registry/src/*/zbus-5.*/src/proxy/builder.rs
sed -n '/Attribute macro for defining D-Bus proxies/,/proc_macro_attribute/p' \
  ~/.cargo/registry/src/*/zbus_macros-5.*/src/lib.rs
```

The macro attributes are documented only in `zbus_macros/src/lib.rs` doc comments — that file is the
reference for every `#[zbus(...)]` sub-attribute. Note that those doc comments have drifted from the
code in at least one place, so a grep locates a symbol and only a typecheck confirms it. Keep a
throwaway crate with `zbus` as its only dependency and a warm `target/`; `cargo check --offline`
on it answers a signature question in under a second, and every entry in
`references/compile-traps.md` was produced that way.

For the remote side, introspect rather than guess. `busctl` is a research tool; it never appears in
shipped code (see Critical constraints in `AGENTS.md`):

```bash
busctl --system introspect org.freedesktop.NetworkManager /org/freedesktop/NetworkManager
busctl --user introspect org.kde.StatusNotifierWatcher /StatusNotifierWatcher
busctl --user monitor org.freedesktop.Notifications        # watch traffic live
```

## Cargo Setup — Get This Right First

```toml
# root Cargo.toml, [workspace.dependencies] — never in a crate manifest
zbus = { version = "5", default-features = false, features = ["tokio"] }
```

`default-features = false` is not optional. zbus's defaults are
`["async-io", "blocking-api"]`:

- **`async-io`** pulls `async-executor`, `async-io`, `async-process`, `async-lock` and `blocking`,
  and makes the connection spawn **its own executor thread**. Every glimpse binary already has a tokio runtime;
  a second one is a second scheduler competing for the same work.
- **`blocking-api`** generates a `TraitNameProxyBlocking` type beside every async proxy. Turning it
  off means the blocking API *does not exist to be called* — the compiler now enforces the
  no-blocking-calls-in-a-handler rule in `.claude/rules/daemon.md` instead of a reviewer having to
  catch it.

With `features = ["tokio"]` and `async-io` off, zbus uses the ambient tokio runtime. You do **not**
need `internal_executor(false)` and you do **not** need an `executor().tick()` loop — those are for
the async-io build. Enabling neither feature is a `compile_error!`.

## Connection Model

A process holds **two** connections for its whole lifetime, created once at startup and cloned into
services:

| Bus | Used for |
| --- | --- |
| system | NetworkManager, BlueZ, logind, UPower |
| session | MPRIS, StatusNotifierItem, dbusmenu, and the names the providers own |

`Connection` is cheap to clone — it is an `Arc` internally, and clones share one socket. Clone it
into each service; never open a per-service connection and never open one per method call.

A service that needs a bus reaches it through the connection handed to it at construction. This is
The resource is injected, so a service test can hand over a
connection to a fixture bus instead.

## Signal Streams Never Stall the Connection

zbus gives every signal subscription a 64-message queue, and its single socket reader **waits** for
room in a full queue before it reads anything else, method replies included. One subscription left
unread for a moment (its consumer is awaiting a call, sleeping, or busy in another `select!` branch)
therefore freezes every signal and every call on that connection, permanently if the consumer is
waiting for a reply that sits behind the full queue. This froze the whole panel once: a burst of
`UnitRemoved` filled an applet watch while its loop served the applet socket.

So no code holds a raw zbus signal stream. Every `receive_<signal>`, `receive_name_owner_changed*`,
`receive_all_signals`, `MessageStream::for_match_rule` and `MessageStream::from` goes straight into
one of the two `glimpse_dbus` helpers at creation; their forwarder reads zbus's queue continuously,
so it never fills:

- **`glimpse_dbus::wake(stream)`** — the signal only means "read it again" (`New*`, most
  `PropertiesChanged`, `DeviceAdded`). Keeps at most one pending `()`; any burst becomes one re-read.
- **`glimpse_dbus::events(stream)`** — the payload matters (`UnitRemoved`, `NameOwnerChanged`,
  `InterfacesAdded`). Bounded to 64, the oldest dropped first, delivered as `Delivery::Item(t)`.
  Filter before it (`filter_map` for the one unit or name you care about) so only relevant signals
  are queued.
- **`Delivery::Missed`** means messages were dropped. Handle it with the site's existing re-read —
  re-enumerate, `NameHasOwner`/`GetNameOwner`, re-read the property — never by ignoring it and never
  with a reset heavier than the state that may have been lost (do not tear down agents, scans or
  pending connects to recover a dropped signal).
- `PropertyStream`s (`receive_<prop>_changed` from `#[zbus(property)]`) are exempt: zbus fills them
  from its own always-polled task and they cannot block the reader.
- Do not make D-Bus calls inside the stream handed to a helper (`events(s.then(|_| proxy.call()))`);
  the forwarder would wait on the call and the queue behind it fills again. Call after the helper.

`crates/glimpse-dbus/tests/signal_streams.rs` fails `just verify` on a raw signal stream outside
`glimpsectl` and test code.

## Decision Table

| Task | Go to |
| --- | --- |
| Follow a backend's state (network, bluetooth, audio, battery, mpris, brightness) | `references/proxies.md` |
| Export an interface a provider owns (notifications, weather, night light) | `references/services.md` |
| Take or track a well-known bus name | `references/services.md` → Name Ownership |
| A zbus call fails to compile | `references/compile-traps.md` |
| Need the exact signature of a remote method, property or signal | `references/interfaces.md` |
| Property changed but nothing fired | `references/proxies.md` → Caching and `emits_changed_signal` |
| A handler can block on a D-Bus round trip | `.claude/rules/daemon.md` → move the `Responder` into `ctx.spawn` |
| Subscribe to a signal, or the bus stopped answering | Signal Streams Never Stall the Connection, above |

## The Five Rules

1. **One proxy per remote object, built once and kept.** Building a proxy performs an
   `AddMatch` round trip and, with default caching, a `GetAll`. Rebuilding one per read turns a
   cached property access into two round trips.
2. **Enumerate once, then follow signals.** Every mirror service does the same thing: read the
   collection property (or `GetManagedObjects`), then subscribe. Never poll, never re-enumerate on
   a timer.
3. **Never retry on top of a backend that retries.** NetworkManager already has reconnect policy.
   A zbus error means *this call* failed; it does not mean a service should start a loop.
4. **A handler that can block moves its `Responder` into `ctx.spawn`.** Handlers run serially on
   `&mut self`. One slow `AboutToShow` otherwise freezes the whole service.
5. **Treat every value off the bus as hostile.** Tray `Title`, notification `summary`/`body`, MPRIS
   `xesam:title`, and NM `Ssid` are attacker-controlled and unbounded. `Ssid` is `ay` and need not
   be valid UTF-8. Cap length and sanitize before any of it reaches a topic, let alone a label.

## Errors

`zbus::Error` is the transport-level error; `zbus::fdo::Error` is the set of standard D-Bus errors
a peer can return. The variants that actually come up:

| Variant | Means |
| --- | --- |
| `Error::MethodError(name, detail, msg)` | the peer returned a D-Bus error — the common case |
| `Error::NameTaken` | another process owns the name; the relevant service is `degraded`, not dead |
| `Error::InterfaceExists(iface, path)` | you exported the same interface twice at one path |
| `Error::InputOutput` | the bus connection died |
| `fdo::Error::ServiceUnknown` | nobody owns that name — the backend is not running |
| `fdo::Error::UnknownObject` / `UnknownInterface` | the object vanished between enumeration and use |

`ServiceUnknown` and `UnknownObject` are normal, not exceptional: devices disappear, players quit.
Handle them by dropping the item, not by logging an error and retrying.

A service that cannot reach its backend at all publishes `degraded` and keeps running. It does not
panic and it does not exit.

## Definition of Done

- `zbus` is declared once in `[workspace.dependencies]` with `default-features = false`.
- No `zbus::blocking::*` anywhere. It should not compile; if it does, the features are wrong.
- Every proxy is built once and stored, not built per call.
- Every signal stream goes through `glimpse_dbus::wake` or `glimpse_dbus::events` at creation, and
  every `Delivery::Missed` re-reads state.
- Every mirror service enumerates once and then follows signals.
- No `unwrap()` or `expect()` on a bus result in a handler or a provider's object-server method.
- Every handler that makes a D-Bus call either returns promptly or moves its `Responder` into
  `ctx.spawn`.
- Strings taken off the bus are length-capped before publication.
- `ServiceUnknown` and `UnknownObject` drop the item instead of retrying.
