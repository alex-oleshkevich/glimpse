# Consuming a Backend

The client half of zbus. Every mirror service in `glimpse-services/src/services/` is built this way.

## Anatomy of a Proxy

```rust
use zbus::proxy;

#[proxy(
    interface    = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NetworkManager {
    fn get_devices(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;

    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;

    #[zbus(property)]
    fn wireless_enabled(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_wireless_enabled(&self, enabled: bool) -> zbus::Result<()>;

    #[zbus(signal)]
    fn device_added(&self, path: zvariant::ObjectPath<'_>) -> zbus::Result<()>;
}
```

The macro applies to a `trait` and generates a struct named `<TraitName>Proxy` —
`NetworkManagerProxy` here. What each piece produces:

| You write | You get |
| --- | --- |
| `fn get_devices` | `async fn get_devices()` calling `GetDevices` (name is pascal-cased) |
| `#[zbus(property)] fn devices` | `async fn devices()` reading the `Devices` property |
| `#[zbus(property)] fn set_wireless_enabled` | writes `WirelessEnabled`; the `set_` prefix is what marks it a setter |
| `#[zbus(signal)] fn device_added` | `async fn receive_device_added() -> Result<DeviceAddedStream>` |
| any property with a change signal | `async fn receive_devices_changed() -> PropertyStream` |

**Always set `default_service` and `default_path`, or set `assume_defaults`.** Setting neither is
silent — the doc comment claims a warning, but 5.16 emits none. The only symptom is that `new()`
gains two parameters, so `SomeProxy::new(&conn)` fails with `E0061: this function takes 3 arguments`
(see `references/compile-traps.md`).

`gen_blocking` is ignored in this workspace — the `blocking-api` feature is off, so no
`...ProxyBlocking` type is generated at all.

Build with `new()` for the defaults, or `builder()` when the path varies per object:

```rust
let nm = NetworkManagerProxy::new(&system_conn).await?;

let ap = AccessPointProxy::builder(&system_conn)
    .path(ap_path)?
    .cache_properties(zbus::proxy::CacheProperties::No)
    .build()
    .await?;
```

## Property Caching

`CacheProperties` has three values; the default is `Lazily`:

| Value | Behaviour |
| --- | --- |
| `Yes` | `GetAll` at construction, then kept fresh from `PropertiesChanged` |
| `Lazily` | **default** — cache populated on first read of each property, then kept fresh |
| `No` | every read is a round trip |

With caching on, `proxy.devices().await` is a local read after the first call, and
`cached_property::<T>("Devices")` is a non-async read that returns `Ok(None)` when the cache has not
been populated.

Caching is right for objects you hold — the NM manager, a device, an adapter. Caching is wrong for
objects that churn: an `AccessPoint` you touch once while building a scan list costs an `AddMatch`
plus a `GetAll` if you let it cache, for a single property read. Use `CacheProperties::No` for
short-lived proxies.

**The cache is only as good as the peer's change signal.** A property whose introspection says
`emits-change` is safe. One declared `emits_changed_signal = "false"` disables caching for that
property and generates no `receive_*_changed` method, because the peer makes no promise to tell you.
Declare it to match the remote interface, not to match what you wish were true:

```rust
#[zbus(property, emits_changed_signal = "const")]
fn protocol_version(&self) -> zbus::Result<i32>;
```

`"const"` caches forever and generates no listener. `"invalidates"` behaves like `"true"` on the
proxy side — the signal names the property without carrying its value, and zbus re-fetches.

## Enumerate Once, Then Follow

The shape every mirror service takes. Subscribe *before* enumerating, or an object that appears in
the gap is lost:

```rust
async fn run(&mut self) -> zbus::Result<()> {
    let nm = NetworkManagerProxy::new(&self.conn).await?;

    // 1. Subscribe first.
    let mut added   = nm.receive_device_added().await?;
    let mut removed = nm.receive_device_removed().await?;

    // 2. Then enumerate.
    for path in nm.devices().await? {
        self.add_device(path).await;
    }

    // 3. Then follow.
    loop {
        tokio::select! {
            Some(sig) = added.next()   => self.add_device(sig.args()?.path.into()).await,
            Some(sig) = removed.next() => self.remove_device(sig.args()?.path.into()).await,
        }
    }
}
```

