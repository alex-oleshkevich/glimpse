---
state: draft
---

# 006 — Lock

Locking the session: compositor surfaces, PAM authentication, rate limiting and getting back in.
`glimpse-lock` is the artifact that does it.

## Problem

A locker is the one component where a bug is a security failure rather than a cosmetic one. It has
to come up when the rest of the session is broken, cover every output before anything sensitive is
visible, and never leave the user unable to get back in.

The failure that costs the most is not a crash. It is a correct password being rejected, because
every plausible cause — a sandboxed unit, a stripped setuid bit, an unreachable backend — produces
the same three words on screen. Diagnosing it from the outside means guessing.

## Goals

- Cover every output before the session is considered locked, using the compositor's guarantee
  rather than a best-effort overlay.
- Authenticate through PAM against the system's real configuration.
- Start and unlock with `glimpsed` dead.
- Fail closed: a crash after locking leaves the session locked, not exposed.
- Make a rejected-correct-password diagnosable in one command.

## Non-goals

- No login-manager role. Starting a session is a different program; greetd's protocol is not
  reachable from inside a running session.
- No dependence on the daemon for anything functional.
- No password storage of any kind, and no fallback authentication path around PAM.

## Tech

### Locking

Acquire `ext_session_lock_manager_v1`, create one lock surface per output, and treat the session as
locked **only** when the compositor sends `locked`. If `finished` arrives instead the lock was
refused, and the process exits non-zero without ever pretending to have locked.

A monitor connected while locked gets a lock surface before it can display anything.

**An unsolicited `unlocked` is a failure, not an unlock.** If the compositor releases the lock
without this process having authenticated anyone, the lock is re-acquired immediately rather than
taken at face value. That event is what a spoofed or misbehaving compositor looks like from inside,
and the only safe reading of it is that the screen is now exposed.

**One authentication panel per session, not per output.** Every output gets a lock surface, but the
password entry, the clock and the controls live on exactly one of them; the rest render the
background alone. Six screens showing a password field is six places for a shoulder to look, and
only one of them holds the keyboard focus anyway. On monitor add or remove the surfaces are
reconciled and the panel reassigned, so unplugging the monitor that held it does not leave the
session with nowhere to type.

**Nothing coordinates two lockers, because nothing has to.** A second process requesting a session
lock while one is held is refused by the compositor with `finished` and exits 7. The previous
implementation held a D-Bus name to enforce this, which is a lock on a lock.

If the process dies after `locked`, the compositor keeps the session locked and shows a blank
screen. That is the correct outcome and must not be worked around.

### Session state and logind

The compositor owns whether the screen is covered. logind owns what the rest of the system believes
about the session, and the two have to agree — otherwise `loginctl`, remote-desktop agents and idle
handlers all read a locked session as unlocked.

**`LockedHint` is set on `locked` and cleared on exit**, including the exit after a failed lock. It
is a hint, and nothing here depends on it for security; what depends on it is every other program's
idea of session state.

**`LockedHint` is also read at start.** True at start means a previous locker died holding the lock,
so this one locks immediately rather than waiting to be asked. That is the path back from a crashed
locker, and without it the session sits on a blank locked screen with no process able to unlock it.

**logind's `Unlock` signal is ignored.** It is worth being precise about why, because the obvious
objection is that `loginctl unlock-session` is polkit-gated. The gate is on
`org.freedesktop.login1.lock-sessions`, `auth_admin_keep`, and it covers the `Manager` methods that
act on *another* user's session. The per-session `Session.Unlock()` method short-circuits polkit
when the caller's uid matches the session owner, so any process running as the user — a script, a
compromised helper — opens the screen with one unauthenticated D-Bus call. Honouring the signal
makes the entire PAM path optional for anything already inside the session.

The legitimate reason to honour it is an external authentication agent: a smartcard, a fingerprint
daemon, a proximity token, a display manager that re-authenticated on a VT switch. Those verify the
user out of band and then tell the locker to stop asking. This design has no such agent — the PAM
conversation forecloses second factors deliberately — so the cost buys nothing.

