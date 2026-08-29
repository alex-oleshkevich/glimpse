# glimpse-lock

The screen locker. `ext-session-lock-v1` surfaces and PAM authentication.

The one component where a bug is a security failure rather than a cosmetic one.

## What it does

- Acquires `ext_session_lock_manager_v1` and creates one lock surface per output
- Treats the session as locked only after the compositor sends `locked`
- Authenticates through `pam_start("glimpse-lock", ...)` off the UI thread, so a blocking module
  cannot freeze the surfaces
- Covers a monitor hotplugged while locked before it can display anything
- Shows the authentication panel on one output; the rest render the background alone
- Keeps logind's `LockedHint` in step with the compositor, and reads it at start to recover from a
  locker that died holding the lock

## Rules

Never depends on `glimpsed` for anything functional. It holds a client — `Client::open`, bound to
`_client` for the length of `run` — so the status widgets have a socket to read, and every one of
them goes blank when the daemon does. Unlock, power actions and keyboard-layout switching use
logind and compositor IPC directly and never touch it. Battery and network widgets go blank when the
daemon is down; unlock, power actions and keyboard-layout switching use logind and compositor IPC
directly.

If the process dies after `locked`, the compositor keeps the session locked and shows a blank
screen. That is correct and must not be worked around.

An `unlocked` event that arrives without this process having authenticated anyone is a failure, not
an unlock: re-acquire the lock rather than trusting it. logind's `Unlock` signal is ignored for the
same reason — only local PAM authentication ends a lock.

Every authentication attempt carries the generation of its lock cycle, and a result whose generation
has moved on is discarded. Otherwise a call outstanding across a forced re-lock can unlock a screen
nobody authenticated against. Submits are single-flight and time out after 30 s.

The username reaching PAM is resolved from `$USER`, then `/etc/passwd` by uid, and validated against
the POSIX portable set before it is used to build a path — `/var/lib/AccountsService/users/<name>`
is a path traversal waiting for a username with a separator in it. Failing to resolve one refuses
the lock rather than authenticating against an account that does not exist.

`AUTHINFO_UNAVAIL` never renders as a wrong password. It is the symptom of the sandbox trap below,
and mislabelling it is what makes that failure expensive.

The conversation answers one `echo-off` prompt and refuses a second: a module retrying its prompt is
indistinguishable from a real second factor, and re-sending the password is the worse guess.

`glimpse-lock check` runs the three diagnostics that explain a rejected correct password —
`NoNewPrivs`, no `pam_permit.so` in the PAM file, and a setuid root `unix_chkpwd`.

**Never sandbox this unit.** `NoNewPrivileges=`, `PrivateUsers=` and `RestrictSUIDSGID=` put the
process in a user namespace that strips the setuid bit from `unix_chkpwd`. PAM then returns
`AUTHINFO_UNAVAIL`, the correct password is rejected, and the session cannot be unlocked. The
symptom looks like a wrong password, which is what makes it expensive to diagnose.

A stylesheet that fails to parse keeps the previous one, and a deleted stylesheet clears its
provider. A half-applied stylesheet here can leave the password entry invisible with no other window
to fall back to.

Configuration is the `[lock]` table of the shared `config.toml`, plus `lock.css`. Tables owned by
other binaries are ignored, not validated. It is re-read through `glimpse_config::watch_config`, so
both `SIGHUP` and a change under the configuration directory apply it — the same two triggers every
other binary has, and neither of them needs `glimpsed` alive.

The process outlives its own startup even while the lock surfaces are unwritten. A locker that
returns is an unlocked session, and under `Restart=always` returning immediately is a restart loop.
Losing every reload trigger does not end it either. `RelmApp::run` supplies that: it blocks on the
GTK main loop, and the hidden `adw::ApplicationWindow` keeps the `GtkApplication` alive with nothing
on screen, so a dead watch cannot end the process.

The binary is a relm4 application in the same shape as `glimpse-panel` and `glimpse-wallpaper` —
`main.rs` parses, loads and calls `RelmApp::run`, and `app.rs` holds the component that owns the
config watch, the theme watch, the daemon client and the CSS providers. It was a `tokio::select!`
loop copied from `glimpse-sunset` until that shape ran out: sunset is genuinely headless, and this
crate draws a surface on every output. What the loop was for survives the move; what it shared with
a night-light service did not.
