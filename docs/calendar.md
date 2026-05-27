# Calendar Sources

Calendar sources feed the clock popover, date markers, event rows, and the `next_event` panel applet. Configure sources once in `~/.config/glimpse/config.toml`; every calendar surface reads from the same event stream.

## Quick Setup

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

## Private URLs

Most calendar providers expose a private iCalendar subscription URL. You can put that URL directly in `uri`, but a sidecar file keeps secrets out of the main config.

Create a file such as:

```txt
~/.config/glimpse/calendars/work.url
```

Put the private calendar URL inside the file:

```txt
https://calendar.example.com/private/basic.ics
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

## Local Calendar Files

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

## Source Fields

| Field | Default | Meaning |
|---|---|---|
| `[calendar].poll_interval` | `600` | Global refresh interval in seconds. Values below 60 are clamped to 60. |
| `id` | required | Stable source id used for event provenance. |
| `type` | required | `ical` for one feed, `directory` for a folder of `.ics` files. |
| `name` | `id` | Display name shown in tooltips and event metadata. |
| `uri` | required | `https://`, `http://`, or `file://` URI. |
| `color` | unset | Calendar color used for date markers, event-row dots, and `next_event` dots. |
| `poll_interval` | global interval | Per-source refresh interval in seconds. Values below 60 are clamped to 60. |

The effective refresh interval is the lowest configured interval across `[calendar]` and all sources, with a 60 second floor.

## Source Types

| Type | URI shape | Behavior |
|---|---|---|
| `ical` | `https://example.com/calendar.ics` | Downloads one iCalendar feed on each poll. |
| `ical` | `file:///path/to/calendar.ics` | Reads one local iCalendar file on each poll. |
| `ical` | `file:///path/to/private.url` | Reads the file, treats an `http://` or `https://` body as the real calendar URL, then downloads it. |
| `directory` | `file:///path/to/calendars` | Reads every `.ics` file in the directory on each poll. |

## Event Support

Glimpse reads common iCalendar fields including `UID`, `SUMMARY`, `DTSTART`, `DTEND`, `LOCATION`, `DESCRIPTION`, `URL`, `STATUS`, `TRANSP`, `ORGANIZER`, `ATTENDEE`, `LAST-MODIFIED`, and `SEQUENCE`.

Recurring events are expanded from `RRULE`, `RDATE`, and `EXDATE`, with a limit of 2048 occurrences per event. IANA timezones and common Outlook Windows timezone aliases are supported.

Meeting links are detected from `URL` and `DESCRIPTION` for Zoom, Google Meet, and Microsoft Teams. When both a meeting link and an event URL exist, Glimpse keeps both so the UI can offer separate actions.

Duplicate events are merged by normalized title, start time, end time, and all-day state. When two sources contain the same event, the first source in config wins.

## Display Rules

| Surface | Events shown |
|---|---|
| Clock popover | Events for the selected date. Ended events stay visible for that day and keep their start time. |
| Date markers | Days with events in the visible month. All-day events are hidden from markers when `[applets.clock].hide_all_day_events = true`. |
| Event rows | Event title, time, source, location, and a colored dot when the source has `color`. Long titles wrap after 40 characters. |
| `next_event` | The next non-all-day event whose end time is after now and whose start time is inside `threshold_minutes`, or an in-progress event. |

The clock popover behaves like a day agenda: past events stay visible while you are looking at that day. The `next_event` applet is stricter because it is an upcoming-event indicator.

### See Also

| Document | Purpose |
|---|---|
| [Configuration](./configuration.md) | Main config file layout and a compact calendar example. |
| [Applets](./applets/) | Clock and `next_event` applet display options. |
