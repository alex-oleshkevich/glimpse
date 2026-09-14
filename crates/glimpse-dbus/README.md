# glimpse-dbus

The `zbus` proxy traits and typed provider clients used across Glimpse processes, plus the two
connections they run on.

## Contents

- `dbus.rs` — `Buses`, holding the session and system connections
- `clients/` — one module per bus service, each a set of `#[zbus::proxy]` trait declarations
- `clients/notifications.rs` — the notification proxy, typed handle and owned follower lifecycle

| Module                    | Bus     | What it fronts                          |
| ------------------------- | ------- | --------------------------------------- |
| `bluez`                   | system  | BlueZ adapters and devices              |
| `geoclue`                 | system  | GeoClue2 manager and client             |
| `login1`                  | system  | logind session, seat and idle hints      |
| `network_manager`         | system  | NetworkManager devices and connections   |
| `power_profiles`          | system  | power-profiles-daemon                    |
| `udisks2`                 | system  | UDisks2 removable media                  |
| `upower`                  | system  | UPower devices and battery state         |
| `mpris`                   | session | MPRIS players                            |
| `status_notifier_item`    | session | StatusNotifierItem tray entries          |
| `glimpse_lock`            | session | the lock screen's own name               |

## Rules

**Backend proxies carry no policy.** System-service modules only declare interfaces. A typed client
for a Glimpse-owned provider may additionally own its availability state and `NameOwnerChanged`
follower so every consuming process gets the same lifecycle behavior.

**Both connections are opened once and shared.** `Buses` is `Clone` and holds the session and system
connections together, because a service that needs one usually ends up needing the other, and two
connections inside one process would double the bus traffic and the failure modes. Each process
composition root connects once and clones them into its services.

**A bus that will not connect is a degraded service, not a dead daemon.** `Buses::connect` never
fails; each accessor returns `Result<&Connection, &str>` where the `Err` is why there is no
connection. A service that needs a bus reports its own unavailable or degraded state carrying that
reason. A session with no D-Bus still has a panel, a wallpaper and a lock screen.

**`Position` on `org.mpris.MediaPlayer2.Player` carries `emits_changed_signal = "false"`, and must
keep doing so.** MPRIS specifies that property as emitting no `PropertiesChanged`, but the macro
defaults an unannotated property to `"true"` — so with the connection's default `Lazily` caching the
first read is cached behind a listener that never fires, and every later read returns that same
number for the life of the proxy. `"false"` is the only value that both registers the property as
uncached and suppresses the `cached_*` getter; `"const"` suppresses the listener and keeps the
cache, which reproduces the same stale read by another route. The daemon log shows this working:
`Ignoring update of uncached property ...Position`.

**The attribute nests inside `property`.** `#[zbus(property(emits_changed_signal = "false"))]`, not
`#[zbus(property, emits_changed_signal = "false")]` — the flat spelling fails with
`unknown attribute`, which names the argument rather than the shape.

**Neither MPRIS proxy sets `default_service`**, because the destination is a different bus name per
player. Both are built through `builder().destination(name)`, and the name must be an owned
`String`: a `&str` ties the proxy's lifetime to that borrow and will not satisfy the `'static`
bound a subscription source requires.

**No `#[zbus::interface]` in this crate.** A proxy is a Glimpse process calling out and is
shareable; an interface is other applications calling in, and it needs a way back into the state of
the process that owns it. The object-server half of notifications lives in
`glimpse-notifications/src/provider.rs`; consumers such as the panel or lock only use this crate's
typed proxy handle.

**Signatures come from introspection, not from memory.** A proxy that disagrees with the running
service fails at the call, not at compile time, which is the expensive kind of wrong. The
project-local `zbus` skill under `.claude/skills/zbus/` carries introspected signatures for these
services; check against it rather than hand-writing a method name.

**No topic types here.** A payload belongs in `glimpse-contracts`, where it can be generated for the
other SDKs. A backend type that leaked into a payload could not be.
