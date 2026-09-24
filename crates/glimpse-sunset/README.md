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
- `gamma.rs` — the Wayland half, and the only file in the tree that binds a `wl_` object outside a
  UI crate
- `provider.rs` — the `me.aresa.Glimpse.NightLight1` object and the task that signals its changes

## Where the decisions live

The state machine is **not here**. `glimpse-services/src/services/night_light.rs` holds it, because
that is what makes it testable without a compositor: the whole schedule, both ramps and every
degraded state are exercised against `FakeGamma` in a headless test. This crate supplies the one
implementation of `trait Gamma` that talks to a real compositor, and a composition root.

The schedule is not this binary's to compute either. `solar` publishes `phase` and `next_change`,
and the night light ramps toward that instant. `[night-light] schedule = "schedule"` is the manual
alternative, computed from `start-time` and `end-time` in the machine's own zone.

## Rules

**Neither setter reaches the document.** A temperature, a time or a
transition length is a preference: it belongs in `[night-light]`, which is already re-read on
change, and writing it back from here would mean editing a file the user owns. The mode in force
right now is the one thing that cannot live there, because it has to outlive neither the reload nor
the process — so it is held in memory, reported as `overridden`, and dropped the moment
`[night-light]` itself is edited or the process restarts. A reload that leaves that table alone
keeps it: every service is reconfigured on every reload, so clearing on any `Input::Config` would
let an edit to `[weather]` cancel a mode nobody touched.

**A manual temperature survives every tick, and ends only at a boundary, a new mode, or a restart.**
`SetTemperature` suppresses the ramp until the phase changes from the one it was set in, until
`SetSchedule` arrives, or until the process restarts — a document reload counts as the restart it is
closest to, so editing `[night-light]` clears it too. It is never cleared by a tick alone: the
temperature it names is what gets applied on every one, exactly like the ramp it replaces. The
boundary it ends at is a step rather than a ramp — the screen jumps straight from the manual kelvin to
whichever steady temperature the schedule already holds. Nothing about it reaches the document, so
there is nothing here for a restart to lose. `SetTemperature(0)` clears it; 0 is outside the accepted
1000–6500 range and cannot name a temperature.

**`configured` reports the document's own schedule, never the effective one.** The published
`schedule` is `effective()` — `forced.unwrap_or(config.schedule)` — so the moment `SetSchedule`
forces the mode to `off` there is nothing left in `schedule` for a UI to switch back to. `configured`
is `[night-light] schedule` unconditionally, independent of any override. `manual` is `true` exactly
while a `SetTemperature` override is in force; it does not change what `overridden` means, which is
still only about `SetSchedule`.

**`Snapshot` is append-only.** `NightLightSnapshot`'s `OwnedValue` decode pops one field per declared
field with `Vec::remove(0)` and drops whatever is left on the wire, so an appended field is invisible
to a client built against the shorter signature and costs nothing to add. Reordering or inserting a
field is not the same operation: every downstream `downcast()` after the change point receives the
wrong type and fails, silently for an untyped reader like `gdbus` or a shell script indexing the
tuple. The same `remove(0)` panics rather than errors if the wire ever carries fewer fields than
declared, so a field may be added but never removed. This is also why the interface carries no
version suffix: an append needs no break to signal.

**The method answers after the display has been driven, not when the command was queued.**
`ServiceEndpoint::command` is a `try_send`, so a fire-and-forget setter would return while the old
mode was still published and a caller reading the snapshot next would see the mode it had just
replaced. `SetSchedule` carries a `oneshot` that the handler sends *after* `evaluate`. A backend
that refused the ramp is a health condition rather than a rejected mode — the mode is in force
either way, and `serving` and `reason` are where that failure is reported.

**Exit 4 means the compositor will never offer gamma control, and nothing else.** The unit carries
`RestartPreventExitStatus=4`, so that code has to be the permanent case alone. `WaylandGamma::connect`
fails three ways and only one of them is permanent: a missing `zwlr_gamma_control_manager_v1` global
is `Unavailable::Unsupported`, while a Wayland socket that is not there yet and a failed registry
roundtrip are `Unavailable::Unreachable` and stay retryable. Mapping the exit code by matching the
`"cannot take gamma control"` context string conflated all three, which would have left the service
permanently stopped after an ordinary compositor restart.