The signal is logged and discarded. `Lock` is honoured, and the asymmetry is the point: honouring
`Lock` costs a screen the user can open with their password, honouring `Unlock` costs the password.

**The session is resolved, not assumed:**

| Order | Source                                | Rejected when                        |
| ----- | ------------------------------------- | ------------------------------------ |
| 1     | `XDG_SESSION_ID` through `GetSession` | the session's `User` is not this uid |
| 2     | `GetSessionByPID` for this process    | the call fails                       |
| 3     | `ListSessions`, filtered to this uid  | no candidate — a hard error          |

The uid check on step 1 is the point of the sequence. `XDG_SESSION_ID` is an ordinary environment
variable, inherited and forgeable, and a session belonging to someone else must never receive this
session's `LockedHint` or its power actions. Failing to read the ownership counts as a mismatch.

**Suspend is coordinated by systemd, not by an inhibitor held here.** `009_systemd.md` orders the
locker before `sleep.target`, so the lock surface exists before the machine suspends without this
process holding a logind delay inhibitor of its own. The previous implementation held one because it
was resident and had no unit to order — and paid for it with a file descriptor whose lifetime
spanned lock cycles, had to survive an unsolicited re-lock, and leaked on every path that forgot to
drop it.

### Who is being authenticated

PAM needs a username, and the screen shows a name and a face. None of the three is a given.

| Value        | Resolution order                                               |
| ------------ | -------------------------------------------------------------- |
| username     | `$USER` when non-empty → `/etc/passwd` by uid                  |
| display name | AccountsService `RealName` → the GECOS name → the username     |
| avatar       | AccountsService `Icon` → `~/.face` → `~/.face.icon` → initials |

**There is no third fallback for the username, and failing to resolve one refuses the lock.** A
process that cannot name the account it is authenticating would hand PAM a placeholder and get back
`USER_UNKNOWN`, which reads on screen as a wrong password against a session nobody can unlock. This
is the one place the locker deliberately fails open: an unlocked screen with a loud error is
recoverable, and a locked screen with no valid account is not.

**The username is validated against the POSIX portable set before it reaches a path.**
AccountsService state lives at `/var/lib/AccountsService/users/<username>`, so a username containing
a separator is a path traversal — and the value came from the environment.

All three are re-resolved each time the session locks rather than once at start, so a changed avatar
or real name appears at the next lock.

### Authentication

`pam_start` with the service from `[lock] pam_service`, then `pam_authenticate`, then `pam_acct_mgmt`,
all off the UI thread. A PAM module that blocks otherwise freezes the locker, and a frozen locker is
resolved by holding the power button.

**One password prompt.** The conversation answers the first `echo-off` prompt with the password and
reports any second one as a conversation error. This is deliberate and it costs something: a module
legitimately asking for a second factor is indistinguishable from a module retrying its single
prompt after a transient failure — an interrupted read, an SSSD reconnect. Guessing wrong in the
permissive direction means the locker silently re-sends the password; guessing wrong in the strict
direction means an honest failure message. The strict direction is the safe one, and it forecloses
multi-factor and challenge-response modules until something can tell the two cases apart.

Text from `text_info` and `error_msg` is captured and shown, so whatever PAM wanted to say reaches
the user even when the prompt itself is refused. **What PAM said beats the generic message** — a
`pam_faillock` "2 attempts remaining" is worth more to the user than "Authentication failed".

**The submit path is single-flight.** While a call is in flight, further submissions are dropped
rather than queued: queueing lets a held Enter key spend the whole cooldown ladder without a human
reading a single message, and a PAM context is not reentrant. The status stays on `Checking…` so
the UI does not read as ignored.

**An empty password never reaches PAM** and does not count as a failure. `pam_authenticate` runs
with `DISALLOW_NULL_AUTHTOK`, so a stack that would otherwise accept an empty token does not.

**Every attempt carries the generation of the lock cycle it started in, and a result whose
generation no longer matches is discarded.** Without it, a call still outstanding when the
compositor force-releases the lock can return `Success` into the *next* lock cycle and open a screen
nobody authenticated against.

