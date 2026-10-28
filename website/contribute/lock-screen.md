# How the lock screen works

A bug in the lock screen is a security failure, so locking, blanking and the password prompt depend on
no other glimpse process. The mirrors it hosts (compositor, media, battery, notifications) are
optional: with none of them present the lock still locks and prompts. It reads its own configuration
and reaches logind directly. Settings live in
[Lock screen](/config/services/lock); this page is about the mechanics.

## Locking is a compositor state, not a window

The locker uses the `ext-session-lock-v1` Wayland protocol. It creates one lock surface per monitor,
including monitors plugged in while locked. The session counts as locked only once the compositor sends
its `locked` event, and only then does the locker tell logind. Every surface shows the same prompt, but
exactly one accepts a password.

If the locker dies while locked, the compositor keeps the session locked behind a blank screen. glimpse
does not work around that, because unlocking on a crash would make crashing the locker an unlock method.
logind still reports the session as locked, so the restarted locker re-acquires the lock with a fresh
prompt.

Two rules matter. An `unlocked` event the locker did not ask for is logged as an error and the lock is
re-acquired at once. And logind's `Unlock` signal is ignored: only a local PAM authentication ends a lock.

## logind drives when a lock happens

The resident daemon locks on three triggers:

- logind's session `Lock` signal. The `glimpse-lock lock` command sends the matching `LockSession` call
  and then waits until logind's `LockedHint` turns true (giving up with exit code 4 after 5 seconds), so
  a following suspend command cannot race ahead of the lock.
- System sleep, through the handshake below.
- Startup, when `LockedHint` is already true. This is the dead-locker case above: the session is locked,
  so the new process locks immediately instead of waiting.

`LockedHint` is logind's record of whether the session is locked. The locker sets it once the compositor
confirms the lock and clears it only when its own unlock completes or a lock it set the hint for fails.
It stays true across a locker restart, which is what lets the new process re-acquire the lock.

## The sleep handshake

The lock has to finish before the machine suspends, or it wakes up showing the desktop. The locker holds
a logind `delay` inhibitor for sleep from startup and re-takes it after every resume. If its startup
self-probe fails, it takes no inhibitor and refuses to lock.

On `PrepareForSleep`, it releases the inhibitor only when the session is locked and every lock window has
drawn a frame, because an undrawn lock surface can flash the desktop on resume. If the session is
unlocked, it locks first and waits for the same condition. The wait is capped just under logind's delay
limit (`InhibitDelayMaxUSec` minus 500 ms, or 4.5 seconds if unavailable). If the deadline passes, the
locker logs a warning and releases the inhibitor so sleep can proceed. Every logind call is capped at
5 seconds.

## PAM does the authentication

Passwords are checked through PAM, using the service named by `[lock] pam-service`, off the UI thread.
Each attempt runs on its own thread with a 30 second timeout, because a PAM call cannot be cancelled. At
most two attempts exist at once and a further submit is refused. The username is the one for the user's
uid in `/etc/passwd` (falling back to `$USER`); if it cannot be resolved, the lock is refused. The
password is zeroed on drop and answers exactly one prompt; a module that asks twice gets a conversation
error. A message from PAM, such as a faillock lockout notice, is shown as itself.

The PAM service name is read once at startup. A change is logged and ignored until restart, because a
process that could repoint it during a lock could aim authentication at a stack that always succeeds. The
installed PAM files (`data/pam.d/`) cover the auth and account stacks only and deny the password and
session stacks, since a locker must never change a password or open a session.

## Why the unit is never sandboxed

`glimpse-lock.service` carries no systemd hardening options at all, and adding any is a bug. Both families
of options break PAM's password check in a way that looks like a wrong password:

- Namespace options (`PrivateTmp=`, `ProtectSystem=` and relatives) put the user service in a user
  namespace where root is unmapped, so the setuid helper PAM uses to verify passwords (`unix_chkpwd`)
  loses its privilege.
- Seccomp-family options (`SystemCallFilter=`, `LockPersonality=`, `RestrictSUIDSGID=` and relatives)
  imply `NoNewPrivileges`, which blocks the same helper.

Either way the helper can't read `/etc/shadow`, PAM answers a plain authentication failure, and the
correct password is rejected as a wrong one. For the same reason the
`lock` subcommand never locks in its own process: it sends the logind request and the resident daemon
does the locking.

The daemon also probes itself at startup: its uid mapping, its `NoNewPrivs` flag, on systems whose
`/etc/shadow` its owner can't read (Fedora) its capability bounding set and securebits, the password helper's
ownership and mount flags, and the PAM stack, which must resolve every include and must not contain
`pam_permit.so` in the service file itself. When a probe fails, every lock request is refused and a
notification tells the user to run `glimpse-lock check`, which runs the same probes. An unlocked screen
with a warning is the lesser failure, since a lock taken in that state could only be released from a
text console. The exception is a session already locked when the daemon starts: it locks anyway, and the
prompt shows the text-console instruction.

## Nothing may stop it while locked

A stopped locker is a locked session with nothing to authenticate against. The unit is part of
`graphical-session.target` only, and `glimpse-session.target` starts it through `Wants=` alone. `PartOf=`,
`BindsTo=` or another unit's `Conflicts=` could stop the locker as a side effect; `Wants=` cannot. The unit
sets `Restart=always` with `StartLimitIntervalSec=0`, so systemd never gives up restarting it, except that `ExecCondition=` skips the start while the Wayland display
socket is missing, and reload
propagation from the session target skips it.

For packagers: do not add hardening options or unit relationships that can stop this unit, and install
one of the shipped PAM files (`data/pam.d/debian` or `data/pam.d/fedora`).