**The ramp completes at the boundary.** Approaching sunset the screen reaches the night temperature
*at* sunset, rather than starting to warm there. That is what lets the service read one instant —
`next_change` — instead of remembering the last one, and it removes the midnight rollover entirely.
The visible cost is that a `transition-minutes` of 15 begins warming a quarter of an hour before the
sun is down.

**A night shorter than the transition never reaches full temperature.** Above about fifty degrees in
June the two ramps overlap; the formula still answers at every instant, and the honest result is a
partial ramp rather than a jump. There is a test for it.

**The D-Bus name is taken before gamma control is.** Building a `ServiceRuntime` allocates channels
and nothing else, so every handle exists before anything has a side effect; the object is exported
and the name requested next; only then is gamma taken and the services started. A second copy of
this binary therefore fails without ever touching the outputs the running one holds. Two details are
load-bearing: the object is exported *before* the name is requested, or a `Get` arriving between the
two finds a name with no object behind it — zbus warns about exactly this; and the name is taken
through `glimpse_dbus::own_name`, which requests `DoNotQueue` alone, because plain `request_name`
lets a duplicate silently steal the name and leave the first process applying gamma and unreachable.
Both were measured, not feared.

**Gamma control is exclusive — one client at a time.** A user already running `wlsunset`,
`gammastep` or `hyprsunset` makes every output answer `failed`, and the service reports
`degraded: another gamma client holds the outputs` and stops there. It does **not** retry in a spin;
the next scheduled tick tries again, which is a probe once a minute rather than a flicker. An output
plugged in later is picked up the same way, with no timer behind hotplug either.

**A `wl_output` is bound only after a roundtrip has drained the queue.** Nothing reads the socket
while the light is handed back — all day under `automatic` — so hotplug events wait there for hours.
Binding straight from the `Global` event binds outputs that are long gone, and niri answers with a
protocol error that kills the connection. Bound from `arm` instead, an output that came and went
never gets bound. A connection that is dead anyway is replaced on the next apply or reset.

**The ramp table is a `memfd`, never a file.** `set_gamma` wants a descriptor, so a path in `/tmp`
would only be something to unlink again; `memfd_create` has no name in any directory, no window
where one exists, and no dependence on `TMPDIR` being writable.

**A clean stop hands the outputs back; a `SIGKILL` cannot.** `Service::stop` calls `Gamma::reset`,
which destroys the controls and returns the display to the compositor's own ramp. A killed process
leaves the last ramp applied until something else takes gamma control — that is a property of the
protocol, not a bug to guard against, and `systemctl --user restart glimpse-sunset` is the cure.

**`Off` releases rather than applying 6500K.** The compositor's own ramp is not necessarily neutral,
so pinning it at daylight is still holding it.

**`serving` is what says whether to believe `temperature`.** When an apply fails the snapshot keeps
the last value it managed to set, exactly as the weather provider keeps its last reading — so a
consumer that reads `temperature` without reading `serving` will show a color nothing is applying.
The alternative, blanking the fields, throws away the only information there is about what the
screen probably looks like.

## Starting it

`Type=dbus` on `me.aresa.Glimpse.NightLight`, started by `glimpse-session.target`. It needs no
daemon: the process exits with code 4 if the compositor offers no `zwlr_gamma_control_manager_v1`,
because without it there is nothing for this binary to do, and `RestartPreventExitStatus=4` stops
the unit retrying a session that will never support it. A compositor that is merely not up yet
exits 1 and is retried normally.

**There is deliberately no D-Bus activation file.** Nothing activates this name on demand yet, and
an activation outside a graphical session would exit 1 every time — five of those inside
`StartLimitIntervalSec` put the unit in a failed state that blocks the next real start until
`systemctl --user reset-failed`. Add the activation file in the change that adds the first consumer,
not before.
