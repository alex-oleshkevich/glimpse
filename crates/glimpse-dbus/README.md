# glimpse-dbus

The `zbus` proxy traits for every bus service the daemon mirrors, and the two connections they run
on.

## Contents

- `dbus.rs` — `Dbus`, holding one session and one system connection
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

**Both connections are opened once and shared.** `Dbus` is `Clone` and holds the session and system
connections together, because a service that needs one usually ends up needing the other, and two
crates opening their own would double the bus traffic and the failure modes.

**Signatures come from introspection, not from memory.** A proxy that disagrees with the running
service fails at the call, not at compile time, which is the expensive kind of wrong. The
project-local `zbus` skill under `.claude/skills/zbus/` carries introspected signatures for these
services; check against it rather than hand-writing a method name.

**No topic types here.** A payload belongs in `glimpse-contracts`, where it can be generated for the
other SDKs. A backend type that leaked into a payload could not be.

## Status

Under construction. `clients/notifications.rs` is not yet declared in `clients/mod.rs`, and
`dbus.rs` still needs `anyhow` as a dependency and the `blocking-api` feature on `zbus` for its
synchronous `connect`.

Spec: [`specs/001_architecture.md`](../../specs/001_architecture.md)
