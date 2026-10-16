# Develop with live reload

Attach an applet project to your running panel and apply changes as you save. Start with a project created by [the todo tutorial](/applets/exec/).

## Start the development run

From the directory containing one `.desktop` file and `config.toml`, run:

```sh
npm install
glimpsectl applet run --dev .
```

The applet appears in the running panel. If a configured applet on that bar already resolves to the same desktop id, development takes over that slot. Keep the terminal open to see stderr and build errors.

## Change options without restarting

For an applet that reads `options.title`, set its local `config.toml` to:

```toml
options = { title = "Groceries" }
```

Save it. The CLI prints `options updated`, and `useOptions()` receives the new object without restarting the process. This is the project's local configuration, separate from your panel configuration.

## Change source and restart

Edit `index.tsx` and save. The CLI prints the changed file and restarts the applet. The generated project's start script rebuilds it before launching. Changes under hidden paths, `node_modules`, `build`, and `dist` do not trigger source restarts.

Changing `env` in the local configuration also restarts the applet. If the applet stops after an error, fix the source and save, or type `r` followed by Enter in the development terminal to restart it.

Press Ctrl-C to detach the development run. See [the CLI reference](/config/cli/glimpsectl#glimpsectl-applet) for panel, output, zone, environment, and alternate configuration options.