Signal streams yield a typed wrapper, not a raw message. `sig.args()?` returns a struct with one
accessor per signal argument, named after the parameter in the trait. The wrapper also derefs to
`zbus::message::Message` when you need the sender or path.

The same ordering rule applies to name ownership — the `request_name_with_flags` docs call it out
explicitly. Create the `NameAcquired`/`NameLost` stream before requesting the name.

## Object Trees

BlueZ and other `ObjectManager` services hand you the whole tree in one call rather than a property
list. `zbus::fdo::ObjectManagerProxy` is built in:

```rust
use zbus::fdo::ObjectManagerProxy;

let om = ObjectManagerProxy::builder(&self.conn)
    .destination("org.bluez")?
    .path("/")?
    .build()
    .await?;

let mut added   = om.receive_interfaces_added().await?;
let mut removed = om.receive_interfaces_removed().await?;

for (path, ifaces) in om.get_managed_objects().await? {
    if ifaces.contains_key("org.bluez.Device1") {
        self.add_device(path).await;
    }
}
```

`get_managed_objects()` returns
`HashMap<OwnedObjectPath, HashMap<OwnedInterfaceName, HashMap<String, OwnedValue>>>` — the same
shape `GetAll` would return, per interface, per object. It gives you every device *and* its
properties in a single round trip, which is why BlueZ enumeration should never loop over
`GetAll` calls.

To turn a returned `ObjectPath` straight into another proxy, annotate the method:

```rust
#[zbus(object = "Device")]
fn device_for(&self, addr: &str);          // returns DeviceProxy

#[zbus(property, object_vec = "AccessPoint")]
fn access_points(&self);                    // returns Vec<AccessPointProxy>
```

## Watching a Backend Come and Go

A backend that is not running is not an error state to retry out of — it is a fact to publish.
`receive_owner_changed()` on any proxy yields `Option<UniqueName>`: `Some` when the service appears,
`None` when it goes away.

```rust
let mut owner = nm.receive_owner_changed().await?;
while let Some(new_owner) = owner.next().await {
    match new_owner {
        Some(_) => self.reenumerate().await,      // came back: enumerate again
        None    => self.publish_degraded().await, // gone: say so, keep running
    }
}
```

This is the whole reconnect story. No backoff loop, no retry counter — rule 3.

## Fire and Forget

A method declared `no_reply` sends without waiting for a reply, and the trait method returns as soon
as the message is written. This is correct for `com.canonical.dbusmenu`'s `Event`, which is
specified as void:

```rust
#[zbus(no_reply)]
fn event(&self, id: i32, event_id: &str, data: &zvariant::Value<'_>, timestamp: u32)
    -> zbus::Result<()>;
```

Do not reach for it as a way to avoid awaiting a slow call. `AboutToShow` returns a `bool` that says
whether the layout changed; sending it `no_reply` throws away the answer and the menu renders stale.
A slow call belongs in `ctx.spawn`, not in `no_reply`.

## Types Off the Bus

| D-Bus | Rust | Note |
| --- | --- | --- |
| `s` | `String` | untrusted; cap length |
| `o` | `OwnedObjectPath` | not a `String`; parse errors are real |
| `ay` | `Vec<u8>` | **not** a string — NM `Ssid` is bytes and need not be UTF-8 |
| `y` | `u8` | NM `Strength` is `y`, not `u32` |
| `x` / `t` | `i64` / `u64` | MPRIS positions are microseconds in `x` |
| `v` | `zvariant::OwnedValue` | `try_into()` to the concrete type |
| `a{sv}` | `HashMap<String, OwnedValue>` | MPRIS `Metadata`, NM connection settings |

An owned type (`OwnedValue`, `OwnedObjectPath`) is needed whenever the value outlives the message —
which is always, if it is going into a topic. Borrowed forms (`Value<'_>`, `ObjectPath<'_>`) are for
arguments you are passing straight back out.

`a{sv}` maps are open. Read the keys you know and ignore the rest — a player that adds a vendor key
to `Metadata` must not break the mpris service. This is the wire-payload direction of the
`deny_unknown_fields` split in `.claude/rules/daemon.md`: config is strict, bus data is permissive.
