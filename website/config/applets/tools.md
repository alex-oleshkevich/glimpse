# Tool applets

The clipboard applet keeps a history of what you copied. Its service table applies everywhere; the
applet table applies to that applet only.

## clipboard

<ConfigTable path="applets.clipboard" describe />

<AppletShot name="clipboard" />

Click the chip to open the popover. Clicking an entry copies it. Each entry also has Pin and Forget.

Clear history keeps pins. Turning `enabled` off clears everything, pins included.

<ConfigTable path="clipboard" describe />

For scripts, `glimpsectl clipboard list --json` lists the entries. Entry IDs change when the panel
restarts. See the [clipboard commands](/config/cli/glimpsectl#glimpsectl-clipboard) for the rest.
