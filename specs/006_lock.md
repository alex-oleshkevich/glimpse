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

If the process dies after `locked`, the compositor keeps the session locked and shows a blank
screen. That is the correct outcome and must not be worked around.

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
the user even when the prompt itself is refused.

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

### Status widgets

Battery, network, keyboard layout and weather come from topics and go blank when the daemon is
down. Unlock, power actions and keyboard-layout switching use logind and compositor IPC directly and
keep working: this is invariant 5 in `001_architecture.md`.

The clock refreshes on the next minute boundary rather than every sixty seconds, so it changes when
the minute changes instead of drifting away from it.

Opacity transitions respect the GTK animation setting, so a user who has turned animations off gets
none.

## The binary

```
glimpse-lock [COMMAND] [OPTIONS]
```

| Command | Effect                                          |
| ------- | ----------------------------------------------- |
| _none_  | Lock the session; exit when it is unlocked      |
| `check` | Run the lockout diagnostics above and exit      |

| Flag                    | Default                                  | Purpose                                                           |
| ----------------------- | ---------------------------------------- | ----------------------------------------------------------------- |
| `-c`, `--config <PATH>` | the layered stack                        | Use exactly this file                                             |
| `--socket <PATH>`       | `$XDG_RUNTIME_DIR/glimpse/glimpsed.sock` | Daemon socket, for decorative widgets only                        |
| `--css <PATH>`          | from config                              | Override the stylesheet                                           |
| `--grace <SECONDS>`     | `0`                                      | Accept unlock without a password for this long after locking      |
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
