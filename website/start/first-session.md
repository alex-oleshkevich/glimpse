# Your first session

In about ten minutes you will find your way around the panel, change how the clock reads, switch
the desktop to dark mode and back, and lock the screen. By the end you will know where every
setting lives and how changes take effect.

Before you start, [install glimpse](./install) and start `glimpse-session.target`. The panel should
be along the top of your screen.

## Look at the panel

The panel has three zones. On the left are your workspaces and the windows on them, followed by
bookmarked places and whatever media is playing. The clock, weather and notifications sit in the
middle. The right side holds the tray and the system applets: network, Bluetooth, sound, power and
the session menu.

Click the clock. A popover opens with a calendar; once you configure a calendar source under
`[calendar]` it also lists your upcoming events (see [Information applets](/config/applets/information)).
Click the clock again or anywhere outside the popover to close it.

Now click the speaker icon on the right. You get output and input volume sliders, a row for the
current device and a volume slider per app. Scroll on
the speaker icon itself and notice that the volume changes without opening anything. Several applets
work this way: a click opens the controls, a scroll adjusts the obvious thing.

## Open the configuration file

The first time glimpse started, it wrote a configuration file for you. Open it in your editor:

```sh
$EDITOR ~/.config/glimpse/config.toml
```

Every setting that has a default is in there, commented out and set to its default. A line that starts with `#` has no
effect, so the file currently changes nothing.

## Change the clock

Find the clock section. It looks like this:

```toml
# [applets.clock]
# extends = "clock"
# label-format = "%a, %-d %b, %H:%M"
```

Remove the `#` from the first and third lines, and change the format to show only the time:

```toml
[applets.clock]
# extends = "clock"
label-format = "%H:%M"
```

Save the file. Look at the panel: the clock now shows only hours and minutes. You didn't restart
anything. glimpse watches the file and applies each change when you save.

Put the date back by changing the format to `"%a, %-d %b, %H:%M"` and saving again. The clock follows
within a moment.

## Catch a mistake

Change the line to something glimpse cannot read, for example by deleting the closing quote:

```toml
label-format = "%H:%M
```

Save. A notification tells you the configuration has an error, and the panel keeps running with the
last version that worked. Ask glimpse what is wrong:

```sh
glimpsectl config validate
```

For a syntax error like this one, it names the file, the line, the column and the problem. Put the quote back and save.

## Switch to dark mode

The default `[appearance]` settings are `desktop-color-scheme = "sun"` and
`color-scheme = "dark"`. With a known location, your desktop turns dark at sunset and light at
sunrise, and glimpse stays dark. You can hold your desktop in one mode yourself:

```sh
glimpsectl appearance set dark
```

You need `glimpse-settings-daemon` and `dconf` running to change the desktop preference. Apps that
follow it turn dark. Your panel follows it through `xdg-desktop-portal` and a settings backend such
as `xdg-desktop-portal-gtk`; without a running portal, the panel keeps its current look.

Return to your configured desktop policy:

```sh
glimpsectl appearance resume
```

Sunset and sunrise are computed from your location. If glimpse has none, `resume` fails with
`no solar phase yet` and the desktop keeps the mode you set. Set `provider = "manual"` with a
`latitude` and `longitude` under [`[geolocation]`](/config/services/appearance), or run GeoClue so
glimpse can find your location.

## Lock the screen

Run:

```sh
loginctl lock-session
```

This is the standard systemd command for locking a session, so anything that already uses it, such
as a key binding or a script, locks glimpse too.

Every monitor shows the lock screen with the time and your name. Type your password and press
<kbd>Enter</kbd>. The lock screen checks it the same way your login does, then lets you back in.

You can also lock from the session menu at the right end of the panel.

## What you learned

You have used the panel's popovers and scroll shortcuts, edited `config.toml` and watched the change
apply on save, checked the file with `glimpsectl config validate`, switched modes with
`glimpsectl appearance`, and locked the screen.

From here:

- [Use glimpse](/guide/) collects the guides, from [arranging the bar](/guide/panels) to
  [binding keys](/guide/keys).
- [Troubleshooting](/guide/troubleshooting) helps when something does not work.
- [Upgrade and uninstall](./upgrade) explains how to keep glimpse current.
