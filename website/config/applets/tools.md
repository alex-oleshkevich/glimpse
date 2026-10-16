# Tool applets

These applets keep a history of what you copied, picked or measured.

Some applets also have a table named after the service behind them, shown after the applet's own.
The service table changes the service for everything that uses it; the
applet table changes only that applet.

## clipboard

<ConfigTable path="applets.clipboard" describe />

<AppletShot name="clipboard" />

Click the chip to open the popover:

- Search matches the text preview without regard to case and hides images while you type.
- Pinned entries appear above Recent. Use the more row or Show fewer to expand or shrink the list.
  Clicking an entry copies it and closes the popover; its chevron offers Copy, Pin and Forget.
- Colors offer RGB and HSL copies, links and paths offer Open, and images offer Save as and
  Open with. Send to device appears when a paired KDE Connect device accepts shares.

Clear history keeps pins. Saving a smaller `limit` or `max-total-kib` trims history immediately;
pins do not count toward `limit`. Setting `enabled = false` clears everything, including pins.
Turning it back on records the current clipboard. Turning `capture-images` off leaves existing
images in history.

<ConfigTable path="clipboard" describe />

For scripts, start with `glimpsectl clipboard list --json`: entry IDs change when the panel
restarts. Each row has `id`, `kind`, `mime`, `pinned`, `copied_at_secs`, `size` and `preview`.
`get <id>` writes the exact bytes; `get <id> --json` returns `id`, `mime` and a `data` byte array.
`copy <id>` restores the original type and moves the entry to the top; `clear` keeps pins.

Clipboard commands exit 3 if the panel is unreachable, 4 for an unknown ID in `get` or `copy`,
and 2 for a non-numeric ID. `pin`, `unpin` and `remove` ignore unknown IDs and exit 0. See the
[clipboard commands](/config/cli/glimpsectl#glimpsectl-clipboard) for the full list.

## color-picker

Left-click opens the palette popover, where Pick a color starts a pick. Right-click starts a pick
directly; another right-click while it is running does nothing. You need `xdg-desktop-portal`
with the glimpse-shot screenshot backend (`glimpse-shot.portal`) and an Access backend such as
`xdg-desktop-portal-gtk`.

<ConfigTable path="applets.color-picker" describe />

<AppletShot name="color-picker" />

<ConfigTable path="color-picker" describe />

## ruler

Left-click opens the history popover, where Measure the screen starts a measurement. Right-click
starts measuring directly; another right-click while it is running does nothing. You need
`glimpse-lens` on the panel's `PATH`.

<ConfigTable path="applets.ruler" describe />

<AppletShot name="ruler" />

<ConfigTable path="ruler" describe />
