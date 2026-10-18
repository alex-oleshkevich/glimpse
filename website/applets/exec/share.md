# Install and share an applet

Install an applet's desktop entry so the panel can discover it by id, then place that id in a panel zone. This guide uses the project from [the todo tutorial](/applets/exec/).

## Prepare the project

Keep the project in a permanent directory. From that directory, install dependencies and check it:

```sh
npm install
npm run check
just dev
```

`just dev` builds the applet and runs `glimpsectl applet run --dev .`, which loads the desktop entry and local configuration and restarts the applet on source edits. The panel validates each tree against its nesting and size limits and logs rejections.

The generated desktop entry contains:

```ini
[Desktop Entry]
Type=Application
Version=1.5
Name=Todo
Comment=A list of things to do
Icon=view-list-symbolic
Exec=node --run start
NoDisplay=true
Implements=me.aresa.Glimpse.Applet1
```

`Implements` makes it discoverable as an applet. The start script bundles `index.tsx` into `build/applet.mjs` and runs it. Without `Path`, the host uses the desktop file's resolved directory as the working directory.

## Install the desktop entry

From the project directory, install the entry into your user applications directory:

```sh
glimpsectl applet install
```

It copies the entry to `${XDG_DATA_HOME:-$HOME/.local/share}/applications/`, sets its `Path` key to the project directory so the applet keeps running from there, and refuses to overwrite an entry that is already there. `glimpsectl applet uninstall me.aresa.GlimpseTodo` removes it again, and removes only entries it installed. The host discovers applets through the desktop application catalog; `NoDisplay=true` keeps this entry out of application menus without preventing applet discovery.

Check discovery:

```sh
glimpsectl applet list --installed
glimpsectl applet inspect --installed me.aresa.GlimpseTodo
```

## Place it and pass options

Add `"me.aresa.GlimpseTodo"` to an existing panel zone in your glimpse configuration. For example, an existing `right` list becomes:

```toml
right = ["me.aresa.GlimpseTodo", "network", "audio"]
```

Add a top-level table for the options the applet understands:

```toml
[applets."me.aresa.GlimpseTodo"]
options = { title = "Groceries" }
```

Save the configuration. The panel starts the installed applet, and the todo chip reads "Groceries". A second instance can use its own configured name and options; see [exec settings](/config/applets/custom#exec).

The panel runs the applet as a systemd user unit with a 256 MiB memory limit. If the process exits with an error, it restarts after 1 second, backing off to at most 60 seconds between attempts.

## Share a package

Build a package from the project directory:

```sh
just package
```

`just package` runs `just build`, which assembles the production files into `dist/`, and writes `<id>.glimpse.zip` holding only those files and a generated `metadata.toml` that lists each with its sha256. `just install` builds the package and installs it in one step.

Recipients install the file with:

```sh
glimpsectl applet install me.aresa.GlimpseTodo.glimpse.zip
```

It checks the package against its own metadata, copies the files into the applet's own directory, writes the desktop entry, and prints a config suggestion. Recipients place the id in their panel as above. Pass `--out PATH` to `glimpsectl applet package` to write the package elsewhere.

Exec applets have your full session access, so install applets whose code you trust. The [desktop entry reference](/applets/exec/reference/desktop-entry) lists the keys the host reads and the entries it refuses.
