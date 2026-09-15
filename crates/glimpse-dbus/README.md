# glimpse-dbus

The `zbus` proxy traits and typed provider clients used across Glimpse processes, plus the two
connections they run on.

## Contents

- `dbus.rs` — `Buses`, holding the session and system connections, and `own_name`
- `provider.rs` — `Exported`, the name-and-object lifecycle the three providers share
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
| `notifications`           | session | the Glimpse notification provider        |
| `weather`                 | session | the Glimpse weather provider             |

## Rules

**Every bus name is taken through `own_name`.** `RequestNameFlags` defaults to
`AllowReplacement | ReplaceExisting | DoNotQueue`, so a plain `request_name` both steals the name
from whoever holds it and offers it to the next process that asks — in the notifications service
that means replacing a running dunst or mako instead of reporting the conflict. `own_name` requests
`DoNotQueue` alone: the first owner keeps the name and a duplicate gets `Error::NameTaken`.
`clippy.toml` bans `Connection::request_name`, `request_name_with_flags` and
`connection::Builder::name`, which carries the same default; `own_name` carries an `#[expect]`, so
the build fails if the ban ever stops resolving. GTK application-id names go through GApplication
and never reach here — that is why a second process with the same app id hands off and exits 0
before any name is requested.

**`Exported` puts the object up before it asks for the name.** A `Get` arriving between the two
would otherwise find the name with nothing behind it, which is also why a refused name must take
the object back down again. It owns the whole lifecycle: export, `own_name`, re-emit `snapshot`
whenever the service's state or health moves, then release and remove on shutdown.

**A provider supplies two `watch::Receiver`s and a three-line `Snapshot` impl, not a follow loop.**
`Exported::serve` runs the loop and never interprets what it is watching, so this crate still does
not depend on `glimpse-services`. The receivers are what a `ServiceHandle` already hands out.

**Backend proxies carry no policy.** System-service modules only declare interfaces. A typed client
for a Glimpse-owned provider may additionally own its availability state and `NameOwnerChanged`
follower so every consuming process gets the same lifecycle behavior.

**Both connections are opened once and shared.** `Buses` is `Clone` and holds session and system
together, because a service that needs one usually needs the other, and two connections in one
process double the bus traffic and the failure modes.

**A bus that will not connect is a degraded service, not a dead daemon.** `Buses::connect` never
fails; each accessor returns `Result<&Connection, &str>` whose `Err` is why there is no connection.
A session with no D-Bus still has a panel, a wallpaper and a lock screen.

**`Position` on `org.mpris.MediaPlayer2.Player` carries `emits_changed_signal = "false"`, and must
keep doing so.** MPRIS specifies it as emitting no `PropertiesChanged`, but the macro defaults to
`"true"` — so under `Lazily` caching the first read is cached behind a listener that never fires and
every later read returns that same number for the proxy's life. `"false"` is the only value that
both registers the property as uncached and suppresses the `cached_*` getter; `"const"` keeps the
cache and reproduces the stale read by another route.

**The attribute nests inside `property`.** `#[zbus(property(emits_changed_signal = "false"))]`, not
`#[zbus(property, emits_changed_signal = "false")]` — the flat spelling fails with
`unknown attribute`, which names the argument rather than the shape.

**Neither MPRIS proxy sets `default_service`**, because the destination is a different bus name per
player. Both are built through `builder().destination(name)`, and the name must be an owned
`String`: a `&str` ties the proxy's lifetime to that borrow and will not satisfy the `'static`
bound a subscription source requires.

**No `#[zbus::interface]` in this crate.** A proxy is a Glimpse process calling out and is
shareable; an interface is other applications calling in, and it needs a way back into the state of
the process that owns it. Object-server halves live in their owning binary crates. `Exported` is
generic over the interface and declares none, so the value is built by the binary that owns the
state behind it.

**Signatures come from introspection, not from memory.** A proxy that disagrees with the running
service fails at the call rather than at compile time. The project-local `zbus` skill carries
introspected signatures; check against it rather than hand-writing a method name.

**A domain type lives here only when a provider decodes it off the bus.** `glimpse-services` depends
on this crate and never the reverse, so weather and notification models sit beside their decoders
here while everything else sits beside the service that owns it. A backend type never reaches one:
what a client hands back is the published model, not the shape some daemon happened to store.

**A decoder belongs beside the wire type it undoes.** `clients/mod.rs` holds what both need —
`optional_clean` and `epoch` — because two copies of a timestamp decoder is two ways to disagree.

**A provider state carries decoded values, never the wire tuple.** `follow_provider` decodes once
and every consumer shares the result; a consumer that destructures the wire shape itself will drop
fields and skip caps. A snapshot that cannot be decoded is `unavailable` with the reason, which is
the shape a dead provider already produces.

**The two followers lose their provider differently, on purpose.** Weather keeps the last reading
and marks it `stale`, derived from the retained data rather than from the reason, so a provider that
comes back empty cannot strand the flag on nothing. Notifications throws its `view` away: every
dismiss and action on a retained list would call a provider that is gone, and the store is
authoritative, so a stale list can show what someone already dismissed.

**A reader caps text the writer already capped.** The owner of a well-known name is whoever claimed
it, so a decoder that skipped the cap would be trusting a bus name rather than a process. Cap tables
are per-direction and need not match: an image *path* stops at 4096 where a themed icon *name* stops
at 200, and reusing the icon cap truncated a legitimate path into one that opens nothing.