**Authentication times out after 30 seconds.** A module waiting on an unreachable LDAP or SSSD
backend otherwise pins the prompt on `Checking…` with no way to retry short of the power button. The
timeout gets its own message rather than rendering as a wrong password — it is the same class of
fault as `AUTHINFO_UNAVAIL`.

**The two failure paths carry different codes**, and conflating them shows the wrong message:

| Source            | Codes unique to it                                       |
| ----------------- | -------------------------------------------------------- |
| `pam_authenticate`| `MAXTRIES`, `AUTHINFO_UNAVAIL`                           |
| `pam_acct_mgmt`   | `NEW_AUTHTOK_REQD`, `CRED_EXPIRED`, `AUTHTOK_EXPIRED`    |

`AUTH_ERR` is the only one that means "wrong password". Everything else — an expired account, an
unreachable backend, a locked-out account — gets its own message. **`AUTHINFO_UNAVAIL` in
particular must never render as a wrong password**: it is the symptom of the sandbox trap below, and
labelling it correctly is what turns a day of debugging into a sentence.

**The password is a `SecretString`**: zeroed on drop, redacted in `Debug`, never logged. Honesty
about the limit — the plaintext still exists briefly in the C string handed to the PAM crate and in
libpam's own copy, and neither is under this program's control. Zeroing the buffer this program owns
is worth doing and is not the same as guaranteeing the secret never sat in memory.

### Rate limiting

A cooldown ladder between consecutive failures: the first failure is free, then 1, 2, 4, 8, 16
seconds, capped at 30. An honest typo is not punished; a script is.

The ladder is a pure function of the failure count so the policy can be tested without driving the
UI, and the remaining time is shown rather than leaving the field mysteriously inert.

The counter is scoped to one lock cycle: it resets on success and when a new lock begins, so no
penalty carries across an unlock. It does not persist, and a restarted locker starts at zero. That
is accepted rather than worked around — the ladder exists to keep guessing at human speed, and
`pam_faillock` is what enforces a real lockout. Duplicating its bookkeeping in a process that can be
killed would only look like enforcement.

### Diagnosing a lockout

`glimpse-lock check` runs the three tests that explain a rejected correct password, and exits
non-zero if any fails:

| Check          | Passes when                                                          |
| -------------- | --------------------------------------------------------------------- |
| `no_new_privs` | `/proc/self/status` reports `NoNewPrivs: 0`                          |
| `pam_file`     | `/etc/pam.d/glimpse-lock` has no uncommented `pam_permit.so`         |
| `unix_chkpwd`  | `/usr/bin/unix_chkpwd` is root-owned with the setuid bit set          |

`no_new_privs` catches the sandboxing trap in `009_systemd.md` from inside the process, which is the
only place the answer is unambiguous.

`pam_file` is a **security** check, not a convenience one. `pam_permit.so` authenticates everyone;
it belongs in a rescue PAM file used to recover a locked-out session and nowhere else, and leaving
it behind turns the locker into a decoration. Checking for it makes that mistake loud.

### The lock screen

Battery, network, keyboard layout and weather come from topics and go blank when the daemon is
down. Unlock, power actions and keyboard-layout switching use logind and compositor IPC directly and
keep working: this is invariant 5 in `001_architecture.md`.

The clock refreshes on the next minute boundary rather than every sixty seconds, so it changes when
the minute changes instead of drifting away from it.

Opacity transitions respect the GTK animation setting, so a user who has turned animations off gets
none.

**Caps Lock is shown while it is on.** It is the most common reason a correct password is typed
wrong, and a password field is the one widget that shows no evidence of it.

Power actions live on the lock screen: suspend runs immediately, restart and shutdown ask first, and
`Escape` closes the menu. The confirmation is not a security measure — anyone at the keyboard can
hold the power button — it is there for the misclick that would discard every unsaved document in
the session.

### The background

`[lock.background]` is a colour, or an image with a fit mode, a blur radius and a dim factor
(`010_configuration.md`). Dim is applied over the image rather than baked into it, so adjusting it
costs nothing and never invalidates a decode.

