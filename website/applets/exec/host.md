# Notify, copy, open links and launch apps

Use SDK helpers from event handlers to integrate an applet with the desktop. These functions return `void`. `notify` runs `notify-send` and `copy` runs `wl-copy` from the applet process, so install libnotify and wl-clipboard. For media players and workspaces, use MPRIS over D-Bus or the compositor's IPC directly.

## Add host actions

This complete `index.tsx` adds actions to a generated applet project:

```tsx
import {
  ActionRow, closePopover, copy, Indicator, launch, notify, openUri,
  Popover, run, Section,
} from "glimpse-sdk";

function TodoActions() {
  return (
    <>
      <Indicator icon="view-list-symbolic" tooltip="Todo actions" />
      <Popover title="Todo actions">
        <Section>
          <ActionRow title="Notify" onActivate={() => notify({
            summary: "Todo saved", body: "Your list is ready.", urgency: "normal",
          })} />
          <ActionRow title="Copy task" onActivate={() => copy("Buy groceries")} />
          <ActionRow title="Open glimpse" onActivate={() => openUri(
            "https://github.com/alex-oleshkevich/glimpse",
          )} />
          <ActionRow title="Open Files" onActivate={() => {
            launch("org.gnome.Nautilus");
            closePopover();
          }} />
        </Section>
      </Popover>
    </>
  );
}

await run(<TodoActions />);
```

Run it with `glimpsectl applet run --dev .`. The launch action requires GNOME Files; use the desktop-file id of another installed application for a different target. `launch` accepts an id with or without `.desktop`, not an arbitrary command line.

## Know the limits

| Request | Limit or behavior |
| --- | --- |
| `notify` | `summary` is required; `body`, `icon`, and `urgency` are optional. The notification daemon applies its own limits. The icon defaults to the applet's desktop-entry icon. |
| `copy` | Copies text with `wl-copy`. |
| `openUri` | At most 2,048 bytes. Only `http`, `https`, `mailto`, and `file` schemes are accepted. |
| `openUri` with `file` | Opens a resolved directory or an eligible regular file. Executable files, desktop entries, scripts, binaries, installers, and archives that run are refused. |
| `launch` | Resolves an installed desktop-file id. An empty id is refused. |
| `closePopover` | Closes this applet's popover. |

The host delays lines beyond 120 per second. Send requests in response to an action, rather than during every render.
