# Calendar Sources

Glimpse reads calendar events from configured iCalendar sources. One shared event stream feeds the clock popover, date markers, event rows, and the `next_event` panel applet.

## Configuration

Add sources under `[calendar]` in `~/.config/glimpse/config.toml`. Glimpse has no account-login UI for calendar providers; use an iCalendar subscription URL from Google Calendar, Outlook, or another provider.

```toml
[calendar]
poll_interval = 600

[[calendar.sources]]
id = "personal"
type = "ical"
name = "Personal"
uri = "https://calendar.google.com/calendar/ical/example/basic.ics"
color = "#4285f4"
poll_interval = 300

[[calendar.sources]]
id = "private-link"
type = "ical"
name = "Private Link"
uri = "file:///home/alex/.config/glimpse/calendars/private.url"
color = "#e01b24"

[[calendar.sources]]
id = "local-test-events"
type = "directory"
name = "Local Test Events"
uri = "file:///home/alex/.config/glimpse/calendars/test-events"
color = "#f6c343"
poll_interval = 60
```

Every configured source is active. To disable a calendar, remove or comment out its `[[calendar.sources]]` block.

| Field | Default | Meaning |
|---|---|---|
| `[calendar].poll_interval` | `600` | Global refresh interval in seconds. Values below 60 are clamped to 60. |
| `id` | required | Stable source id used for logging and event provenance. |
| `type` | required | `ical` for one iCalendar feed, `directory` for a local directory of `.ics` files. |
| `name` | `id` | Display name shown in tooltips and event metadata. |
| `uri` | required | `https://`, `http://`, or `file://` URI, depending on source type. |
| `color` | unset | Calendar color used for date markers, event-row dots, and `next_event` dots. |
| `poll_interval` | global interval | Per-source refresh interval in seconds. Values below 60 are clamped to 60. |

The effective refresh interval is the lowest configured interval across `[calendar]` and all sources, with a 60 second floor.

## Source Types

| Type | URI shape | Behavior |
|---|---|---|
| `ical` | `https://example.com/calendar.ics` | Downloads one iCalendar feed on each poll. |
| `ical` | `file:///path/to/calendar.ics` | Reads one local iCalendar file on each poll. |
| `ical` | `file:///path/to/private.url` | Reads the file, treats an `http://` or `https://` body as the real calendar URL, then downloads it. |
| `directory` | `file:///path/to/calendars` | Reads every `.ics` file in the directory on each poll and ignores other files. |

Use a URL sidecar file when you want to keep a private calendar URL outside the main config file. The sidecar file should contain only the calendar URL. Use a directory source when another tool writes `.ics` files locally or when you want a test source for development. New, changed, and removed `.ics` files are picked up on the next poll.

## Event Handling

Glimpse parses `UID`, `SUMMARY`, `DTSTART`, `DTEND`, `LOCATION`, all-day dates, `DESCRIPTION`, `URL`, `STATUS`, `TRANSP`, `ORGANIZER`, `ATTENDEE`, `LAST-MODIFIED`, `SEQUENCE`, IANA timezones, and common Outlook Windows timezone aliases. Recurring events are expanded from `RRULE`, `RDATE`, and `EXDATE`, capped at 2048 occurrences per event.

Meeting links are detected from known online meeting URLs in `URL` or `DESCRIPTION`, including Zoom, Google Meet, and Microsoft Teams links. The original event URL is kept separately so the UI can offer both "join meeting" and "open event" actions when both are available.

Duplicate events are merged by normalized title, start time, end time, and all-day state. When two configured sources contain the same event, the first source in config wins.

## Display Rules

| Surface | Events shown |
|---|---|
| Clock popover | Events for the selected date. Ended events stay visible for that day and keep their start time. |
| Date markers | Days with events in the visible month. All-day events are hidden from markers when `[applets.clock].hide_all_day_events = true`. |
| Event rows | Event title, time, source, location, and a colored dot when the source has `color`. Long titles wrap after 40 characters. |
| `next_event` | The next non-all-day event whose end time is after now and whose start time is inside `threshold_minutes`, or an in-progress event. Click opens rich details and actions for fields present in the event. |

The clock popover follows the GNOME-style model for the current day: past events remain visible in the day list, but the empty state changes to "No more events today" when there are no rows. The `next_event` applet is stricter because it is an upcoming-event indicator.

## Local Test Events

The repository includes a helper for testing the `next_event` applet with a local directory source:

```sh
scripts/calendar-fake-event.py --in 5 --duration 30 --title "Next Event Test"
scripts/calendar-fake-event.py clear
```

By default it writes `next-event.ics` into `~/.config/glimpse/calendars/test-events`. If that directory source is missing from config, the script prints the TOML block to add. Use `--dir` to write into another configured directory source and `--config` when testing a non-default config file.

## Debugging

Calendar source logs are emitted by `glimpse-shell` through the normal Rust tracing setup:

```sh
RUST_LOG=glimpse_core::services::calendar_events=debug cargo run -p glimpse-shell
```

Useful log messages include:

| Message | Meaning |
|---|---|
| `loaded configured calendar source` | A source loaded successfully; the log includes source id and event count. |
| `failed to load configured calendar source` | Fetching, reading, or parsing failed for one source. Other sources still load. |
| `failed to parse iCalendar file` | A file inside a directory source was invalid and skipped. |
| `failed to expand recurring calendar event` | A recurrence rule could not be expanded; Glimpse keeps the base event. |

Run with `RUST_LOG=debug` when you also need surrounding shell and applet logs.

### Debug Checklist

| Symptom | What to check |
|---|---|
| No events from one source | Run with calendar debug logs and look for that source id. Other sources continue loading when one source fails. |
| Local directory events do not appear | Confirm the source uses `type = "directory"`, a `file://` URI, and `.ics` files directly inside that directory. |
| New local event is delayed | Wait for the effective poll interval. The service polls at the lowest configured interval with a 60 second floor. |
| Remote provider events do not appear | Open the `.ics` URL outside Glimpse and confirm it returns iCalendar text. For private URLs, prefer a `file://.../private.url` sidecar. |
| Recurring Outlook event logs a timezone warning | Check whether the feed uses an unsupported Windows timezone id. Supported Outlook aliases are mapped before recurrence expansion. |
| `next_event` is empty | Confirm the event is not all-day, has an end time after now, and starts within `[applets.next_event].threshold_minutes`. |

### See Also

| Document | Purpose |
|---|---|
| [Configuration](./configuration.md) | Main config file layout and a compact calendar example. |
| [Applets](./applets/) | Clock and `next_event` applet display options. |
