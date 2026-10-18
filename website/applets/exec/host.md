# Notify, copy, open links and launch apps

Call these from an event handler: a press, a toggle, a submitted entry. Each returns at once and
reports nothing back.

## Send a notification and copy text

`notify` runs `notify-send` and `copy` runs `wl-copy` from your applet's process, so install
libnotify and wl-clipboard. A missing tool prints one line to stderr; the applet keeps running.

::: code-group

```tsx [TypeScript]
import { copy, notify } from "glimpse-sdk";

notify({ summary: "Todo saved", body: "Your list is ready.", urgency: "normal" });
copy("Buy groceries");
```

```python [Python]
ctx.notify("Todo saved", body="Your list is ready.", urgency="normal")
ctx.copy("Buy groceries")
```

```go [Go]
ctx.Notify(sdk.Notification{Summary: "Todo saved", Body: "Your list is ready.", Urgency: "normal"})
ctx.Copy("Buy groceries")
```

```rust [Rust]
ctx.notify("Todo saved", Some("Your list is ready."), None, Some("normal"));
ctx.copy("Buy groceries");
```

:::

The notification uses your desktop entry's name and icon unless you pass an icon. Urgency is `low`,
`normal` or `critical`; anything else is `normal`.

## Open a link or a file

::: code-group

```tsx [TypeScript]
openUri("https://github.com/alex-oleshkevich/glimpse");
```

```python [Python]
ctx.open_uri("https://github.com/alex-oleshkevich/glimpse")
```

```go [Go]
ctx.OpenURI("https://github.com/alex-oleshkevich/glimpse")
```

```rust [Rust]
ctx.open_uri("https://github.com/alex-oleshkevich/glimpse");
```

:::

The panel opens `http`, `https`, `mailto` and `file` URIs up to 2,048 bytes in your default
application. A `file` URI opens a folder or a document; the panel refuses anything that would run a
program, such as a script, a binary, a desktop entry or an installer.

## Launch an app and close the popover

::: code-group

```tsx [TypeScript]
import { ActionRow, launch, usePopover } from "glimpse-sdk";

function Files() {
  const popover = usePopover();
  return (
    <ActionRow
      title="Files"
      onActivate={() => {
        launch("org.gnome.Nautilus");
        popover.close();
      }}
    />
  );
}
```

```python [Python]
ctx.launch("org.gnome.Nautilus")
ctx.close_popover()
```

```go [Go]
ctx.Launch("org.gnome.Nautilus")
ctx.ClosePopover()
```

```rust [Rust]
ctx.launch("org.gnome.Nautilus");
ctx.close_popover();
```

:::

`launch` takes an installed desktop-file id, with or without `.desktop`, not a command line. The
panel starts the app, so it gets focus and keeps running when your applet restarts.
