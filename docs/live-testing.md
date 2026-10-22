# Live testing

## Never test against the live configuration

**`direnv` exports `GLIMPSE_CONFIG_DIR` in this repo, and it stacks above the user directory
instead of replacing it.** `.envrc` sets `GLIMPSE_CONFIG_DIR=var/config` along with
`GLIMPSE_PANEL_APP_ID` and `GLIMPSE_WALLPAPER_APP_ID`, so a binary launched from inside the working
tree loads the dev documents over `~/.config/glimpse`. Give it a temporary `XDG_CONFIG_HOME` too, and
read the `load config path=` lines in the log before believing anything on screen.

**Give every test panel its own `GLIMPSE_PANEL_APP_ID`, and never reuse one.** The application ID
is a unique name on the session bus, so a second panel claiming an ID the previous run has not
finished releasing hands off to that instance instead: it logs `load config path=`, never reaches
`initializing app`, maps no window, and sits there looking like a hang. Measured — the same trap the
preview host answers with `ApplicationFlags::NON_UNIQUE`. A fresh ID per run costs nothing.

**`pkill -x glimpse-panel` kills the SESSION panel.** `-x` is the right answer to `-f` matching its
own shell, but the session binary is also called `glimpse-panel`, so an exact-name kill takes the
user's bar down with the test's. Kill by the pid you started, and if the session panel does go,
`systemctl --user start glimpse-panel.service` brings it back.

`~/.config/glimpse` is the user's own, and a binary started without `--config-dir` both reads and
watches it.

```bash
XDG_CONFIG_HOME="$SCRATCH/home/.config" glimpse-panel --config-dir "$SCRATCH/cfg"
```

- `--config-dir` is the default choice: its `config.toml` and `panel.toml` stack above everything
  else, and every write (the wallpaper portal, the shot preferences) lands in it. The temporary
  `XDG_CONFIG_HOME` keeps the user's files out of the stack; it also hosts a stack under test, such
  as a `panel.d/` drop-in. `/etc/glimpse` cannot be redirected this way; a test that must exclude it
  loads explicit paths through `load_files`.
- Themes resolve through the same roots, so a `themes/` directory in `--config-dir` is found first.
  `GLIMPSE_THEMES_DIR` replaces every theme root for loading and watching, `GLIMPSE_THEME` overrides
  the selected name.
- **Send the log somewhere else.** The watch covers the `--config-dir` root, so redirecting output
  into it makes every line an event and a reread of the configuration.

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
