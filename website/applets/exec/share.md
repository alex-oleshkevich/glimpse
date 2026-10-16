# Install and share an applet

Install an applet's desktop entry so the panel can discover it by id, then place that id in a panel zone. This guide uses the project from [the todo tutorial](/applets/exec/).

## Prepare the project

Keep the project in a permanent directory. From that directory, install dependencies and check it:

```sh
npm install
npm run check
```

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

## Share the source

Share the source, `package.json`, `tsconfig.json`, the desktop entry, and local `config.toml`. Recipients install dependencies, install the entry on their machine, and place its id in their panel. Avoid distributing an absolute `Path` from your machine; use a link or have recipients set their own path.

Exec applets run as systemd user units with the user's full session access, so install applets whose code you trust. The [desktop entry reference](/applets/exec/reference/desktop-entry) lists the keys the host reads and the entries it refuses.
