# glimpse-lock

The screen locker. `ext-session-lock-v1` surfaces and PAM authentication.

The one component where a bug is a security failure rather than a cosmetic one.

## Commands

- `glimpse-lock` — the resident daemon under `glimpse-lock.service`. It waits; it does not lock at
  start unless logind's `LockedHint` is already true.
- `glimpse-lock lock` — calls logind `LockSession` on this session and exits 0 once `LockedHint`
  turns true, or 4 after 5 s without it, so `glimpse-lock lock && systemctl suspend` does not race
  the lock. It sends a D-Bus call and nothing else, which is why it is safe from a sandboxed parent.
- `glimpse-lock check` — one `name: ok|fail (reason)` line per check, exit 5 if any fails. It also
  prints the `user` line: the name PAM would be asked about, or why there is none.
- `glimpse-lock --standalone` — development only; see below.

Exit codes: 0 ok, 1 failure, 3 configuration, 4 not locked in time, 5 a check failed.

## What it does

- Locks on logind `Session.Lock` when `[power] lock-on-request` is on, before sleep when
  `[power] lock-before-sleep` is on, and at start when `LockedHint` is already true — a previous
  locker died holding the lock
- Creates one lock surface per monitor through `Instance::connect_monitor`, including a monitor
  hotplugged while locked, before that monitor can show anything
- Treats the session as locked only after the compositor sends `locked`, and only then sets
  `LockedHint`
- Authenticates through PAM service `[lock] pam-service` off the UI thread
- Puts a `LockStage` on every lock surface — background, clock, user and password prompt — and every
  stage mirrors the same prompt state: busy, message, Caps Lock, availability and the name. Exactly
  one stage's prompt is interactive and only it submits. The lock window holding keyboard focus
  takes it the moment it becomes active, so keystrokes never land on a mirror. Until one is active,
  `[lock] prompt-output` decides: a connector pins it, `"focused"` follows the compositor's focus
  cached ahead of the lock, and either falls back to the first monitor. Placement is re-decided on
  every surface added, window activation, focus change, output change and `prompt-output` reload:
  the wanted output takes the prompt as soon as it has a lock surface, a wanted output without one
  leaves it where it is, and a lock never has zero interactive prompts while any surface exists.
  When the interactive monitor leaves, interactivity moves to the wanted output among those left,
  else the first. The newly interactive entry takes focus, and again only when the prompt state
  changes, never while its session sheet is open; the old one is cleared
- Shows AccountsService `RealName`, else `UserName`, fetched at start and again on every lock and
  never awaited; until it answers, the stage shows the PAM username
- Ticks the clock on the minute boundary with one timer per lock, re-armed after each tick. The
  timer counts monotonic time, which stops during suspend, so a resume ticks at once and re-arms
  it from the wall clock; unlocking cancels it

## Backgrounds

`[lock.background]` `image` and `image-dark` are decoded off the UI thread, per output at its size,
scale and fit, at start and whenever the configuration, the output set or an output's size changes —
never at the lock. `blur-radius` is a GPU gaussian applied as a texture lands, through one offscreen
renderer realized for the display, since no lock window exists yet to lend its own. Naming neither inherits `[wallpaper]`'s top-level pair (not its
per-output images); naming either takes the whole pair. The first frame is the cached texture, or
`color` until one lands and whenever one fails. A stale decode is dropped, a failed one is retried
only on an output or configuration change, and the old texture stays until its replacement lands.

Dark is `adw::StyleManager::is_dark` under `[appearance] color-scheme`: `image-dark` if set, else
`image`, with `dim-dark`; light is `image` with `dim`. Both decode ahead, so a flip while locked
swaps every stage at once. `color` paints under the image and fills `contain`'s letterbox; it and
the dims apply at once, without a decode.

## Session actions

