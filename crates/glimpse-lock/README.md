# glimpse-lock

The screen locker: `ext-session-lock-v1` surfaces and PAM authentication. A bug here is a security
failure, not a cosmetic one.

## Commands

- `glimpse-lock` — the resident daemon under `glimpse-lock.service`; waits, locking at start only
  if `LockedHint` is already true.
- `glimpse-lock lock` — calls logind `LockSession`, exits 0 once `LockedHint` turns true or 4 after
  5s without it, so it can't race a following `systemctl suspend`. Sends one D-Bus call, so it's
  safe from a sandboxed parent.
- `glimpse-lock check` — one `name: ok|fail (reason)` line per check, exit 5 if any fails, plus the
  `user` line PAM would authenticate against.
- `glimpse-lock --standalone` — development only: locks at once without talking to logind and logs
  what it would have notified; the probe and PAM still run.
- Exit codes: 0 ok, 1 failure, 3 configuration, 4 not locked in time, 5 a check failed.

## What it does

Locks on logind `Session.Lock` (`[power] lock-on-request`), before sleep
(`[power] lock-before-sleep`), and at start when `LockedHint` is already true. Creates one lock
surface per monitor, including one hotplugged while locked, before it can show anything, and treats
the session as locked — setting `LockedHint` — only once the compositor sends `locked`.
Authenticates through PAM service `[lock] pam-service` off the UI thread. Every surface carries a
`LockStage` mirroring the same prompt state, but exactly one is interactive and only it submits; a
lock never has zero interactive prompts while any surface exists. Shows AccountsService `RealName`,
else `UserName`, refreshed on every lock and never awaited — the stage shows the PAM username until
it answers.

`[lock.background]` `image`/`image-dark` decode off the UI thread, never at the lock itself; naming
either inherits the whole pair from `[wallpaper]`, and both scheme variants decode ahead so a
dark/light flip while locked swaps every stage at once. `[lock.session]` `actions` (gated by
`enabled`) names which of suspend, reboot, power-off the sheet offers, each also gated by logind's
own `Can*` answer — hidden where polkit can't prompt through a lock, disabled with the blocking
inhibitor's reason otherwise.

`Mpris`, `Compositor`, `Keyboard`, `Battery`, `Network` and `Bluetooth`, plus the notifications and
weather providers, are hosted once at daemon start, never on the lock path. **`Network` and
`Bluetooth` run with `agent: false`**, since a locked screen that registered an agent would accept
a pairing or hand out a Wi-Fi password prompt to whoever is standing at it. `status.rs` never shows
an SSID or device name, and the locker never leases a weather place of its own, only what the
panel's applet already asked for. Notifications group by `app_id` (never `app_name`,
sender-chosen), and `lock_started` marks which count as "since this lock", clearing only on a real
authenticated unlock; bodies and summaries never reach a chip, only `app_id`, `app_name`, `icon`
and `urgency`.

## Rules

Depends on no other glimpse process — it reads its own config and reaches logind and the compositor
directly, so nothing else's absence can blank it or block a lock. **If the process dies after
`locked`, the compositor keeps the session locked with a blank screen — that is correct and must
not be worked around.** `LockedHint` stays true, so a restarted locker re-acquires the lock.

The lifecycle is one state machine in `lifecycle.rs`; GTK, zbus and PAM only feed it and carry out
its effects, and a stale-generation signal is always ignored. **An `unlocked` this process didn't
ask for is a failure, not an unlock**: it's logged at `error` and the lock is re-acquired at once
under a new generation, `LockedHint` left true. The hint clears only on the `unlocked` ending this
locker's own `Instance::unlock`, or on a `failed` after this process set it — never on logind's
`Unlock` signal, which is ignored. Only local PAM authentication ends a lock.

Every PAM attempt carries its own id, and a result whose id isn't current is discarded. Each
attempt runs on its own thread with a 30s timeout, since a PAM call can't be cancelled; at most two
threads exist (one abandoned, one in flight), and a further submit is refused. A submit records the
monitor it came from; the attempt reads the password only if that monitor is still interactive and
the entry isn't empty, so interactivity moving in between fails closed rather than spending a
faillock try on an empty password.

**The username reaching PAM is `/etc/passwd`'s name for the uid, else `$USER`, validated against
the POSIX portable set (no leading `-`, never `.` or `..`)** — a later
`/var/lib/AccountsService/users/<name>` lookup is a path-traversal target otherwise, and a username
that fails to resolve refuses the lock rather than authenticating against an account that doesn't
exist.

