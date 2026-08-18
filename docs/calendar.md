# Calendar sources

Calendar sources feed the clock popover, date markers, event rows, and the `next_event` panel applet. Configure sources once in `~/.config/glimpse/config.toml`; every calendar surface reads from the same event stream.

## Quick setup

Add one or more sources under `[calendar]`:

```toml
[calendar]
poll_interval = 600

[[calendar.sources]]
id = "personal"
type = "ical"
name = "Personal"
uri = "https://calendar.google.com/calendar/ical/example/basic.ics"
color = "#4285f4"

[[calendar.sources]]
id = "work"
type = "ical"
name = "Work"
uri = "file:///home/alex/.config/glimpse/calendars/work.url"
color = "#e01b24"
poll_interval = 300
```

Every configured source is active. To disable one, remove or comment out its `[[calendar.sources]]` block.

## Calendar URLs and optional URL files

Most calendar providers expose an iCalendar subscription URL. Google calls these **Public address in iCal format** and **Secret address in iCal format**. Outlook calls the subscription URL an **ICS link**. You can put a provider URL directly in `uri`:

```toml
[[calendar.sources]]
id = "work"
type = "ical"
name = "Work"
uri = "https://calendar.google.com/calendar/ical/example/private-example/basic.ics"
color = "#e01b24"
```

Provider calendar URLs behave like read-only access tokens. Anyone with the URL can usually read the feed without signing in. Keep private or secret provider URLs out of shared config files, and reset or unpublish the provider URL if it was exposed.

If you do not want the provider URL in `config.toml`, store the URL in a local sidecar file and point `uri` at that file. The sidecar file is a Glimpse storage convenience; the provider still gives you a URL.

Create a file such as:

```txt
~/.config/glimpse/calendars/work.url
```

Put the provider calendar URL inside the file:

```txt
https://calendar.google.com/calendar/ical/example/private-example/basic.ics
```

Then reference that file from config:

```toml
[[calendar.sources]]
id = "work"
type = "ical"
name = "Work"
uri = "file:///home/alex/.config/glimpse/calendars/work.url"
color = "#e01b24"
```

Do not commit `.url` files that contain private or secret provider URLs.

## Provider ICS links

Glimpse reads iCalendar subscription feeds. Use the provider's `.ics` or iCal subscription URL, not an exported one-time `.ics` download, when you want future event changes to appear after polling.

| Provider term | Link type | Best use |
|---|---|---|
| Google Calendar **Public address in iCal format** | Public iCal URL | Public calendars whose events may be visible to anyone. |
| Google Calendar **Secret address in iCal format** | Secret iCal URL | Personal read-only feeds for your own machine. |
| Outlook **ICS link** | Published iCalendar URL | Outlook.com, Microsoft 365, or Exchange calendars published as read-only feeds. |

### Google Calendar

Google Calendar exposes two relevant iCal URLs from the calendar's settings. Google only lets you edit these settings from a computer.

#### Google public address in iCal format

Use this when the calendar is meant to be public.

1. Open Google Calendar in a browser.
2. Open **Settings**.
3. Under **Settings for my calendars**, select the calendar.
4. Under **Access permissions for events**, enable **Make available to public**.
5. Choose whether public viewers can see event details or only free/busy time.
6. Open **Integrate calendar**.
7. Copy **Public address in iCal format**.
8. Put that URL in `uri`, or store the URL in a `.url` sidecar file and point `uri` at the file.

Google's public iCal address only works when the calendar is public. Sharing changes can take several minutes to propagate.

#### Google secret address in iCal format

Use this for your own private read-only feed.

1. Open Google Calendar in a browser.
2. Open **Settings**.
3. Under **Settings for my calendars**, select the calendar.
4. Open **Integrate calendar**.
5. Copy **Secret address in iCal format**.
6. Put that URL in `uri`, or store the URL in a sidecar file such as `~/.config/glimpse/calendars/personal.url`.
7. If you use a sidecar file, configure the source with `uri = "file:///home/alex/.config/glimpse/calendars/personal.url"`.

Do not share the secret address with other people. If the address was exposed, use Google's **Reset** action for the secret address and update the `.url` file. Google Workspace administrators can disable secret addresses; if the field is missing on a work or school account, contact the Workspace administrator.