Decoding follows `005_wallpaper.md`: the same fit modes, the same cache key, the same request-id
discipline, so a slow decode belonging to a superseded configuration cannot land on screen. Two
things differ, and both follow from what a locker is:

- **The decode is sized to the largest connected output, once.** Every lock surface shows the same
  image, and decoding it per monitor for a screen that is up for a minute is work nobody sees.
- **The background never gates the prompt.** The password entry is usable against the configured
  colour before any image is decoded. A locker that waits for a 4K JPEG is a locker that appears
  frozen exactly when the user is in a hurry.

### Styling

Three CSS providers on the display, lowest priority first: the built-in base, the theme pack's
`lock.css`, then the user's `themes/lock.css`. The rules from `004_panel.md` carry over — providers
are installed once and reloaded in place, and each connects `parsing-error`, because GTK4's loaders
return nothing.

**A stylesheet that fails to parse keeps the previous one.** A half-applied stylesheet on a panel is
ugly; on a lock screen it can leave the password entry invisible, and there is no other window to
fall back to. The candidate is parsed into a throwaway provider first, and the live provider is
replaced only when that comes back clean.

**A stylesheet that has been deleted clears its provider** instead of leaving the last-loaded rules
in place, so removing a file does what the user meant by removing it.

## The binary

```
glimpse-lock [COMMAND] [OPTIONS]
```

| Command | Effect                                          |
| ------- | ----------------------------------------------- |
| _none_  | Lock the session; exit when it is unlocked      |
| `check` | Run the lockout diagnostics above and exit      |

`check` prints one `name: ok` or `name: fail` line per diagnostic on stdout and runs all three
regardless of failures — the second and third answers are what disambiguate the first.

| Flag                    | Default                                  | Purpose                                                           |
| ----------------------- | ---------------------------------------- | ----------------------------------------------------------------- |
| `-c`, `--config <PATH>` | the layered stack                        | Use exactly this file                                             |
| `--socket <PATH>`       | `$XDG_RUNTIME_DIR/glimpse/glimpsed.sock` | Daemon socket, for decorative widgets only                        |
| `--css <PATH>`          | from config                              | Override the stylesheet                                           |
| `--no-daemon`           | off                                      | Do not connect to `glimpsed` at all; render clock and prompt only |
| `--check-config`        | off                                      | Validate configuration and stylesheet, exit                       |
| `--log <FILTER>`        | `info`                                   | `tracing-subscriber` filter                                       |
| `-V`, `--version`       |                                          |                                                                   |
| `-h`, `--help`          |                                          |                                                                   |

### Environment

| Variable          | Use                              |
| ----------------- | -------------------------------- |
| `WAYLAND_DISPLAY` | required                         |
| `XDG_SESSION_ID`  | logind session for power actions |

### Files

| Path                                       | Role                                                        |
| ------------------------------------------ | ----------------------------------------------------------- |
| `$XDG_CONFIG_HOME/glimpse/config.toml`     | the `[lock]` table; schema in `010`                         |
| `$XDG_CONFIG_HOME/glimpse/themes/lock.css` | stylesheet, relocatable via `lock.css_path`; daemon-watched |
| `/etc/pam.d/glimpse-lock`                  | PAM service definition, installed from `data/`              |

### Exit codes

| Code | Meaning                                                     |
| ---- | ----------------------------------------------------------- |
| 0    | unlocked normally, or `check` passed                        |
| 1    | configuration invalid per `--check-config`, or `check` failed |
| 2    | usage error                                                 |
| 5    | no Wayland display                                          |
| 6    | compositor does not support `ext-session-lock-v1`           |
| 7    | lock refused by the compositor (`finished` before `locked`) |
| 8    | PAM could not be initialised                                |
| 9    | the user to authenticate could not be resolved              |

Invalid configuration is not an exit. It logs and falls back to defaults at startup, and is dropped
on reload — see `010_configuration.md`.

## Risks

