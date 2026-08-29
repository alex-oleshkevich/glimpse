# glimpsed

The daemon. Broker, service host, and the only process that talks to backends.

Owns every topic, serves every client from one socket, and holds the two D-Bus names that have no
backing store anywhere else: `org.freedesktop.Notifications` and `org.kde.StatusNotifierWatcher`.

## Contents

- `main.rs` — startup, tracing setup, exit codes
- `cli.rs` — the flag surface and log-format resolution
- `errors.rs` — the `Exit` table and the one `anyhow::Error` to exit-code mapping
- `broker/` — the single task holding topic values and per-client coalescing *(pending)*
- `reload.rs` — the task that fans a reloaded document out to every service's `ConfigSink`
- `registry.rs` — service registration, DAG validation, supervision *(pending)*
- `wayland/` — the `WaylandEdge` implementation: gamma, idle, clipboard *(pending)*

The broker, the socket and the service host all run; `registry.rs` and `wayland/` do not exist yet,
so there is no demand-driven lifecycle, no dependency DAG and no Wayland edge.

## Rules

Nothing depends on this crate. It is a leaf.

`LogFormat::resolve` takes `JOURNAL_STREAM` as an argument rather than reading it, so the decision
is testable without mutating process state — which edition 2024 makes `unsafe`, and correctly so,
since it is a data race in a threaded program. Everything else reads the environment through clap's
`env =`, which keeps the precedence flag over variable over default in one place.

The daemon joins `glimpse_ipc::SOCKET_RELATIVE_PATH` onto `XDG_RUNTIME_DIR` itself rather than
calling `glimpse_ipc::socket_path`, which discovers a socket that is already there. For a daemon
that is the refusal-to-start case, not the answer.

`exit` holds only the codes something returns today; the rest arrive with the code that returns
them. 2 will never be there — clap owns it, and that is where `--only` together with `--without`
lands.

A bad `--log` filter warns and falls back to `info` rather than refusing to start. `RUST_LOG` is
inherited from a shell or a unit, and killing the session daemon over a stale value in someone's
profile trades a cosmetic problem for a dead session.

The broker routes and nothing else. No icon work, no image decoding, no synchronous writes to
clients — anything slow inline hits every client's latency.

The socket lives under `XDG_RUNTIME_DIR` at mode 0600, and there is no `/tmp` fallback: a
predictable world-writable path invites pre-creation and symlink hijack. A second instance connects
first and exits 3 rather than unlinking a socket a live daemon may still own — and the message names
the path, which is why `DaemonError::IpcServer` is `#[error(transparent)]`: a wrapper message of its
own would replace the only sentence saying which daemon is already there.

The socket is unlinked on a clean shutdown, while the listener is still held so nothing can have
taken the path in between. Not a correctness fix — `bind` connects first and clears a dead socket —
but leaving one behind means every start goes through that path instead of none.

`--only` and `--without` are applied in `register::<S>()`, before anything is declared, so an
excluded service is absent from `system.services` rather than listed as one that failed. A name
matching no service warns: a misspelt `--only` otherwise starts nothing at all and reads as a daemon
that broke.

A command that could not be delivered returns an error. Reporting success for a command that never
reached its service is worse than reporting failure. A full or closed service inbox is `Unavailable`
and retryable — a stopped service is one a supervisor may bring back, so a caller retrying is right.

Method routing mirrors topic routing exactly. `register::<S>()` is the last place the concrete
service type is known, so it builds the erased `Dispatch` there and hands it to the broker inside
the same `Declare` that carries `METHODS` — declaring a method with no way to route it cannot be
expressed. The broker looks the name up in the store, calls the dispatcher and returns; the service
answers the `Responder` itself, so nothing in the broker task ever awaits a service. `system.methods`
is published from `Declare` alone, because that is the only message that changes the registry.

Configuration reload is one task of its own rather than an arm of the shutdown loop. `SIGHUP` and
the filesystem are two triggers for the same work and neither replaces the other: a user editing
with a tool that defeats inotify still has a way to apply the change, and a session whose watches
the kernel refused still reloads on request. `shutdown_signal` therefore handles only `SIGTERM` and
`SIGINT`, which is what its name claims.