`[lock.session]` `actions` (gated by `enabled`) names which of suspend, reboot and power-off the
sheet may offer; each one is also gated by logind's own `Can*` answer, fetched and performed by the
same logind connection as everything else in `logind.rs` — never a second bus connection, and never
serialized behind `SetLockedHint`, `TakeInhibitor` or `ReleaseInhibitor`, since a fetch or a perform
is spawned off the worker's request queue rather than awaited on it. `"yes"` shows the row; `"no"`,
`"na"` and `"challenge"` hide it, since polkit cannot prompt through a session lock; `"inhibited"` and
either `*-inhibitor-blocked` answer show it disabled, with a subtitle naming the first blocking
inhibitor from `ListInhibitors` whose `what` covers the action (`sleep` for suspend, `shutdown` for
reboot and power-off). The mapping from one answer to one `SessionActionState` is a pure function in
`session.rs`, unit-tested per answer.

The answers are cleared and refetched at the start of every lock, so a fresh lock never shows a row
enabled on a previous lock's stale answer while the new one is in flight. Once locked, a
`BlockInhibited` change refetches again; `zbus`'s change stream fires once immediately on whatever is
already cached, and again on every toggle while unlocked, so a refetch only runs while surfaces
exist — the App gates it, not the worker. The computed states are cached and pushed to every stage
through `Look`, so a monitor hotplugged mid-lock gets them the same way it gets the background and
the clock.

A requested action calls `suspend`/`reboot`/`power_off` with `interactive = false`, bounded by the
worker's own 5 s timeout, and carries the requesting monitor and a request id rather than one shared
slot. A second request is ignored while one is already in flight. A result clears the in-flight
state only when its id matches, whichever way it ends; any other is logged at debug and dropped, and
ids come from one counter on the App, so two locks never share one. A success closes the sheet on
every stage. A timeout shows nothing — the call may still land at logind after the fact — and only
logs a warning; an explicit D-Bus error is the only case that shows a short, translated failure
message, on the stage that asked, falling back to the interactive stage if that one is gone, and
logged at warn with neither shown if both are gone. `--standalone` never talks to logind, so the
sheet stays empty and the power button hidden; AccountsService is still read for the display name.

## Now playing, notifications and status

`Mpris`, `Compositor`, `Keyboard`, `Battery`, `Network` and `Bluetooth` from `glimpse_services`, plus
the notifications and weather providers from `glimpse_dbus`, are hosted the way the panel hosts
them — started once in `spawn_services` at daemon start, never on the lock path, and reconfigured
rather than restarted on a reload. Each publishes a `tokio::sync::watch`; the App forwards every
change to its own input and keeps the latest value, so all of them read live whether or not a lock
exists yet. Every hosted service reads the document through `hosted`: `[mpris] fetch-art` is always
`false`, since the locker never shows album art, and `[keyboard] remember` is always `global`, so
the locker's `Keyboard` only mirrors the layout and never switches it behind the panel's back.

**`Network` and `Bluetooth` run with `agent: false`.** An agent answers pairing confirmations and
Wi-Fi secret requests; a locked screen that registered one would accept a pairing or hand out a
password prompt to whoever is standing at it. Both are mirrors here and nothing else.

`status.rs` maps each state to one `IndicatorSpec` for the `StatusIsland`, unit-tested per case:
battery is the display device's icon and `{percentage}%`, `Low` a warning and `Critical`/`Action`
an error, never `attention`; network and bluetooth are their service's icon alone, so no SSID or
device name reaches the island; the layout is the current code, cleaned and capped, and hidden
under two layouts; weather is the condition icon and reading of the `here` place, else of the
first place with a reading, hidden with none or with no provider, and with no trouble chip, alert
icon or severity. The locker never leases a weather place: it shows what the panel's weather
applet already asked for, and requests nothing that would override a fixed place or keep GeoClue
busy. Network stays hidden until the service's first publish, so the island never shows "offline"
for a service that has not looked yet. A slot restages only when its icon, label or severity
changed. `[lock.status] enabled = false` empties all five slots at `look()`, so a reload applies it
without a restart. The island's indicators send no command; the power button is its only control.

