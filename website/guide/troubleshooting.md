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
`glimpse-idle.service`, `glimpse-weather.service` and `glimpse-settings-daemon.service`.

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
your session does not reach that target, start `glimpse-session.target` from your compositor's startup
commands instead.

## Restart one part

Saving the config reloads it, but to restart a component outright:

```sh
systemctl --user restart glimpse-panel.service
```

The same works for every unit listed above except `glimpse-lock.service`. Never restart or stop the
lock screen while the session is locked.

## A setting has no effect, or the config is broken

```sh
glimpsectl config validate
```

Validation reports the key that is wrong, plus the file and line for a syntax error; `glimpsectl config path` shows which files
glimpse reads, in the order they stack. Unknown keys are refused, so a misspelled key fails
validation instead of being silently ignored. If the file fails to load at startup, idle
timers and the night light stay off until it is fixed and everything else uses its defaults. A broken
save made while glimpse runs is not applied, and the previous configuration keeps running. Either way
the panel posts one notification.

## The lock screen rejects the right password

```sh
glimpse-lock check
```

It diagnoses why a correct password would be rejected. The lock screen authenticates through PAM:
the service name is `[lock] pam-service` (default `glimpse-lock`), read from
`/etc/pam.d/<name>`.
