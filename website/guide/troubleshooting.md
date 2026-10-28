# Troubleshooting

## Check everything at once

```sh
glimpsectl doctor
```

`doctor` checks the configuration and every provider glimpse depends on, and tells you what is
missing. `glimpsectl status` shows which glimpse components are running.

## Read the logs

Each part of glimpse is a systemd user service, and its log is in the journal:

```sh
journalctl --user -u glimpse-panel.service
```

The other units follow the same pattern: `glimpse-wallpaper.service`, `glimpse-lock.service`,
`glimpse-sunset.service` (the night light), `glimpse-notifications.service`,
`glimpse-idle.service`, `glimpse-weather.service`, `glimpse-settings-daemon.service`,
`glimpse-desktop.service` (sets the session identity) and `glimpse-shot-portal.service` (screenshots
requested by apps). Weather and the screenshot portal start when an app requests them.

## The panel does not appear

glimpse's services hang off `glimpse-session.target`, which, once enabled, starts with
`graphical-session.target`. If it was never enabled, or your compositor never starts that target,
nothing comes up. Start glimpse by hand and check the
panel's status:

```sh
systemctl --user start glimpse-session.target
systemctl --user status glimpse-panel.service
```

If the bar appears and your session reaches `graphical-session.target`, enable glimpse once with
`systemctl --user enable --now glimpse-session.target` and it starts with every graphical session. If
your session does not reach that target, start and stop glimpse with your compositor as shown in
[Start the shell](/start/install#start-the-shell).

## Restart one part

Saving the config reloads it, but to restart a component outright:

```sh
systemctl --user restart glimpse-panel.service
```

Start `glimpse-session.target` before restarting weather, idle, notifications, settings or the
screenshot portal; those services refuse to start while the target is stopped.

Never restart or stop `glimpse-lock.service` while the session is locked. Stopping or restarting
`glimpse-session.target` leaves the lock screen running; it belongs to `graphical-session.target`.

## A setting has no effect, or the config is broken

```sh
glimpsectl config validate
```

Validation reports the key that is wrong, plus the file and line for a syntax error.
`glimpsectl config path` lists the files in the order they apply: `config.toml` and `panel.toml` whether or not they exist, `panel.d` files only when they do.
Unknown keys are refused, so a misspelled key fails
validation instead of being silently ignored. Only the panel reads `panel.toml`, so a mistake there
leaves the other programs alone. If `config.toml` fails to load at startup, idle
timers and the night light stay off until it is fixed and everything else uses its defaults. A broken
save made while glimpse runs is not applied, and the previous configuration keeps running. Either way
the panel posts one notification.

## The lock screen rejects the right password

If the lock screen shows "Can't verify passwords" and hides the password field, switch to a text
console with <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>F2</kbd>, log in and run:

```sh
glimpse-lock check
```

It prints one line per check and exits with status 5 if any fails. Failures identify a missing
service, a user namespace, `NoNewPrivs` blocking privilege changes, a capability bounding set or
securebits that leave the helper without the capability Fedora's `/etc/shadow` needs, or a
`unix_chkpwd` helper without setuid or setgid permissions or on a `nosuid` filesystem. Do not
sandbox the lock screen service; that can prevent it from checking passwords.

The lock screen authenticates through PAM. The service name is `[lock] pam-service` (default
`glimpse-lock`), read from `/etc/pam.d/<name>` or `/usr/lib/pam.d/<name>`. A missing PAM file, a
service name containing `/`, or a service containing `pam_permit.so` fails the check. A service
that accepts any password is refused on purpose. Changing `pam-service` takes effect after the lock
screen restarts; log out and back in to apply it.

Only local password authentication unlocks the screen; logind's `Unlock` signal does not. Restarting
the lock screen does not recover access: it locks again if the session is already marked locked,
and the compositor keeps the session locked if the lock screen process dies.
