# How the lock screen works

The lock screen is the one part of glimpse where a bug is a security failure rather than a cosmetic one.
It is also the most self-contained: `glimpse-lock` depends on no other glimpse process, reads its own
configuration, and reaches logind and the compositor directly, so nothing else's absence can blank it or
block a lock. Settings live in [Lock screen](/config/services/lock); this page is about the mechanics.

## Locking is a compositor state, not a window

The locker uses the `ext-session-lock-v1` Wayland protocol. When it locks, it creates one lock surface
per monitor, including a monitor hotplugged while already locked, before that monitor can show anything.
The session counts as locked only once the compositor confirms with its `locked` event; only then does
the locker report the state to logind. Every surface shows the same prompt state, but exactly one is
interactive and only it can submit a password, and a lock never has zero interactive prompts while any
surface exists.

If the locker process dies after the compositor has confirmed the lock, the compositor keeps the session
locked behind a blank screen. That is correct behavior and glimpse does not work around it: unlocking
because the locker crashed would make crashing the locker an unlock method. Instead, logind still reports
the session as locked, so the restarted locker sees that state at startup and re-acquires the lock with a
fresh prompt.

The lifecycle is one state machine fed by the compositor, logind and PAM. Two rules in it are worth
naming. An `unlocked` event this process did not ask for is treated as a failure, logged as an error, and
the lock is re-acquired at once. And logind's `Unlock` signal is ignored: only a local PAM authentication
ends a lock, so nothing on a bus can talk the screen open.

## logind drives when a lock happens

The resident daemon locks on three triggers:

- logind's session `Lock` signal. The `glimpse-lock lock` command sends the matching `LockSession` call
  and then waits until logind's `LockedHint` turns true (giving up with exit code 4 after 5 seconds), so
  a following suspend command cannot race ahead of the lock.
- System sleep, through the handshake below.
- Startup, when `LockedHint` is already true. This is the dead-locker case above: the session is locked,
  so the new process locks immediately instead of waiting.

`LockedHint` is logind's record of whether the session is locked, and the locker is careful about when it
changes. It sets the hint once the compositor confirms the lock, and clears it only when its own unlock
sequence completes, or when a lock it set the hint for fails. It stays true across a locker restart, which is what makes both the re-acquire at
startup and the "never unlock by accident" rules possible.

## The sleep handshake

Locking before sleep has to finish before the machine actually suspends, or the laptop wakes up showing
the unlocked desktop. The locker holds a logind `delay` inhibitor for sleep from startup and re-takes it
after every resume. If its startup self-probe fails, it takes no inhibitor and refuses to lock. On `PrepareForSleep`, it releases the inhibitor only when the session is locked and
every lock window of the current lock has drawn a frame; "painted" matters because a lock surface that
exists but has not drawn yet can still flash the desktop on resume. If the session is unlocked, it locks
first and waits for the same condition. The wait is capped just under logind's own delay limit
(`InhibitDelayMaxUSec` minus 500 ms), so a slow compositor lets the suspend proceed rather than holding
the machine awake. Every logind call is capped at 5 seconds, and the inhibitor is released only on this
path or at exit, never by a timeout. Releasing it anywhere else would suspend an unlocked machine.

## PAM does the authentication

Passwords are checked through PAM, using the service named by `[lock] pam-service`, off the UI thread.
Each attempt runs on its own thread with a 30 second timeout, because a PAM call cannot be cancelled; at
most two attempts exist at once and a further submit is refused. The username handed to PAM is the name
for the user's uid from `/etc/passwd` (falling back to `$USER`), validated against the portable character
set, and a username that cannot be resolved refuses the lock rather than authenticating against an
account that may not exist. The password is held in memory that is zeroed on drop and is answered to
exactly one password prompt; a module that asks twice gets a conversation error instead of the password
again. A message PAM sends through the conversation, such as a faillock lockout notice, is shown as
itself rather than collapsed into "wrong password".

The PAM service name is read once at startup. A configuration change to it is logged and ignored until
restart, because a same-user process that could repoint it during a lock could aim authentication at a
stack that always succeeds. The installed PAM files (`data/pam.d/`) cover the auth and account stacks
only, and deny the password and session stacks, since a locker must never change a password or open a
session.

While locked, the screen shows limited status (media, battery, network and similar read-only state).
Two restrictions follow from the threat model: the network and bluetooth views run without their agents,
so a locked screen never accepts a pairing or answers a Wi-Fi password prompt, and notifications appear
only as a total or as the names of the apps that sent them (`[lock.notifications] privacy`), never
with summaries or bodies.

## Why the unit is never sandboxed

`glimpse-lock.service` carries no systemd hardening options at all, and adding any is a bug. Both families
of options break PAM's password check in a way that looks like a wrong password:

- Namespace options (`PrivateTmp=`, `ProtectSystem=` and relatives) put the user service in a user
  namespace where root is unmapped, so the setuid helper PAM uses to verify passwords (`unix_chkpwd`)
  loses its privilege.
- Seccomp-family options (`SystemCallFilter=`, `LockPersonality=`, `RestrictSUIDSGID=` and relatives)
  imply `NoNewPrivileges`, which blocks the same helper.

Either way PAM answers `AUTHINFO_UNAVAIL` and the correct password is rejected. Because such a setting
is inherited, the `lock` subcommand also never locks in its own process: it only sends the logind
request and lets the resident daemon, started by the unit, do the locking.

Rather than trust the unit file, the daemon probes itself at startup: its uid mapping, its
`NoNewPrivs` flag, the ownership and mount flags of the password helper, and the PAM stack for its
service, which must resolve through every include it names and must not have an always-succeed module
(`pam_permit.so`) in the service file itself. When a probe fails, every lock
request is refused and a notification tells the user to run `glimpse-lock check`, which runs the same
probes against the running service. An unlocked screen with a loud warning is the lesser failure, since a
lock taken in that state could only be released from a text console. The one exception is a session that
is already locked when the daemon starts: then it locks anyway, and the prompt itself shows the
text-console instruction.

## Nothing may stop it while locked

A stopped locker is a locked session with nothing to authenticate against, so the unit's relationships are
chosen to make that unreachable. The unit is part of `graphical-session.target` only, and
`glimpse-session.target` starts it through `Wants=` alone. Stronger relationships (`PartOf=` on anything
else, `BindsTo=`, or another unit's `Conflicts=`) could all stop the locker as a side effect of stopping
something else; `Wants=` cannot. The unit also sets `Restart=always` with `StartLimitIntervalSec=0`, so
systemd never gives up restarting it. Reload propagation from the session target deliberately skips it.

For packagers, the practical consequence is short: do not add hardening options to this unit, do not add
unit relationships that can stop it, and install one of the shipped PAM files for your distribution's
stack (`data/pam.d/debian` and `data/pam.d/fedora` cover the two common layouts).
