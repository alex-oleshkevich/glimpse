# Move your settings from version 0.15

A `config.toml` written for version 0.15 does not load: glimpse reports it and starts with the
built-in defaults, with the idle timers and the night light off. The prompt on this page lets a
coding agent on your machine convert it for you, check that it loads, and tell you what it could not
carry over. [How configuration works](/config/) describes the format it writes.

## Hand the job to an agent

Open a coding agent that can read files and run commands in your session, such as Claude Code or
Codex, and give it this request:

```text
Migrate my glimpse configuration by following
https://alex-oleshkevich.github.io/glimpse/migrate-0.15.txt
```

If the agent cannot open web pages, paste the prompt itself:

::: details The migration prompt

<<< @/public/migrate-0.15.txt{md}

:::

## What the agent does

It copies `~/.config/glimpse` to `~/.config/glimpse.0.15-backup` before it changes anything, and
writes only inside `~/.config/glimpse`. It folds any `include` files into one document, then splits
it into `config.toml` and `panel.toml`.

It keeps only the settings you changed, so everything else takes the current default. Where a
choice has no single right answer, such as whether glimpse stays dark or follows your desktop, it
asks you. Before it edits your compositor's configuration or your systemd user units, it shows you
the change and waits for your answer. It never stops the lock screen.

It finishes once `glimpsectl config validate` prints nothing, then reports every setting it dropped
and why.

## What you finish yourself

- Exec applets you wrote or installed by hand: rebuild them with
  [the applet SDK](/applets/exec/). The agent removes them from the panel and keeps the old files
  in the backup.
- Your own CSS: the agent lists what it customized. Redo it with
  [theme tokens](/config/theme-tokens) in `~/.config/glimpse/styles.css` or a theme folder, as
  [Write a theme](/guide/theming) describes.
- Key bindings for volume, brightness, media, keyboard layout, Wi-Fi and Bluetooth: glimpsectl has
  no command for these, so the agent leaves them commented out. [Bind glimpse to keys](/guide/keys)
  lists what glimpsectl does offer.

## Check the result

```sh
glimpsectl config validate
glimpsectl doctor
```

A clean `validate` prints nothing. `doctor` checks every part of glimpse and tells you what is
missing.