The conversation answers `echo-on` with the username and exactly one `echo-off` with the password;
a second `echo-off` gets `CONV_ERR` rather than resending it, since a module re-prompting is
indistinguishable from a real second factor. **A PAM text always wins over "Wrong password"**:
`pam_faillock` sends its lockout sentence through the conversation, then returns plain `AUTH_ERR`.
A correct password whose `acct_mgmt` answers `NEW_AUTHTOK_REQD`/`AUTHTOK_EXPIRED` unlocks and
posts a password-change notice; every other answer refuses. **`AUTHINFO_UNAVAIL` never renders as
a wrong password** — it names `glimpse-lock check`, since it's the sandbox trap below.

The password lives in a `Zeroizing<String>`, moved into the PAM thread and dropped there; no struct
holding it derives `Debug`. Two short-lived copies unavoidably escape the wipe — the conversation's
reply `CString` and pam-client2's own `strdup` — both freed by the module.

## The sandbox self-probe

**Never sandbox this unit — no systemd sandboxing option of any kind.** Namespace options
(`PrivateTmp=`, `ProtectSystem=`, …) put it in a user namespace where root is unmapped and
`unix_chkpwd`'s setuid/setgid bits are ignored; seccomp options imply `NoNewPrivileges`. Either way
PAM returns `AUTHINFO_UNAVAIL` and the correct password is rejected — and a sandbox is inherited,
which is why the daemon never locks from its own command line.

At start the daemon probes itself: `/proc/self/uid_map` must read exactly `0 0 4294967295`,
`NoNewPrivs` must be 0, and the first `unix_chkpwd` on `PATH` must be root-owned and setuid/setgid
on a filesystem without `nosuid` (no `unix_chkpwd` at all skips the probe). The PAM stack for
`[lock] pam-service` is resolved through every `include`/`substack`/`@include` it names; missing,
unresolvable, or an uncommented `pam_permit.so` in the service file itself refuses locking like a
failed probe. `check` runs the same probes against the running service's `MainPID`, then against
itself, so a shell outside the unit's sandbox can't pass while the service fails.

**When a probe fails, every `Lock` is refused** and posts a notification naming `glimpse-lock
check`; no sleep inhibitor is taken. An unlocked screen with a loud warning is the lesser failure,
since a lock taken in that state could only be released from a text console — except when
`LockedHint` is already true at start, where a dead locker already holds the lock, so it locks
anyway and the prompt shows the text-console instruction instead.

A compositor without `ext-session-lock-v1`, or a `failed` lock, never ends the process — it stays
alive and notifies per refused `Lock`. **The unit sets `StartLimitIntervalSec=0`, because a locker
that stops while it holds the lock strands the session locked with nothing to authenticate
against.**

## The sleep handshake

A `delay` inhibitor for `sleep` is held from start and re-taken after every resume. On
`PrepareForSleep(true)`: already locked and painted releases it at once; acquiring or locked but
unpainted waits for both; idle locks first, then waits the same way. Painted means every lock
window of the current generation has drawn a frame; the wait is capped at logind's
`InhibitDelayMaxUSec` minus 500ms so a slow compositor lets suspend proceed rather than hold it.
**No unit relationship may stop this service while it holds the lock — releasing the inhibitor on
any other path suspends an unlocked machine.** A failed inhibitor is retried at the next resume and
after the next successful unlock, not on a timer; every logind call is capped at 5s, so a wedged
reply delays only calls queued behind it, and the inhibitor fd is dropped only on a release or at
exit, never by a timeout.

## Configuration

`[lock]` (`pam-service`, `prompt-output`), `[lock.background]`, `[lock.clock]`, `[lock.session]`,
`[lock.media]`, `[lock.notifications]`, `[lock.status]`, plus `[power] lock-before-sleep`/
`lock-on-request` and `lock.css`; other binaries' tables are ignored, not validated. Reloaded
through `glimpse_config::watch_config`, and nothing in a reload can unlock. **`pam-service` is read
once at start and a change to it is logged and ignored until restart** — a same-user process that
could repoint it mid-lock could otherwise point PAM at a `pam_permit.so` stack and unlock the
screen. A stylesheet that fails to parse keeps the previous one, which can leave the password entry
invisible with nothing to fall back to.

The binary is a relm4 application: `main.rs` parses, probes and calls `RelmApp::run`, behind a
hidden `adw::ApplicationWindow` that keeps `GtkApplication` alive with nothing on screen, since a
locker that returns is an unlocked session and, under `Restart=always`, a restart loop.
