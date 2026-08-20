---
state: draft
---

# 009 — systemd Integration

How the binaries are started, stopped, restarted and supervised inside a Wayland session.

## Problem

The suite is four processes with an ordering constraint, a daemon that must be told when the session
ends, and one component whose correctness is destroyed by the sandboxing options people habitually
add. Starting them from the compositor's config as bare `spawn` lines gives no restart policy, no
ordering, no readiness signal, and no way to see why something died.

The Wayland session also has an environment problem: the systemd user manager is already running
before the compositor starts, so it does not know `WAYLAND_DISPLAY`. A unit that ignores this starts
and immediately fails.

## Goals

- Every long-lived binary starts and stops with the graphical session, in the right order.
- A crash restarts automatically without masking a persistent failure as a crash loop.
- The daemon reports readiness, so dependants are ordered against something meaningful.
- The locker keeps working, which means it stays out of the sandbox options that break PAM.
- `journalctl --user -u <unit>` is enough to diagnose a failure.

## Non-goals

- No system-level units. Everything is a user unit.
- No session management. Reaching `graphical-session.target` is the compositor's or session
  manager's job, not glimpse's.
- No socket activation. The daemon has no consumers before the session exists.

## Tech

### Session model

A Wayland compositor is expected to bring up `graphical-session.target` in the systemd user manager.
niri does this through `niri.service` and the `niri-session` wrapper; Hyprland through
`hyprland-session.target`; [uwsm](https://github.com/Vladimir-csp/uwsm) wraps any compositor into
templated `wayland-session@.target` units bound to the stock hierarchy. glimpse targets whichever is
present and defines none of its own session targets.

Environment propagation is the session manager's responsibility. The compositor or its wrapper runs
`dbus-update-activation-environment --systemd` (or uwsm's equivalent) so that `WAYLAND_DISPLAY`,
`XDG_CURRENT_DESKTOP`, `NIRI_SOCKET` and the rest reach the user manager. glimpse units assume this
has happened and fail loudly rather than guessing.

### The standard stanza

Every unit that runs inside the session uses the same trio, which is the pattern niri and Hyprland
both document:

```ini
[Unit]
PartOf=graphical-session.target
After=graphical-session.target
Requisite=graphical-session.target
```

- `After=` orders startup.
- `PartOf=` propagates stop and restart from the target, so ending the session stops the unit.
- `Requisite=` refuses to start when the target is not already active, which turns "started too
  early" into an immediate, legible failure instead of a crash loop against a missing display.

### glimpsed.service

```ini
[Unit]
Description=glimpse session daemon
Documentation=https://github.com/alex/glimpse
PartOf=graphical-session.target
After=graphical-session.target dbus.socket
Requisite=graphical-session.target
StartLimitIntervalSec=60
StartLimitBurst=5

[Service]
Type=notify
NotifyAccess=main
ExecStart=/usr/bin/glimpsed
ExecReload=/bin/kill -HUP $MAINPID
Restart=on-failure
RestartSec=1s
TimeoutStopSec=10s
WatchdogSec=30s
RuntimeDirectory=glimpse
RuntimeDirectoryMode=0700
RuntimeDirectoryPreserve=restart
Slice=session-graphical.slice

[Install]
WantedBy=graphical-session.target
```

- `Type=notify` with `READY=1` after the socket is listening and every `OnBoot` service has reached
  `ready` or `degraded`. Dependants ordered `After=glimpsed.service` therefore start against a
  daemon that can answer, not merely one that has been forked.
- `WatchdogSec=30s` with pings from the broker loop. A wedged broker still holds the socket open and
  would otherwise look healthy while delivering nothing.
- `RuntimeDirectory=glimpse` creates `$XDG_RUNTIME_DIR/glimpse` at mode 0700, which is where the
  socket and the decoded tray pixmaps live. `RuntimeDirectoryPreserve=restart` keeps it across a
  restart so clients do not observe a missing directory.
- `StartLimitBurst=5` in 60 seconds turns a persistent failure into a stopped unit rather than an
  endless respawn against, for example, a missing session bus.

### glimpse.service and glimpse-wallpaper.service

```ini
[Unit]
Description=glimpse panel
PartOf=graphical-session.target
After=graphical-session.target glimpsed.service
Requisite=graphical-session.target
Wants=glimpsed.service

[Service]
ExecStart=/usr/bin/glimpse
Restart=on-failure
RestartSec=1s
Slice=session-graphical.slice

[Install]
WantedBy=graphical-session.target
```

`Wants=`, deliberately not `Requires=`. The panel is specified to survive a dead daemon by rendering
empty, and `Requires=` would kill it instead. The wallpaper unit is identical with its own
`ExecStart`.

### glimpse-lock.service

```ini
[Unit]
Description=glimpse screen locker
PartOf=graphical-session.target
After=graphical-session.target
Requisite=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/bin/glimpse-lock
Restart=no
OOMScoreAdjust=-500
```

No `[Install]` section. The locker is started on demand — by a keybind, by
`systemctl --user start glimpse-lock`, or by the daemon's `power` service — and never pulled in by
the session target.

`Restart=no` is deliberate. A locker that respawns on failure can mask a configuration error by
looping, and the compositor already fails closed: if the process dies after the `locked` event, the
session stays locked with a blank screen.

**No unit relationship may stop the locker while it holds the lock.** A stopped locker is not an
unlocked session — the compositor keeps the lock, the screen stays blank, and the one process that
could have authenticated is gone. `PartOf=graphical-session.target` above is the single exception,
and only because it fires when the compositor is leaving too, so there is no surface left to
authenticate against. Every other form that propagates a stop — `PartOf=` on any other unit,
`BindsTo=`, a `Conflicts=` from a target someone else starts — is a route into that state.
`Wants=` and `WantedBy=` are the only relationships that do not.

**Locking before sleep is the daemon's `power` service, and it is the only route.**

logind emits `PrepareForSleep` and does not wait. Something has to hold a `delay` inhibitor across
that signal, and that something has to be resident — which the locker is not, since it is not
running when the lid closes. The `power` service is `OnBoot + Never` and already follows logind, so
it holds the inhibitor, starts `glimpse-lock.service` through the systemd user manager's D-Bus API,
and drops the descriptor once logind reports `LockedHint`.

This costs invariant 5, and the cost is stated rather than worked around: with `glimpsed` down an
explicit lock still works and locking before suspend does not. There is no user-level `sleep.target`
to order against — systemd 261 still ships none — so the only alternative is a second resident
process whose whole job is holding one file descriptor.

**The inhibitor buys about five seconds.** `InhibitDelayMaxSec` defaults to 5 s, after which logind
suspends regardless of who is still delaying. The locker has to reach `locked` inside that window,
which is a real constraint on how much it may do before creating its surfaces — it is why
`006_lock.md` specifies that the background never gates the prompt.

### D-Bus activation

`glimpsed` owns `org.freedesktop.Notifications` and `org.kde.StatusNotifierWatcher`. Activation
files in `data/dbus-1/services/` let an application that starts before the session target pull the
daemon up rather than silently losing its tray icon:

```ini
[D-BUS Service]
Name=org.kde.StatusNotifierWatcher
Exec=/usr/bin/glimpsed
SystemdService=glimpsed.service
```

`SystemdService=` matters: without it the bus forks its own copy outside the unit, and there are then
two daemons contending for the socket. With it, activation starts the unit.

Only one package on a system may own each of these names. Installing glimpse alongside dunst, mako,
or a Plasma session is a packaging conflict, and the activation files are where it surfaces.

### Sandboxing

Hardening directives are safe on `glimpsed`, the panel and the wallpaper, and several are worth
having:

| Directive                     | Applies to                | Note                                                   |
| ----------------------------- | ------------------------- | ------------------------------------------------------ |
| `PrivateTmp=yes`              | daemon, panel, wallpaper  | safe                                                    |
| `ProtectSystem=strict`        | daemon, panel, wallpaper  | needs `ReadWritePaths=` for `$XDG_RUNTIME_DIR/glimpse`  |
| `ProtectHome=read-only`       | daemon                    | breaks the wallpaper, which reads user images           |
| `ProtectKernelTunables=yes`   | daemon                    | safe                                                    |
| `RestrictSUIDSGID=yes`        | daemon, panel, wallpaper  | **never on the locker**                                  |
| `NoNewPrivileges=yes`         | daemon, panel, wallpaper  | **never on the locker**                                  |
| `PrivateUsers=yes`            | none                      | breaks PAM and offers little here                        |

**The locker gets no sandboxing.** `NoNewPrivileges=`, `PrivateUsers=`, `RestrictSUIDSGID=` and the
options that imply them place the process in a user namespace where the setuid bit on
`/usr/bin/unix_chkpwd` is stripped. PAM then returns `AUTHINFO_UNAVAIL` and the correct password is
rejected, leaving a session that cannot be unlocked. This failure looks like a wrong password, not
like a sandbox problem, which is what makes it expensive to diagnose.

`ProtectHome=` is similarly wrong for the wallpaper, which exists to read images out of the user's
home.

### Installed files

| Path                                                        | From                          |
| ----------------------------------------------------------- | ----------------------------- |
| `/usr/lib/systemd/user/glimpsed.service`                    | `data/systemd/`               |
| `/usr/lib/systemd/user/glimpse.service`                     | `data/systemd/`               |
| `/usr/lib/systemd/user/glimpse-wallpaper.service`           | `data/systemd/`               |
| `/usr/lib/systemd/user/glimpse-lock.service`                | `data/systemd/`               |
| `/usr/share/dbus-1/services/org.freedesktop.Notifications.service` | `data/dbus-1/services/` |
| `/usr/share/dbus-1/services/org.kde.StatusNotifierWatcher.service` | `data/dbus-1/services/` |
| `/etc/pam.d/glimpse-lock`                                   | `data/pam.d/`                 |

Users override with drop-ins in `~/.config/systemd/user/<unit>.d/`, never by editing the shipped
unit.

### Enabling

```bash
systemctl --user enable --now glimpsed.service glimpse.service glimpse-wallpaper.service
```

Under niri, an alternative is linking into `~/.config/systemd/user/niri.service.wants/`, which ties
the units to that compositor rather than to any graphical session.

### Verification

```bash
just check-units                                              # unit syntax and dependency sanity
systemctl --user status glimpsed.service
systemctl --user show glimpsed.service -p NRestarts -p ExecMainStatus
journalctl --user -u glimpsed.service -f
systemd-analyze --user critical-chain glimpse.service         # ordering, once the session is up
```

## Risks

- **Operational** — an unlockable session caused by sandboxing the locker is the highest-severity
  failure in the project, and the symptom points at the wrong cause.
- **Operational** — bus name conflicts with an already-installed notification daemon produce a
  daemon that starts, fails to take its name, and is degraded in a way that is invisible until a
  notification is sent. The daemon reports this on `system.services` rather than only in the log.
- **Technical** — `Requisite=` makes ordering failures loud. That is the intent, but it means a
  compositor that does not reach `graphical-session.target` produces units that refuse to start with
  a message that reads like glimpse's fault.

## Alternatives considered

- **Spawning from the compositor config** — rejected: no restart policy, no ordering against daemon
  readiness, no journal integration, and no way to stop the suite without killing the compositor.
- **Socket activation for `glimpsed`** — rejected: nothing connects before the session exists, so
  activation would only add a unit and a failure mode.
- **`Requires=glimpsed.service` on the panel** — rejected: it contradicts the panel's specified
  behaviour of surviving a dead daemon.
- **`Restart=always` on the locker** — rejected: it converts a configuration error into a respawn
  loop, and the compositor's fail-closed behaviour already covers the case that matters.
- **Defining a `glimpse-session.target`** — rejected: session composition belongs to the compositor
  or to uwsm. Another target would fragment an already fragmented area.

## Changelog

- 2026-08-20 — created.
- 2026-08-20 — gave the locker an `[Install] WantedBy=lock.target` section, which the previous text both required and denied; recorded that `PartOf=lock.target` must never be added, because `systemd-lock-handler` stops that target on logind's `Unlock` and `PartOf=` would turn an unauthenticated D-Bus call into a SIGTERM to the locker.
- 2026-08-20 — dropped `systemd-lock-handler` as a supported integration: no `[Install]` section, and locking before sleep is the daemon's `power` service holding a logind delay inhibitor. Recorded the invariant-5 cost, the absence of any user-level `sleep.target`, the five-second `InhibitDelayMaxSec` ceiling, and the rule that no unit relationship may stop the locker while it holds the lock.