### Outlook and Microsoft 365

Outlook exposes read-only calendar feeds through calendar publishing. Microsoft shows both an HTML link and an ICS link; use the ICS link for Glimpse.

1. Open Outlook on the web or new Outlook.
2. Open **Settings**.
3. Open **Calendar**.
4. Open **Shared calendars** or the calendar sharing/publishing section.
5. Under **Publish a calendar**, choose the calendar to expose.
6. Choose the visible detail level.
7. Select **Publish**.
8. Copy the **ICS** link.
9. Put that link in `uri`, or store the link in a `.url` sidecar file and point `uri` at the file.

Published Outlook calendars are viewable by anyone who has the link. To revoke access, unpublish the calendar in Outlook and replace or remove the Glimpse source.

## Local calendar files

Use a directory source when another app already writes `.ics` files locally:

```toml
[[calendar.sources]]
id = "local"
type = "directory"
name = "Local"
uri = "file:///home/alex/.config/glimpse/calendars"
color = "#f6c343"
```

Glimpse reads every `.ics` file directly inside that directory and ignores other files. New, changed, and removed files are picked up on the next poll.

## Source fields

| Field | Default | Meaning |
|---|---|---|
| `[calendar].poll_interval` | `600` | Global refresh interval in seconds. Values below 60 are clamped to 60. |
| `id` | required | Stable source id used for event provenance. |
| `type` | required | `ical` for one feed, `directory` for a folder of `.ics` files. |
| `name` | `id` | Display name shown in tooltips and event metadata. |
| `uri` | required | `https://`, `http://`, or `file://` URI. |
| `color` | unset | Calendar color used for date markers, event-row dots, and `next_event` dots. |
| `poll_interval` | global interval | Contributes to the shared refresh interval (see below) in seconds. Values below 60 are clamped to 60. Does not make this source poll independently of the others. |

The effective refresh interval is the lowest configured interval across `[calendar]` and all sources, with a 60 second floor.

## Source types

| Type | URI shape | Behavior |
|---|---|---|
| `ical` | `https://example.com/calendar.ics` | Downloads one iCalendar feed on each poll. |
| `ical` | `file:///path/to/calendar.ics` | Reads one local iCalendar file on each poll. |
| `ical` | `file:///path/to/work.url` | Reads the file, treats an `http://` or `https://` body as the provider calendar URL, then downloads it. |
| `directory` | `file:///path/to/calendars` | Reads every `.ics` file in the directory on each poll. |

## Event support

Glimpse reads common iCalendar fields including `UID`, `SUMMARY`, `DTSTART`, `DTEND`, `LOCATION`, `DESCRIPTION`, `URL`, `STATUS`, `TRANSP`, `ORGANIZER`, `ATTENDEE`, `LAST-MODIFIED`, and `SEQUENCE`.

Recurring events are expanded from `RRULE`, `RDATE`, and `EXDATE`, with a limit of 2048 occurrences per event. IANA timezones and common Outlook Windows timezone aliases are supported.

Meeting links are detected from `URL` and `DESCRIPTION` for Zoom, Google Meet, and Microsoft Teams. When both a meeting link and an event URL exist, Glimpse keeps both so the UI can offer separate actions.

Duplicate events are merged by normalized title, start time, end time, and all-day state. When two sources contain the same event, the first source in config wins.

## Display rules

| Surface | Events shown |
|---|---|
| Clock popover | Events for the selected date. Ended events stay visible for that day and keep their start time. |
| Date markers | Days with events in the visible month. All-day events are hidden from markers when `[applets.clock].hide_all_day_events = true`. |
| Event rows | Event title, time, source, location, and a colored dot when the source has `color`. Long titles wrap after 40 characters. |
| `next_event` | The next non-all-day event whose end time is after now and whose start time is inside `threshold_minutes`, or an in-progress event. |

The clock popover behaves like a day agenda: past events stay visible while you are looking at that day. The `next_event` applet is stricter because it is an upcoming-event indicator.

### See also

| Document | Purpose |
|---|---|
| [Configuration](./configuration.md) | Main config file layout and a compact calendar example. |
| [Applets](./applets/) | Clock and `next_event` applet display options. |
