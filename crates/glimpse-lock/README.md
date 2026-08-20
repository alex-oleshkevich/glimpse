# glimpse-lock

The screen locker. `ext-session-lock-v1` surfaces and PAM authentication.

The one component where a bug is a security failure rather than a cosmetic one.

## What it does

- Acquires `ext_session_lock_manager_v1` and creates one lock surface per output
- Treats the session as locked only after the compositor sends `locked`
- Authenticates through `pam_start("glimpse-lock", ...)` off the UI thread, so fingerprint and
  smartcard modules work
- Covers a monitor hotplugged while locked before it can display anything

## Rules

Never depends on `glimpsed` for anything functional. Battery and network widgets go blank when the
daemon is down; unlock, power actions and keyboard-layout switching use logind and compositor IPC
directly.

If the process dies after `locked`, the compositor keeps the session locked and shows a blank
screen. That is correct and must not be worked around.

**Never sandbox this unit.** `NoNewPrivileges=`, `PrivateUsers=` and `RestrictSUIDSGID=` put the
process in a user namespace that strips the setuid bit from `unix_chkpwd`. PAM then returns
`AUTHINFO_UNAVAIL`, the correct password is rejected, and the session cannot be unlocked. The
symptom looks like a wrong password, which is what makes it expensive to diagnose.

Spec: [`specs/006_glimpse_lock.md`](../../specs/006_glimpse_lock.md)
