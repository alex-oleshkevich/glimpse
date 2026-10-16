# Tool applets

These applets keep a history of what you copied, picked or measured.

Each applet also has a table named after its service. The service table applies everywhere; the
applet table applies to that applet only.

## clipboard

<ConfigTable path="applets.clipboard" describe />

<AppletShot name="clipboard" />

Click the chip to open the popover. Clicking an entry copies it. Each entry also has Pin and Forget.

Clear history keeps pins. Turning `enabled` off clears everything, pins included.

<ConfigTable path="clipboard" describe />

For scripts, `glimpsectl clipboard list --json` lists the entries. Entry IDs change when the panel
restarts. See the [clipboard commands](/config/cli/glimpsectl#glimpsectl-clipboard) for the rest.

## color-picker

Left-click opens the palette. Right-click starts a pick right away. Picking needs
`xdg-desktop-portal` with an Access backend such as `xdg-desktop-portal-gtk`.

<ConfigTable path="applets.color-picker" describe />

<AppletShot name="color-picker" />

<ConfigTable path="color-picker" describe />

## ruler

Left-click opens the history. Right-click starts measuring right away. It needs `glimpse-lens` on
your `PATH`.

<ConfigTable path="applets.ruler" describe />

<AppletShot name="ruler" />

<ConfigTable path="ruler" describe />
