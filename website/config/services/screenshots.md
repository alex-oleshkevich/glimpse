# Screenshots

`[screenshot]` sets where `glimpse-shot` saves captures and how its editor starts.

`directory` and `filename` supply the default save path for every capture. Use an absolute
directory: `~` is not expanded, and relative paths start from the directory where you launch
`glimpse-shot`. Filename patterns use local time. If a generated name already exists, a new
capture gets ` (2)` before `.png`, then ` (3)`, instead of overwriting it.

`glimpse-shot --screen`, `--window` and `--region` read the configured `delay`, `flash` and
`notify`. Flash needs desktop animations enabled. Notifications stay off with `editor = "window"`.
The app's Take Screenshot button uses its own Delay row, initially set from `delay`, and reads
`flash` but keeps notifications off. `glimpse-shot capture` waits only for `--delay`; it never
flashes or notifies.

<ConfigTable path="screenshot" />

The preferences window saves only the keys you change to the `[screenshot]` table of the top writable `config.toml`:
the one in your `--config-dir` when you pass one, otherwise `~/.config/glimpse/config.toml`. It shows
the effective values from every configuration directory and is read-only when that file is not writable. Other tables are preserved when preferences are saved.

The preferences rows correspond to these keys:

| Preferences row | Key |
| --- | --- |
| Folder | `directory` |
| File Name | `filename` |
| Annotate In | `editor` |
| Default Delay | `delay` |
| Show Pointer | `cursor` |
| Flash on Capture | `flash` |
| Notify After Saving or Copying | `notify` |
| Color | `color` |
| Line Width | `stroke` (`thin`, `medium`, `thick`) |