- **Operational** — systemd sandboxing breaks PAM. `NoNewPrivileges=`, `PrivateUsers=` and related
  hardening place the process in a user namespace that strips the setuid bit from `unix_chkpwd`,
  producing `AUTHINFO_UNAVAIL` and an unlockable session. See `009_systemd.md`, and `check` above.
- **Technical** — an authentication path that blocks the GTK main thread makes the locker appear
  frozen during a slow PAM module, which users resolve by force-rebooting.

## Alternatives considered

- **greetd as the authentication backend** — rejected: greetd is a login manager. Its socket exists
  only in the greeter process it spawned, and its protocol creates sessions rather than verifying a
  password inside a running one. A greeter built on greetd is a separate program that would share
  `glimpse-widgets`.
- **A layer-shell overlay instead of `ext-session-lock-v1`** — rejected: an overlay gives no
  guarantee it covers every output, and a crash exposes the desktop instead of keeping it locked.
- **Treating a second `echo-off` prompt as a second factor** — rejected for now, and this is the one
  worth revisiting. It would enable fingerprint, smartcard and challenge-response modules, at the
  cost of re-sending the password whenever a module retries a prompt for transient reasons. Nothing
  in the PAM conversation API distinguishes the two.
- **A grace period accepting unlock without a password** — rejected: it is a window in which the
  session is locked in appearance only, its length is a guess, and nothing else in this document
  needs it. A `--grace` flag was specified before the behaviour behind it was, which is how a
  security hole acquires a default value.
- **Holding a logind sleep delay inhibitor** — rejected: unit ordering does the same job without a
  descriptor whose lifetime has to survive lock cycles. See Session state and logind.
- **A literal fallback username when `$USER` and passwd both fail** — rejected: it authenticates
  against an account that does not exist and reports the result as a wrong password.
- **`preview`, `export-css` and `export-config` subcommands** — moved rather than rejected.
  Previewing the lock UI belongs to `glimpse-devtools`; writing out default configuration and
  stylesheets is `glimpsectl`'s job, for every binary at once.
- **A `status` subcommand and a bus name reporting lock state** — rejected: both only answer while a
  resident process is there to answer, which on-demand start removes. `LockedHint` is where the rest
  of the system reads this.
- **A resident locker holding a bus name** — rejected: the previous implementation ran as a daemon
  answering a `Lock` method, which keeps a process alive all session for an event that happens
  rarely, and makes the locker's own crash domain a session-long concern. On-demand start keeps
  `Restart=no` meaningful, per `009_systemd.md`.

## Changelog

- 2026-08-20 — created, split out of `001_architecture.md`.
- 2026-08-20 — configuration moved into the shared `config.toml` under `[lock]`.
- 2026-08-20 — invalid configuration logs and falls back to defaults instead of exiting; exit 1 is now only `--check-config`.
- 2026-08-20 — stylesheet changes arrive from the daemon's `watcher` service when it is running; the locker still loads its own files and never depends on it.
- 2026-08-20 — the stylesheet is `themes/lock.css` by default and relocatable via `lock.css_path`; the PAM service name comes from `lock.pam_service`.
- 2026-08-20 — renamed from `006_glimpse_lock.md` and reorganised around locking behaviour, with the binary and its flags as one section rather than the subject.
- 2026-08-20 — specified authentication from `_old/glimpse-lock`: one password prompt with the reasoning against MFA, the two distinct PAM code sets, `AUTHINFO_UNAVAIL` never rendering as a wrong password, `SecretString` zeroing and its honest limit, the cooldown ladder, and the `check` subcommand with its three diagnostics.
- 2026-08-20 — specified what a second pass over `_old/glimpse-lock` turned up: re-locking on an unsolicited `unlocked`, one authentication panel across outputs, `LockedHint` set and read, `Unlock` ignored, uid-checked session resolution, user identity resolution and its path-traversal guard, single-flight submits, the auth generation counter, the 30 s timeout, cooldown scope, Caps Lock, the power menu, the background and the stylesheet rules. Dropped `--grace`, which had a flag but no behaviour.