The hosted `Compositor` is both `Keyboard`'s dependency and the source of the focused output that
`[lock] prompt-output = "focused"` follows, read from its outputs and forwarded only when it moves.

Two pure functions turn that state into what a stage shows, each unit-tested per case: `media::track_of`
picks the player MPRIS marks current and maps it to a `Track`, `None` with no current player or
`[lock.media] enabled = false`; `chips::groups_of` keeps notifications created at or after the lock
began — a notification replaced while locked counts as a new one, by its fresh `created` time — groups
them by `app_id` (never `app_name`, which a sender chooses freely), and orders a critical group first,
then by the most recent record. `[lock.notifications] privacy` picks between one chip per app and one
chip carrying the total; an app's chip carries the icon of its most recent notification that has one,
else the generic bell. Both are pushed to every stage through `Look`, so a monitor hotplugged mid-lock
gets them the same way it gets the background and the clock.

The moment a lock's notifications window starts, `lock_started`, is set once per lock cycle and
survives an unrequested-unlock relock — which never authenticates and so never really left the lock
notifications arrived under — so a stolen relock does not lose them. Only an authenticated unlock
clears it, which is what makes the next lock's window start fresh.

`TrackCard`'s play/pause and next call `MprisHandle::control` off the GTK thread; a failure is logged
at warn and never shown, matching every other command in this crate. Bodies and summaries never reach
a chip — only `app_id`, `app_name`, `icon` and `urgency` do.

## Rules

Depends on no other glimpse process. It reads its configuration from disk and reaches logind and
compositor IPC directly, so there is nothing whose absence can blank it and nothing to wait for
before it can lock. Nothing is awaited between a lock request and creating the surfaces. The
notifications and weather providers are read optionally: either being absent hides its chips or its
slot and touches nothing else. Its own notices go to `org.freedesktop.Notifications.Notify`, which
any notification daemon serves, off the GTK thread and capped at 5 s.

If the process dies after `locked`, the compositor keeps the session locked and shows a blank
screen. That is correct and must not be worked around. `LockedHint` is left true, so the restarted
locker re-acquires the lock.

The lifecycle is one state machine in `lifecycle.rs` that takes inputs and returns effects; GTK,
zbus and PAM only feed it and carry out what it returns. Every lock cycle has a generation, and a
`locked`, `failed`, `unlocked` or paint signal from another generation is ignored.

An `unlocked` that arrives without this process having asked for it is a failure, not an unlock: it
is logged at `error` and the lock is re-acquired at once under a new generation, with `LockedHint`
left true. The hint is cleared in two places only: on the `unlocked` that ends the locker's own
`Instance::unlock`, when the phase is `Unlocking` and the hint is one it set, and on `failed` after
it had set one. logind's `Unlock` signal is ignored — only local PAM authentication ends a lock, and
there is no D-Bus method that unlocks and no locker D-Bus API at all.

`LockedHint` is only ever cleared when this process set it: a hint a dead locker left is not ours.

Every attempt carries its own id, and a result whose id is not the current attempt's is discarded,
whether it is from the same lock cycle or an earlier one. Each attempt runs on its own thread with a
30 s timeout; a PAM call cannot be cancelled, so a hung thread is abandoned. At most two threads
exist: while one is abandoned and one is in flight, submitting is refused. The entry is cleared on
every attempt, and a new lock cycle builds a new, empty prompt. A submit records the monitor it came
from, a submit the lifecycle drops clears that record, and the attempt reads the password only if
that monitor is still the interactive one and the entry is not empty; otherwise it fails without
reaching PAM, since interactivity moving between the submit and the attempt clears the entry and an
empty password would still spend a faillock try.

