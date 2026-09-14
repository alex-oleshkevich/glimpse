# glimpse-dbus

The `zbus` proxy traits and typed provider clients used across Glimpse processes, plus the two
connections they run on.

## Contents

- `dbus.rs` — `Buses`, holding the session and system connections
- `clients/` — one module per bus service, each a set of `#[zbus::proxy]` trait declarations
- `clients/notifications.rs` — the notification proxy, typed handle and owned follower lifecycle
- `clients/weather.rs` — weather wire values, typed proxy, conversions and owner follower

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
| `notifications`           | session | the Glimpse notification provider        |
| `weather`                 | session | the Glimpse weather provider             |

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
the process that owns it. Object-server halves live in their owning binary crates; consumers such
as the panel or lock only use this crate's typed proxy handles.

**Signatures come from introspection, not from memory.** A proxy that disagrees with the running
service fails at the call, not at compile time, which is the expensive kind of wrong. The
project-local `zbus` skill under `.claude/skills/zbus/` carries introspected signatures for these
services; check against it rather than hand-writing a method name.

**A domain type lives here only when a provider decodes it off the bus.** `glimpse-services` depends
on this crate and never the reverse, so weather and notification models sit beside their decoders
here while everything else sits beside the service that owns it. A backend type never reaches one:
what a client hands back is the published model, not the shape some daemon happened to store.

**A decoder belongs beside the wire type it undoes.** `weather::decode_snapshot` and
`notifications::decode_snapshot` are the readers' counterparts to the encoders each provider uses,
so a wire change touches one file rather than every consumer. `clients/mod.rs` holds what both
need — `optional_clean` and `epoch` — because two copies of a timestamp decoder is two ways for
them to disagree.

**A provider state carries decoded values, never the wire tuple.** `NotificationsProviderState.view`
is a `NotificationsView`, the way `WeatherProviderState.status` is a `WeatherStatus`, so
`follow_provider` decodes once and every consumer shares the result. It held the raw
`NotificationsSnapshot` until `glimpse-kyt0.9.8`, and the panel had grown its own positional
destructure and its own 14-field decoder to undo it — one that capped nothing and dropped
`dnd.until` through a `(dnd, _)` pattern. That is the copy that reached a `Gtk.Label`, so the crate
that never renders anything was the one honouring the cap. A snapshot that cannot be decoded is
`unavailable` with the reason, which is the same shape a dead provider already produces.

**A reader caps text the writer already capped.** The provider bounds what it sends, but the owner
of a well-known name is whoever claimed it, so a decoder that skipped the cap would be trusting a
bus name rather than a process. The cap tables are therefore per-direction and need not match: the
store caps an image *path* at 4096 while a themed icon *name* stops at 200, and reusing the icon
cap for the path truncated a legitimate path into one that opens nothing.
