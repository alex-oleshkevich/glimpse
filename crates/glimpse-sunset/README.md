# glimpse-sunset

The night light: it owns `[night-light]`, follows the solar phase, and applies a color temperature
to every output through `zwlr_gamma_control_unstable_v1`. It exports what it is doing on
`me.aresa.Glimpse.NightLight1`, with two setters: `SetSchedule` forces the mode and `SetTemperature`
overrides the temperature it runs at.

## Contents

- `main.rs` — `run(cli) -> anyhow::Result<()>`, with `main` turning the outcome into an `ExitCode`
- `errors.rs` — the exit codes and the single `downcast_ref` that maps an error onto one
- `cli.rs` — the argument surface, flattening the shared structs from `glimpse-utils`
- `services.rs` — the composition root, where the start order is the whole of the design
- `gamma.rs` — the Wayland half, the only file here that binds a `wl_` object outside a UI crate
- `provider.rs` — the `me.aresa.Glimpse.NightLight1` object and the task that signals its changes

## Where the decisions live

The state machine is **not here**: `glimpse-services/src/services/night_light.rs` holds the whole
schedule and both ramps, testable without a compositor against `FakeGamma`. This crate supplies only
the implementation of `trait Gamma` that talks to a real compositor, and a composition root. The
schedule itself comes from `solar`, which publishes `phase` and `next_change` for the night light to
ramp toward; `[night-light] schedule = "schedule"` is the manual alternative, computed from
`start-time`/`end-time` in the machine's own zone.

## Rules

- **Neither setter reaches the document.** A temperature, a time or a transition length is a
  preference in `[night-light]`; the mode in force lives in memory instead, reported as
  `overridden`, and clears only on a `[night-light]` edit or a restart, never on an unrelated
  reload, or an edit to `[weather]` would cancel a mode nobody touched.
- **A manual temperature survives every tick, ending only at a boundary, a new mode, or a restart.**
  `SetTemperature` suppresses the ramp until the phase changes, `SetSchedule` arrives, or the process
  restarts. It ends in a step, not a ramp: the screen jumps to whichever steady temperature the
  schedule already holds. `SetTemperature(0)` clears it, since 0 is outside the 1000–6500 range.
- **`configured` reports the document's own schedule, never the effective one** — published
  `schedule` is `effective()` (`forced.unwrap_or(config.schedule)`), so once `SetSchedule` forces
  `off` there is nothing left in it for a UI to switch back to; `manual` tracks `SetTemperature` only.
- **`Snapshot` is append-only.** Its `OwnedValue` decode pops one field per declared field with
  `Vec::remove(0)` — an appended field is invisible to a shorter-signature client and free to add,
  but removing one **panics** the client, which is why the interface carries no version suffix.
- **The method answers after the display has been driven, not when the command was queued.**
  `SetSchedule` carries a `oneshot` sent *after* `evaluate` rather than firing and forgetting on the
  `try_send`. A backend that refused the ramp is a health condition, reported through
  `serving`/`reason` — the mode is in force either way.
- **Exit 4 means the compositor will never offer gamma control, and nothing else** — the unit carries
  `RestartPreventExitStatus=4`, so only a missing `zwlr_gamma_control_manager_v1` global maps to it; a
  socket not there yet or a failed registry roundtrip stay retryable, or a restart would strand it.
- **The ramp completes at the boundary** — the screen reaches the night temperature *at* sunset
  rather than starting to warm there, letting the service read one instant, `next_change`, instead of
  remembering the last one. A night shorter than the transition never reaches full temperature: the
  two ramps can overlap, and the honest result is a partial ramp rather than a jump.
- **The D-Bus name is taken before gamma control is**, so a second copy of this binary fails without
  ever touching the running one's outputs. The object is exported *before* the name is requested, or
  a `Get` in between finds a name with no object behind it, and the name is taken through
  `glimpse_dbus::own_name`'s `DoNotQueue`, since plain `request_name` lets a duplicate steal it.
- **Gamma control is exclusive — one client at a time.** A user already running `wlsunset`,
  `gammastep` or `hyprsunset` makes every output answer `failed`, reported as `degraded: another
  gamma client holds the outputs`; it does not retry in a spin, the next scheduled tick tries again.
- **A `wl_output` is bound only after a roundtrip has drained the queue** — nothing reads the socket
  while the light is handed back all day under `automatic`, so binding straight from the `Global`
  event binds outputs long gone and niri kills the connection with a protocol error.
- **The ramp table is a `memfd`, never a file** — `set_gamma` wants a descriptor, and `memfd_create`
  has no name in any directory and no dependence on `TMPDIR` being writable.
- **A clean stop hands the outputs back; a `SIGKILL` cannot.** `Service::stop` calls `Gamma::reset`,
  returning the display to the compositor's own ramp; a killed process leaves the last ramp applied
  until something takes gamma control — `systemctl --user restart glimpse-sunset` is the cure. `Off`
  releases rather than applying 6500K, since the compositor's own ramp is not necessarily neutral.
- **`serving` is what says whether to believe `temperature`.** A failed apply keeps the snapshot's
  last value, so a consumer reading `temperature` without `serving` shows a color nothing is applying.

## Starting it

`Type=dbus` on `me.aresa.Glimpse.NightLight`, started by `glimpse-session.target`. The process exits
4 if the compositor offers no `zwlr_gamma_control_manager_v1` (`RestartPreventExitStatus=4` stops the
unit retrying a session that will never support it); a compositor merely not up yet exits 1 and retries.

**There is deliberately no D-Bus activation file** — an activation outside a graphical session would
exit 1 every time, and enough of those inside `StartLimitIntervalSec` fail the unit until `systemctl
--user reset-failed`. Add it in the change that adds the first consumer.