The username reaching PAM is resolved from `$USER`, then `/etc/passwd` by uid, and validated against
the POSIX portable set (not starting with `-`, never `.` or `..`) before it is used — a later
`/var/lib/AccountsService/users/<name>` lookup is a path traversal waiting for a separator. Failing
to resolve one refuses the lock rather than authenticating against an account that does not exist.

The conversation answers `echo-on` with that username and exactly one `echo-off` with the password;
a second `echo-off` gets `CONV_ERR`. A module retrying its prompt is indistinguishable from a real
second factor, and re-sending the password is the worse guess. `text-info` and `error-msg` are kept,
sanitized and capped, and **a PAM text always wins over "Wrong password"**: `pam_faillock` sends its
lockout sentence through the conversation and then returns plain `AUTH_ERR`.

A correct password whose `acct_mgmt` answers `NEW_AUTHTOK_REQD` or `AUTHTOK_EXPIRED` unlocks and
posts a notice to change it; `ACCT_EXPIRED` and every other answer refuse.

`AUTHINFO_UNAVAIL` never renders as a wrong password. It names `glimpse-lock check`, because it is
the symptom of the sandbox trap below and mislabelling it is what makes that failure expensive.

The password is read out of the entry once per attempt into a `Zeroizing<String>`, moved into the
PAM thread and dropped there. No struct holding the password derives `Debug`, and it is never
formatted or logged. Two copies escape the wipe: the `CString` the conversation answers with,
whose `Drop` overwrites only its first byte, and the `strdup` pam-client2 makes of it for the reply
libpam hands to the module, which the module frees. Both are short-lived and cannot be avoided.

## The sandbox self-probe

**Never sandbox this unit — no systemd sandboxing option of any kind.** Namespace options put a
user service in a user namespace where root is unmapped and `unix_chkpwd`'s setuid and setgid bits
are ignored; seccomp-family options imply `NoNewPrivileges`. Either way PAM returns
`AUTHINFO_UNAVAIL` and the correct password is rejected. A sandbox is also inherited, which is why
the daemon never locks from its own command line.

