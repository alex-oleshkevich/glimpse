# Live testing

## Never test against the live configuration

**`direnv` exports `GLIMPSE_CONFIG_PATH` in this repo, and it beats a scratch `HOME`.** `.envrc`
sets `GLIMPSE_CONFIG_PATH=var/config/config.toml` along with `GLIMPSE_PANEL_APP_ID` and
`GLIMPSE_WALLPAPER_APP_ID`, so a binary launched from inside the working tree silently loads the
dev document even when `HOME` points at a scratch directory. Override it explicitly, and read the
`load config path=` line in the log before believing anything on screen.

**Give every test panel its own `GLIMPSE_PANEL_APP_ID`, and never reuse one.** The application ID
is a unique name on the session bus, so a second panel claiming an ID the previous run has not
finished releasing hands off to that instance instead: it logs `load config path=`, never reaches
`initializing app`, maps no window, and sits there looking like a hang. Measured — the same trap the
preview host answers with `ApplicationFlags::NON_UNIQUE`. A fresh ID per run costs nothing.

**`pkill -x glimpse-panel` kills the SESSION panel.** `-x` is the right answer to `-f` matching its
own shell, but the session binary is also called `glimpse-panel`, so an exact-name kill takes the
user's bar down with the test's. Kill by the pid you started, and if the session panel does go,
`systemctl --user start glimpse-panel.service` brings it back.

`~/.config/glimpse/config.toml` is the user's own and a binary started without `--config` both reads
and watches it.

```bash
glimpse-panel --config "$SCRATCH/config.toml"     # replaces the whole stack, drop-ins included
HOME="$SCRATCH/home" glimpse-panel                # a fake home, when drop-ins are under test
```

- `--config` is the default choice and enough for anything that is one document. It cannot exercise
  layering, because an explicit path replaces the stack rather than joining it. A test needing
  `config.d/` sets `HOME` (or `XDG_CONFIG_HOME`); one needing the `/etc/glimpse` layer builds it
  through `load_from`, which takes the system directory as an argument for exactly this reason.
- Themes redirect separately, because `theme_dir_for` resolves through `user_dir()` rather than the
  config stack: `GLIMPSE_THEMES_DIR` replaces both roots for loading and watching, `GLIMPSE_THEME`
  overrides the selected name.
- **Send the log somewhere else.** `--config` watches that file's parent directory, so redirecting
  output into it makes every line an event that reloads the configuration — with a document that
  will not parse, a closed loop running at exactly `DEBOUNCE` that looks like a retrying watcher.

## Testing against a compositor

`crates/glimpse-compositors/tests/live.rs` runs against whatever the environment names. Point it
elsewhere with `NIRI_SOCKET` or `HYPRLAND_INSTANCE_SIGNATURE` (unsetting the other): spawn
`niri -c <config>` or `Hyprland -c <config>`, diff `$XDG_RUNTIME_DIR` before and after to find the
socket, export it. Neither compositor has a headless mode, so a nested instance always opens a
window — do mutating tests there. A Unix socket path is capped near 108 bytes, so a daemon under
test gets its socket in `$XDG_RUNTIME_DIR`, never in a long scratch path.

**Urgency is set directly under niri and cannot be under Hyprland.** `niri msg action
set-window-urgent --id <id>` marks a window and `unset-window-urgent` clears it, which is what
`scripts/urgency-test.sh` wraps. Hyprland has no such dispatcher — urgency only arrives from an
`xdg_activation_v1` request it declines, so it needs a real application asking for attention. A GTK
window calling `present()` produces none: on Wayland GTK sends an activation request only when it
already holds a token.
