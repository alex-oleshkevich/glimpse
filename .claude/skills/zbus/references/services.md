# Exporting an Interface

The server half of zbus. glimpsed owns two names on the session bus, and both are *owned services*
in the sense of `specs/001_architecture.md` — there is no backing daemon, so glimpsed is the store.

| Name | Object path | Interface |
| --- | --- | --- |
| `org.freedesktop.Notifications` | `/org/freedesktop/Notifications` | `org.freedesktop.Notifications` |
| `org.kde.StatusNotifierWatcher` | `/StatusNotifierWatcher` | `org.kde.StatusNotifierWatcher` |

## Anatomy of an Interface

`#[interface]` applies to an `impl` block, not a trait. Everything in the block is exported unless
it is marked otherwise.

```rust
use zbus::{interface, object_server::SignalEmitter};

struct Watcher {
    items: Vec<String>,
    hosts: usize,
}

#[interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    async fn register_status_notifier_item(
        &mut self,
        service: &str,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = hdr.sender().ok_or(zbus::fdo::Error::Failed("no sender".into()))?;
        let id = resolve_item_id(sender, service)?;
        self.items.push(id.clone());
        self.registered_status_notifier_items_changed(&emitter).await?;
        Watcher::status_notifier_item_registered(&emitter, &id).await
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.clone()
    }

    #[zbus(property, emits_changed_signal = "const")]
    fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(signal)]
    async fn status_notifier_item_registered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;
}
```

Method names are pascal-cased on the wire, so `register_status_notifier_item` becomes
`RegisterStatusNotifierItem`. Override with `#[zbus(name = "...")]`.

### Injected arguments

Any argument marked with one of these is filled in by zbus and does not appear on the wire:

| Attribute | Type | Use |
| --- | --- | --- |
| `#[zbus(header)]` | `Header<'_>` | the caller's unique name via `hdr.sender()`, and the object path |
| `#[zbus(signal_emitter)]` | `SignalEmitter<'_>` | emit signals from inside the method |
| `#[zbus(connection)]` | `&Connection` | make a call back out |
| `#[zbus(object_server)]` | `&ObjectServer` | export or remove another object |

`#[zbus(header)]` is how the SNI watcher gets item identity. A caller may pass either a bus name or
an object path to `RegisterStatusNotifierItem`; the sender in the header is the authoritative half.
Item identity comes from the item's own `Id` property, not from the bus name — see
`references/interfaces.md`.

### Properties

A getter takes no argument; a setter is the same name with a `set_` prefix. Either may return
`zbus::fdo::Result<T>` if it can fail, or a bare `T` if it cannot.

Each property generates two extra methods on your type:

- `<property>_changed(&emitter)` — emits `PropertiesChanged` **with** the new value
- `<property>_invalidated(&emitter)` — emits it **without** the value

A setter calls `_changed` for you. Any other mutation — an item registering, a notification
arriving — must call it explicitly, or subscribers never learn. Prefer `_changed`;
`_invalidated` forces every peer to fetch, which is traffic you pay for on every listener.

### Signals

A signal is a method declaration with no body. Its first parameter is `&SignalEmitter<'_>` and it
does not appear on the wire. The macro also generates a `<Interface>Signals` trait carrying the same
methods *without* the emitter argument, implemented for both `SignalEmitter<'_>` and
`InterfaceRef<Interface>` — that trait is how you emit from outside a method body.

### Errors

Return `zbus::fdo::Result<T>` to reply with a standard D-Bus error. For a domain error with its own
name, derive `DBusError`:

```rust
#[derive(zbus::DBusError, Debug)]
#[zbus(prefix = "me.aresa.Glimpse")]
enum Error {
    #[zbus(error)]
    ZBus(zbus::Error),      // this variant also gives you From<zbus::Error>
    UnknownItem(String),    // an optional String field is the human-readable description
    Busy,
}
```

## Serving and Name Ownership

Prefer the connection builder: interfaces are live the instant the connection is, so there is no
window where the name is owned but a call would 404.

```rust
let conn = zbus::connection::Builder::session()?
    .name("org.kde.StatusNotifierWatcher")?
    .serve_at("/StatusNotifierWatcher", Watcher::default())?
    .build()
    .await?;
```

`serve_at` adds `Peer`, `Introspectable` and `Properties` on your behalf. Declaring any of them
yourself makes `build()` fail.

To export later — a tray item's menu, a per-object interface — use the `ObjectServer`:

```rust
let server = conn.object_server();
server.at("/StatusNotifierWatcher", Watcher::default()).await?;  // Ok(false) if already there
server.remove::<Watcher, _>("/StatusNotifierWatcher").await?;    // Ok(false) if it wasn't
```

Note the asymmetry: `Builder::serve_at` *replaces* an interface already at that path, while
`ObjectServer::at` returns `Ok(false)` and leaves the existing one alone.

To emit a signal or mutate state from outside a method, take an `InterfaceRef`:

```rust
let iface: zbus::object_server::InterfaceRef<Watcher> =
    conn.object_server().interface("/StatusNotifierWatcher").await?;

iface.get_mut().await.items.push(id.clone());
iface.status_notifier_item_registered(&id).await?;   // via the generated WatcherSignals trait
```

### Taking the name

`Connection::request_name` implies `DoNotQueue` and fails with `Error::NameTaken` if someone else
holds it. `request_name_with_flags` gives you the choice:

```rust
use zbus::fdo::{RequestNameFlags, RequestNameReply};

let reply = conn
    .request_name_with_flags("org.kde.StatusNotifierWatcher", RequestNameFlags::AllowReplacement.into())
    .await?;
// RequestNameReply::PrimaryOwner | InQueue | Exists | AlreadyOwner
```

**Create the ownership streams before requesting the name.** The crate documents this caveat
directly: a `NameAcquired` or `NameLost` emitted between the request and the stream's creation is
lost.

```rust
let dbus = zbus::fdo::DBusProxy::new(&conn).await?;
let mut lost = dbus.receive_name_lost().await?;     // first
conn.request_name(name).await?;                      // then
```

`NameTaken` is **not** fatal. Another notification daemon — dunst, mako, a Plasma session — already
owns the name. The affected service publishes `degraded` and the rest of glimpsed runs normally.
This is the packaging conflict `specs/009_systemd.md` describes, and reporting it on
`system.services` rather than only in the log is what makes it diagnosable.

Track `NameLost` for the whole process lifetime, not just at startup. Requesting with
`AllowReplacement` means someone can take the name later.

## Method Ordering and `spawn`

By default `#[interface]` spawns a task per method call, so calls may complete out of order. Set
`spawn = false` to force sequential handling:

```rust
#[interface(name = "org.kde.StatusNotifierWatcher", spawn = false)]
```

The trade-off is real and documented: with `spawn = false`, **making a D-Bus call from inside an
interface method can deadlock**. Use it only for an interface whose methods mutate state and make no
outbound calls. The SNI watcher qualifies — register and unregister are pure state edits. The
notifications interface does not: `Notify` may need to reach back out, so leave it on the default.

This is a different axis from the service handler rule in `.claude/rules/daemon.md`. That rule is
about the glimpse service loop; `spawn` is about zbus's own dispatch. Both can be in play on the
same code path.

## Testing

An exported interface is testable without a session bus. Build a peer-to-peer connection pair, or
point a `Builder::address` at a private bus started for the test. Because a service receives its
`Connection` rather than opening one, the test injects a fixture connection the same way a service
test injects a mock `WaylandEdge`.
