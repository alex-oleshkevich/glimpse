# glimpse-dbus

The `zbus` proxy traits and typed provider clients used across Glimpse processes, plus the two
connections they run on.

## Contents

- `dbus.rs` — `Buses`, holding the session and system connections, and `own_name`
- `provider.rs` — `Exported`, the name-and-object lifecycle the three providers share
- `clients/` — one module per bus service, each a set of `#[zbus::proxy]` trait declarations; a
  Glimpse-owned provider's module also carries a typed handle and owned follower lifecycle
- `testing/` — `PrivateBus` and the tray fakes, compiled only under the `testing` feature

| Module                    | Bus     | What it fronts                          |
| ------------------------- | ------- | --------------------------------------- |
| `bluez`                   | system  | BlueZ adapters and devices              |
| `geoclue`                 | system  | GeoClue2 manager and client             |
| `login1`                  | system  | logind session, seat and idle hints      |
| `network_manager`         | system  | NetworkManager devices and connections   |
| `power_profiles`          | system  | power-profiles-daemon                    |
| `udisks2`                 | system  | UDisks2 removable media                  |
| `upower`                  | system  | UPower devices, decoded from one `GetAll`; `EnableChargeThreshold` |
| `kdeconnect`              | session | `kdeconnectd` devices and their plugin objects; proxies are hand-written, because the daemon's own introspection is invalid |
| `mpris`                   | session | MPRIS players                            |
| `status_notifier_item`    | session | StatusNotifierItem tray entries          |
| `status_notifier_watcher` | session | the tray registry, and `Registry` behind it |
| `dbusmenu`                | session | a tray item's `com.canonical.dbusmenu`   |
| `idle`                    | session | the Glimpse idle-inhibitor provider      |
| `freedesktop_notifications` | session | `Notify` on any notification daemon   |
| `notifications`           | session | the Glimpse notification provider        |
| `weather`                 | session | the Glimpse weather provider             |
| `night_light`             | session | the Glimpse night light provider         |

## Rules

**Every bus name is taken through `own_name`, never a raw `request_name`.** The raw call defaults to
`AllowReplacement | ReplaceExisting | DoNotQueue`, stealing the name from whoever holds it; `own_name`
requests `DoNotQueue` alone, so a duplicate owner gets `Error::NameTaken` instead.

**`ObjectManager` sits at a different path per service.** BlueZ is at `/`, NetworkManager at
`/org/freedesktop`, UDisks2 at `/org/freedesktop/UDisks2` — a `path_namespace` copied from one
service's watcher matches nothing on another.

**Two NetworkManager values do not decode the way their names suggest.** `Device.StateReason` is
`(uu)` and repeats `State` in element 0 — take element 1. `Connection.Active` has no `StateReason`
property at all; the reason exists only on the `StateChanged` signal, so a client caches
`path -> reason` and evicts it on state 4.

**`autoconnect` is absent from a profile whenever it is true** — decode it as defaulting to `true`,
or reading `None` as false inverts every profile that has never had it switched off.

**`Ssid` is `ay` and promises nothing**, not valid UTF-8 and not non-empty. The decoded display
string is lossy and not a name to join by; `AccessPointProperties` carries `raw_ssid` beside it, and
activation passes those bytes.

**`Exported` puts the object up before it asks for the name**, so a `Get` arriving between the two
finds something behind it; a refused name takes the object back down. It owns export, `own_name`,
re-emitting `snapshot` on every state or health change, and release on shutdown.

**A provider supplies two `watch::Receiver`s and a `Snapshot` impl, not a follow loop** —
`Exported::serve` runs the loop and never interprets what it watches, so this crate still does not
depend on `glimpse-services`.

**Backend proxies carry no policy** — a system-service module decodes what it answers, and what to
do about it belongs to the service. A Glimpse-owned provider's client may additionally own its
availability state and `NameOwnerChanged` follower, so every consumer shares one lifecycle.

**Both connections are opened once and shared**, since a service that needs one bus usually needs
the other. **A bus that will not connect is a degraded service, not a dead daemon** —
`Buses::connect` never fails; each accessor returns `Result<&Connection, &str>` naming why.

**`Position` on `org.mpris.MediaPlayer2.Player` must keep `emits_changed_signal = "false"`.** MPRIS
emits no `PropertiesChanged` for it, but the macro defaults to `"true"`, which under `Lazily`
caching reads it once behind a listener that never fires. The attribute nests inside `property`:
`#[zbus(property(emits_changed_signal = "false"))]`, not the flat form, which fails with
`unknown attribute`.

**Neither MPRIS proxy sets `default_service`**, since the destination is a different bus name per
player; both build through `builder().destination(name)` with an owned `String` — a borrowed `&str`
will not satisfy the `'static` bound a subscription source requires.

**No `#[zbus::interface]` in this crate, except under the `testing` feature.** A proxy calls out and
is shareable; an interface is called into and needs a way back into the owning process's state, so
object-server halves live in the owning binary. `testing/tray.rs` is the carve-out: a fake has no
process state to reach back into.

**A tray item, a UDisks2 object and a BlueZ device all decode from one `GetAll`-shaped map with
defaults, never through typed getters** — reading a property individually costs a round trip and a
failure for every member the peer never implemented. Every BlueZ decoder also returns an
all-`Option` partial, since `PropertiesChanged` carries only a subset of the shape the other two
share; the service merges the partial onto what it already holds.

**The tray watcher's rules live in `Registry`, a plain struct, not the interface** — which key an
item lands on and that a repeat registration is idempotent are decided there and tested without a
bus; the `#[zbus::interface]` is a shell forwarding to it.

**Signatures come from introspection, not memory** — a proxy that disagrees with the running service
fails at the call rather than at compile time; the project-local `zbus` skill carries introspected
signatures to check against.

**A domain type lives here only when a provider decodes it off the bus, beside its decoder.**
`glimpse-services` depends on this crate and never the reverse, so a backend type never reaches a
consumer. `clients/mod.rs` holds what more than one decoder needs — `optional_clean`, `epoch` — so a
timestamp is never decoded twice.

**A provider state carries decoded values, never the wire tuple.** `follow_provider` decodes once
and every consumer shares the result; an undecodable snapshot is `unavailable` with the reason, the
shape a dead provider already produces.

**A follower's loss policy is chosen per provider.** Weather keeps its last reading marked `stale`,
since an empty comeback must not strand the flag on nothing. Notifications and night light discard
their state instead, since a stale list could dismiss against a gone provider and a stale
temperature would claim what the screen no longer shows. Idle keeps `health` while discarding
`inhibitors`, since a disconnect says nothing about which backend degraded.

**`NightLightProviderHandle::set_temperature(0)` clears the override, not 0 kelvin** — that sentinel
belongs to `glimpse-sunset` and is documented only in its README.

**A reader caps text the writer already capped** — trusting the cap already applied would mean
trusting a bus name rather than a process. Cap tables are per-direction and need not match: an image
path and a themed icon name have different limits.
