# Notify, copy, open links and launch apps

Use SDK host requests from event handlers to integrate an applet with the desktop. These functions send a request and return `void`.

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

Run it with `glimpsectl applets run --dev .`. The launch action requires GNOME Files; use the desktop-file id of another installed application for a different target. `launch` accepts an id with or without `.desktop`, not an arbitrary command line.

## Keep requests within host limits

| Request | Limit or behavior |
| --- | --- |
| `notify` | At most one per second per slot. Extra notifications are dropped. `summary` is required; `body`, `icon`, and `urgency` are optional. |
| `copy` | Copies text. The payload is limited to 1 MiB, and the whole encoded protocol line must also fit within 1 MiB. |
| `openUri` | At most 2,048 bytes. Only `http`, `https`, `mailto`, and `file` schemes are accepted. |
| `openUri` with `file` | Opens a resolved directory or an eligible regular file. Executable files, desktop entries, scripts, binaries, installers, and archives that run are refused. |
| `launch` | Resolves an installed desktop-file id. An empty id is refused. |
| `closePopover` | Closes this applet's popover. |

The host delays lines beyond 120 per second. This is separate from the notification cap and the ten-per-second combined cap on [media and compositor requests](/applets/exec/media). Send requests in response to an action, rather than during every render.