At start the daemon checks its own process: `/proc/self/uid_map` must be exactly `0 0 4294967295`,
`NoNewPrivs` must be 0, and the first `unix_chkpwd` in `/usr/bin`, `/usr/sbin`, `/sbin` must be
owned by root and setuid or setgid — Debian ships it `2755 root:shadow` — on a filesystem without
`nosuid`. No `unix_chkpwd` at all skips that probe, since a stack on systemd-homed or sssd needs
none. The PAM stack for `[lock] pam-service` is checked too: the name is lowercased once at start,
as `pam_start` does, so the probe and PAM read the same file, and it is looked up in `/etc/pam.d`
and then `/usr/lib/pam.d`, as is every `include`, `substack` and `@include` target it names,
nested ones and `\`-continued lines included. Missing, a target in neither directory, or an
uncommented `pam_permit.so` in the service file itself refuses locking like a failed probe.

When a probe fails, every `Lock` is refused and posts a notification naming `glimpse-lock check`,
no sleep inhibitor is taken, and a suspend posts one saying it is suspending unlocked. A lock taken
in that state could only be released from a text console, so an unlocked screen with a loud warning
is the lesser failure. The exception is `LockedHint` true at start: the compositor already holds a
dead locker's lock, so it locks anyway and the prompt replaces the entry with the text-console
instruction.

`check` reads `MainPID` of `glimpse-lock.service` from systemd over D-Bus and runs the `uid_map`
and `NoNewPrivs` probes against `/proc/<pid>`, then the daemon's own startup checks against itself —
a shell outside the unit's sandbox passes its own probes while the service fails. A missing PAM
stack matters because PAM then falls through to `other`.

A compositor without `ext-session-lock-v1`, or a `failed` lock, never ends the process: it stays
alive and notifies per refused `Lock`. Exiting would be a restart loop that `StartLimitBurst=5`
turns into a dead unit, and the next compositor would have no locker.

## The sleep handshake

A `delay` inhibitor for `sleep` is held from start and re-taken after every resume. On
`PrepareForSleep(true)`:

- locked, and one frame painted on every lock surface — the inhibitor is dropped at once, because
  `Instance::lock` on a locked instance fires neither `locked` nor `failed`;
- acquiring, or locked but not yet painted — it waits for both;
- idle — it locks, then waits the same way.

Painted means a frame clock `after-paint` on every lock window of the current generation, in
either order with `locked`: a lock surface presents its first frame before the compositor sends
`locked`, so a paint seen first still counts once `locked` arrives. A monitor added while locked
makes the lock unpainted again until its surface paints. The wait is capped at logind's
`InhibitDelayMaxUSec` minus 500 ms, so a slow compositor lets the suspend proceed rather than hold
it. Releasing on any other path suspends an unlocked machine.

A failed inhibitor is not retried on a timer: it is asked for again at the next resume and after
the next successful unlock. A logind signal stream that ends is logged at `error` once.

At start, connecting to the system bus with the session lookup, and then subscribing to the three
signals, are each capped at 5 s; either expiring is logged at `error` and the locker starts as if
logind were unreachable. `LockedHint` and `InhibitDelayMaxUSec` are then read concurrently, each
capped at 5 s. After that, logind signals are forwarded on their own task, and method calls run in
order on another, each capped at 5 s, so a wedged reply delays only the calls queued behind it. A
call that times out is logged at `warn`; the inhibitor fd is dropped only on a release or at exit,
never by a timeout. A request sent after that worker has gone, or the worker dying, is logged at
`error`.

## Development

`--standalone` locks at once, never talks to logind (no `Lock`, `PrepareForSleep`, inhibitor,
`LockedHint` or session lookup), logs what it would have notified, and exits 0 after a
PAM-authenticated unlock and non-zero otherwise. The probe and PAM still run; nothing unlocks it
without PAM success. It is the only safe way to drive the locker inside a nested compositor.

## Configuration

`[lock]` `pam-service`, `prompt-output`, `[lock.background]`, `[lock.clock]` (`enabled`,
`time-format`, `date-format`), `[lock.session]` (`enabled`, `actions`), `[lock.media]` (`enabled`),
`[lock.notifications]` (`enabled`, `privacy`) and `[lock.status]` (`enabled`), and
`[power] lock-before-sleep` / `lock-on-request`, plus `lock.css`. The hosted services also read
their own tables — `[mpris]`, `[keyboard]`, `[network]`, `[bluetooth]` and the rest — with the two
overrides above. Tables owned by other binaries are otherwise ignored, not validated. It is re-read
through `glimpse_config::watch_config`, so both `SIGHUP` and a change under the configuration
directory apply it, and nothing in a reload can unlock. While locked a reload applies the
background, dim, fit, clock, prompt output, session actions, media, notifications and status to
every stage at once. `pam-service` is read once at start and a change to it is
logged and ignored until a restart: a same-user process that could repoint it mid-lock at a
`pam_permit.so` stack could otherwise unlock the screen.

A stylesheet that fails to parse keeps the previous one, and a deleted stylesheet clears its
provider. A half-applied stylesheet here can leave the password entry invisible with no other window
to fall back to.

The binary is a relm4 application: `main.rs` parses, probes and calls `RelmApp::run`, and `app.rs`
holds the component that owns the state machine, the config and theme watches and the CSS
providers. A hidden `adw::ApplicationWindow` keeps the `GtkApplication` alive with nothing on
screen, so a dead watch or a lost bus cannot end the process — a locker that returns is an unlocked
session, and under `Restart=always` a restart loop. Lock windows are built in one place,
`surface::build`, and `surface.rs` owns everything on them; `app.rs` hands it a `Look` and a
`PromptState` and never reaches a stage directly.