A reload re-reads the whole stack and re-applies **only the services whose own table changed**.
`register::<S>()` is the last place the concrete type is known, so it builds the `ConfigSink` there
alongside the `Dispatch`, projecting with `S::Config::from(document)` and closing over the previous
slice; editing `[[panels]]` therefore cannot perturb the night light schedule, because that subtree
is unchanged and its service never hears about the reload at all. A sink offers rather than queues: awaiting would park the one task that
reloads every service behind whichever of them is wedged.

A reload that does not parse is dropped and the running configuration survives. The daemon does not
exit over it, and it does not half-apply it.

`wl_` objects appear only under `wayland/`. Services reach Wayland through `trait WaylandEdge`,
which is what keeps every service test headless.

Never add `panic = "abort"`. Per-service panic isolation depends on unwinding.

## Units

Five user units ship in `data/systemd/`, installed to `{prefix}/lib/systemd/user`. Four are
`WantedBy=graphical-session.target`; `glimpse-lock.service` deliberately has no `[Install]` section,
because starting it locks the screen — it is started on demand, not pulled in at login.

All five units carry `ExecReload=/bin/kill -HUP $MAINPID`, so `systemctl --user reload <unit>`
re-reads the configuration stack through the same path the filesystem watch uses. That is the way
out for an editor whose write inotify never sees.

`ExecReload=` goes in with the handler and never before it. `SIGHUP`'s default disposition
terminates the process, so on a binary that registers no handler the directive turns
`systemctl --user reload` into a kill.

The UI units use `Wants=glimpsed.service`, never `Requires=` — a dead daemon must not take the
panel, wallpaper or night light with it. The locker names glimpsed nowhere at all: it has to
authenticate with the daemon dead, so the dependency would buy nothing, and every relationship it
carries is another way for something to stop it.

For the same reason the locker carries no `Requisite=`, which the other four do. `Requisite=` is
documented as "similar to `Requires=`", and `Requires=` stops the configuring unit when the listed
unit is stopped. On the locker that is a stop edge bought for nothing but failing fast outside a
session, so it is left off and `just check-units` rejects it along with `Requires=`, `BindsTo=` and
`Conflicts=`.

The units carry no comments. Everything they would have said is here, and `just check-units` enforces
the parts that matter rather than trusting anyone to read them.

Units inherit the user manager's environment, so `WAYLAND_DISPLAY` and the rest have to be in it
before `graphical-session.target` is reached — the compositor does that with `systemctl --user
import-environment` or `dbus-update-activation-environment`. None of these units set it themselves.

`glimpse-lock.service` has **no sandboxing whatsoever**, and `just check-units` enforces that with an
allowlist over its `[Service]` keys rather than a list of banned directives — a dozen options imply
`NoNewPrivileges=`, and any one of them places the locker in a user namespace that strips setuid from
`unix_chkpwd`. PAM then rejects the correct password and the symptom looks exactly like a typo.

`check-units` also verifies every `ExecStart` names a binary the build actually ships. It has to:
the recipe filters systemd's "is not executable" complaint, since a source tree never has the
binaries at their installed paths, and without the name check a typo would hide behind that filter.

`[package.metadata.deb]` and `[package.metadata.generate-rpm]` live here rather than on any of
the other five binary crates because cargo-deb/cargo-generate-rpm each need one crate to invoke
against, not because glimpsed is special — the assets lists pull in all six binaries plus config,
wallpapers, units, and the license from the shared target dir and repo root. `data/pam.d` and
`data/dbus-1/services` are still empty placeholders, so their contents aren't in the asset lists
yet; add them once something real lands there.

Themes are the one asset whose directory structure is load-bearing: `themes/<name>/panel.css` is
found by name, so a flat glob into a single destination would collapse every theme's sheets on top of
one another. `scripts/install.sh` and `scripts/package-binary.sh` therefore walk `data/themes/*/*.css` and
rebuild `<name>/` from the source path, and generalise to any number of themes; the two static asset
lists cannot compute a destination, so each shipped theme needs its own line in both. Only `adwaita`
ships today.
