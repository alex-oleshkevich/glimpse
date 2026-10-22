# Applets

An applet is one item on a panel. You place applets by name in a panel's `left`, `center` and
`right` lists, and change how one behaves in a table named after it. Both live in `panel.toml`:

```toml
[applets.clock]
label-format = "%a %-d %b  %H:%M"
week-numbers = true
```

An applet you never mention in `[applets]` runs with its defaults.

## Running an applet twice

To show the same applet twice with different settings, give the second copy its own name and say
which applet it is with `extends`. Then place it by that name:

```toml
[applets.clock-utc]
extends = "clock"
timezone = "UTC"
label-format = "UTC %H:%M"

[[panels]]
center = ["clock", "clock-utc"]
```

A table named after a built-in applet, such as `[applets.clock]`, doesn't need `extends`.

## Tooltips

Every applet takes `tooltip-format`, which sets what its tooltip on the bar says. Most applets
replace tokens in braces with live values. Each applet's section below lists its tokens and the
format it uses when you set none, which is a good starting point to copy:

```toml
[applets.network]
tooltip-format = "{connection}\n{metered}"
```

## System

<AppletList page="/config/applets/system" :names="['audio', 'bluetooth', 'network', 'display', 'power', 'keyboard', 'printing', 'removable', 'privacy', 'idle', 'session']" />

## Workspaces and windows

<AppletList page="/config/applets/workspaces" :names="['pager', 'workspace-name', 'tray', 'places']" />

## Information

<AppletList page="/config/applets/information" :names="['clock', 'next-event', 'weather', 'mpris', 'notifications']" />

## Tools

<AppletList page="/config/applets/tools" :names="['clipboard']" />

## Applets you build

<AppletList page="/config/applets/custom" :names="['command', 'exec']" />

A command applet runs a program when you click or scroll on it. An exec applet is a program that
draws its own indicator and popover. Their settings are on
[Command and exec applets](./custom).
