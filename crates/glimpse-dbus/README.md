# glimpse-dbus

The `zbus` proxy traits for every bus service the daemon mirrors, and the two connections they run
on.

## Contents

- `dbus.rs` — `Buses`, holding the session and system connections
- `clients/` — one module per bus service, each a set of `#[zbus::proxy]` trait declarations

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

**Proxies only, no policy.** A module here declares interfaces and nothing else: no reconnect loop,
no auto-connect decision, no retry on top of what the backend already does. The service that owns
the topic decides what to do with a signal; this crate only makes the signal reachable.

**Both connections are opened once and shared.** `Buses` is `Clone` and holds the session and system
connections together, because a service that needs one usually ends up needing the other, and two
crates opening their own would double the bus traffic and the failure modes. `glimpsed` connects
once before any service starts and clones it into every one.

**A bus that will not connect is a degraded service, not a dead daemon.** `Buses::connect` never
fails; each accessor returns `Result<&Connection, &str>` where the `Err` is why there is no
connection. A service that needs a bus reports its own `degraded` carrying that reason, so
`system.services` names which feature was lost and why. A session with no D-Bus still has a panel,
a wallpaper and a lock screen.

**No `#[zbus::interface]` in this crate.** A proxy is glimpsed calling out and is shareable; an
interface is other applications calling in, and it needs a way back into the state of the service
that owns it. The object-server half of an owned service lives with that service in
`glimpse-services/src/services/`, the way `tray/watcher.rs` serves `org.kde.StatusNotifierWatcher`.

**Signatures come from introspection, not from memory.** A proxy that disagrees with the running
service fails at the call, not at compile time, which is the expensive kind of wrong. The
project-local `zbus` skill under `.claude/skills/zbus/` carries introspected signatures for these
services; check against it rather than hand-writing a method name.

**No topic types here.** A payload belongs in `glimpse-contracts`, where it can be generated for the
other SDKs. A backend type that leaked into a payload could not be.

## Status

Under construction. `clients/notifications.rs` is a `#[zbus::interface]` block that belongs with the
notifications service and is not declared here; a live fixture connection for tests needs zbus's
`p2p` feature, which nothing has asked for yet.
