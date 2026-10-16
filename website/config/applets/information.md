# Information applets

These applets show the time, your calendar, the weather, what is playing and what has happened.

Some applets also have a table named after the service behind them, shown after the applet's own.
The service table changes the service for everything that uses it; the
applet table changes only that applet.

## clock

<ConfigTable path="applets.clock" describe />

<AppletShot name="clock" />

<ConfigTable path="calendar" describe />

## next-event

<ConfigTable path="applets.next-event" describe />

<AppletShot name="next-event" />

## weather

<ConfigTable path="applets.weather" describe />

`label` sets the name in the applet's tooltip and at the top of its popover; the bar itself shows
only the icon and the temperature. With no `label`, a `latlon` place reads as the nearest city,
such as `Amsterdam, NL`. The hour strip shows at most 23 hours however high `hours` is set, and
the day list never goes past `[weather] forecast-days` rows counting today (7 by default).

<AppletShot name="weather" />

<ConfigTable path="weather" describe />

## mpris

<ConfigTable path="applets.mpris" describe />

<AppletShot name="mpris" />

<ConfigTable path="mpris" describe />

## notifications

<ConfigTable path="applets.notifications" describe />

<AppletShot name="notifications" />

## system-monitor

<ConfigTable path="applets.system-monitor" describe />

<AppletShot name="system-monitor" />

<ConfigTable path="system-monitor" describe />

## kdeconnect

<ConfigTable path="applets.kdeconnect" describe />

<AppletShot name="kdeconnect" />
