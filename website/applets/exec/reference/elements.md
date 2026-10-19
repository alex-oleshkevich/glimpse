# Elements

Every element an applet can draw, what it is for, and its props. [Types](#types) covers how each SDK
names things and the rules every element shares; the [parent table](#parent-table) says which
element may hold which.

## Types

### Names, children and callbacks

Element types are lowercase and props are camelCase; the tables on this page use these names, and
each SDK spells them in its own style. This helper builds a section that holds one row per message,
a toggle row and a button:

::: code-group

```tsx [TypeScript]
const inbox = (messages: Message[]): Node => (
    <Section
        title="Inbox"
        className="dim-label"
    >
        {messages.map((m) => (
            <Row
                key={m.id}
                title={m.subject}
            />
        ))}
        <ToggleRow
            title="Do not disturb"
            active={quiet}
            onToggle={setQuiet}
        />
        <Button onClick={sync}>Sync now</Button>
    </Section>
);
```

```python [Python]
def inbox(messages: list[Message]) -> W:
    return Section(
        [
            Row(
                key=m.id,
                title=m.subject,
            )
            for m in messages
        ],
        ToggleRow(
            title="Do not disturb",
            active=quiet,
            on_toggle=set_quiet,
        ),
        Button(
            "Sync now",
            on_click=sync,
        ),
        title="Inbox",
        class_name="dim-label",
    )
```

```go [Go]
func inbox(messages []Message) sdk.Widget {
	rows := make([]sdk.Widget, 0, len(messages))
	for _, m := range messages {
		rows = append(rows, sdk.Row(sdk.RowProps{
			Key:   m.ID,
			Title: m.Subject,
		}))
	}
	return sdk.Section(
		sdk.SectionProps{
			Title:     "Inbox",
			ClassName: "dim-label",
		},
		rows,
		sdk.ToggleRow(sdk.ToggleRowProps{
			Title:    "Do not disturb",
			Active:   quiet,
			OnToggle: setQuiet,
		}),
		sdk.Button(sdk.ButtonProps{OnClick: sync}, "Sync now"),
	)
}
```

```rust [Rust]
fn inbox(messages: &[Message], quiet: bool) -> Widget<Msg> {
    section()
        .title("Inbox")
        .class_name(ClassName::DimLabel)
        .children(messages.iter().map(|m| {
            row()
                .key(m.id)
                .title(&m.subject)
        }))
        .child(
            toggle_row()
                .title("Do not disturb")
                .active(quiet)
                .on_toggle(Msg::SetQuiet),
        )
        .child(
            button()
                .text("Sync now")
                .on_click(|| Msg::Sync),
        )
        .into()
}
```

:::

- A child is an element or a list of elements. `key` keeps an element's identity when its list
  reorders.
- Where an SDK takes text as a child, text in a `label`, `indicator` or `button` becomes its `text`,
  and text in any other element becomes a `label`.
- An event prop takes a handler, called with the arguments its Type column shows, such as
  `(active: boolean)`.
- A prop left out takes its default. The Go SDK leaves out every zero field except the `value` of
  `sliderrow` and `scale`, so a number prop whose zero is a real value is a pointer:
  `Width: sdk.Ptr(48)`, `Level: sdk.Ptr(0.5)`.

### Controlled inputs

A control shows the value your applet passes. Update that value in the callback; if you do not, the
next render puts the old value back. The controlled props are `togglerow.active`,
`expanderrow.open`, `sliderrow.value`, `entry.value`, `scale.value`, `switch.active`,
`togglebutton.active`, `checkbutton.active`, `dropdown.selected` and `choicelist.selected`.

### Class names

`className` takes one style class from this list:

`dim-label`, `caption`, `heading`, `title-1`, `title-2`, `title-3`, `title-4`, `numeric`,
`accent`, `success`, `warning`, `error`, `flat`, `pill`, `circular`, `column`,
`network-popover__rule`, `bluetooth-popover`, `network-popover`.

Its type is `ClassName` in every SDK. `addon`, `container`, `row`, `actionrow`, `togglerow`,
`meterrow`, `expanderrow`, `end`, `start`, `sliderrow`, `factlist` and `swatch` take no
`className`. There is no applet CSS.

### Severity and tone

| Type | Values | Used by |
| --- | --- | --- |
| `Severity` | `info`, `warning`, `error` | `indicator`, `notice` |
| `RowTone` | `warning`, `error`, `destructive` | the `tone` of rows and `sliderrow` |
| `Tone` | `accent`, `dim`, `success`, `warning`, `error` | `dots`, `calendar` events |

A `Tone` is a named theme color, so it follows the light or dark theme. These and every other enum
prop take one of the values their table lists, in the form the example above uses for `className`.

### Text and values

- Text props (titles, subtitles, labels, values, tooltips, counts, descriptions and placeholders) are
  plain text, never markup. The panel drops control characters and bidi overrides, folds whitespace
  into single spaces, trims the ends, and cuts the text at 256 characters with a trailing `…`. Text
  that ends up empty counts as absent. `entry.value` keeps its spaces and holds up to 4096
  characters.
- An icon is a themed icon name matching `^[A-Za-z0-9_.-]+$`, such as `network-wireless-symbolic`,
  never a path. Any other value is dropped.
- A prop of the wrong type, or an enum string its table does not list, is dropped as if absent.
- An array prop with more than 64 items, at any depth, rejects the whole render. `bars.values`,
  `bars.tooltips` and `sparkline.values` may hold 366.
- A parent with more than 200 children rejects the whole render.

## Parent table

The root holds `indicator` and `popover` elements. The panel shows the first `popover`.

| Parent | Children |
| --- | --- |
| root | `indicator`, `popover` |
| `popover` | at most one `addon`, and any body element |
| `addon` | at most one of `readout`, `switch`, `container`, `swatch` |
| `container` | at most one body element |
| `section` | body elements |
| `revealer` | body elements |
| `stack` | body elements |
| `box` | body elements |
| `row` | at most one of `image`, `readout`, `spinner`, drawn at its end |
| `actionrow` | at most one of `image`, `readout`, `spinner`, drawn at its end, and at most one `start` |
| `meterrow` | at most one of `image`, `readout`, `spinner`, drawn at its end |
| `expanderrow` | at most one `end` and one `start`; drawer children: `actionrow`, `togglerow`, `meterrow`, `sliderrow`, `expanderrow`, `factlist`, `choicelist`, `row` |
| `end` | at most one of `image`, `readout`, `spinner` |
| `start` | at most one of `image`, `button`, `readout`, `spinner`, `dots`, `swatch` |
| `sliderrow` | at most one `start` |
| `togglerow` | nothing |
| anything else | nothing |

Body elements are `section`, `row`, `actionrow`, `togglerow`, `meterrow`, `expanderrow`,
`sliderrow`, `entry`, `placeholder`, `box`, `container`, `label`, `image`, `button`, `switch`,
`scale`, `spinner`, `separator`, `factlist`, `choicelist`, `readout`, `notice`, `calendar`, `dots`,
`dropdown`, `togglebutton`, `checkbutton`, `revealer`, `stack`, `rangebar`, `bars`, `sparkline` and
`unsupported`.

A `section` may sit in a `popover`, `box`, `container`, `revealer` or `stack`, but never anywhere
under another `section`, however deep. A `type` the panel does not know becomes `unsupported`. A
child the table does not list rejects the whole render.

## Elements

### `indicator`

The chip on the bar that shows your applet's state at a glance. A left click opens the popover.

::: code-group

```tsx [TypeScript]
<Indicator
    icon="mail-unread-symbolic"
    badge="3"
    tooltip="3 unread messages"
/>
```

```python [Python]
Indicator(
    icon="mail-unread-symbolic",
    badge="3",
    tooltip="3 unread messages",
)
```

```go [Go]
sdk.Indicator(sdk.IndicatorProps{
	Icon:    "mail-unread-symbolic",
	Badge:   "3",
	Tooltip: "3 unread messages",
})
```

```rust [Rust]
indicator()
    .icon("mail-unread-symbolic")
    .badge("3")
    .tooltip("3 unread messages")
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `icon` | string | | The chip's icon |
| `text` | string, up to 32 characters shown | | A label beside the icon |
| `tooltip` | string | | Text shown on hover |
| `badge` | string, up to 8 characters | | A short count on the chip, such as `3`; it takes the attention dot's place |
| `overlay` | string | | An icon drawn over the corner of `icon` |
| `dot` | CSS color | | A dot in this color, such as `#e01b24` |
| `severity` | `info`, `warning`, `error` | | `warning` and `error` color the icon and label; `info` looks like none |
| `attention` | boolean | `false` | Marks the chip with a dot |
| `notice` | boolean | `false` | A rounded accent background behind the icon and label |
| `className` | class | | A style class from [Class names](#class-names) |
| `onPress` | `(button: number)` | | Called on a click other than the left one, with `2` for middle, `3` for right, or the button's code |
| `onScroll` | `(dx: number, dy: number)` | | Called once per scroll step: `dy` is `-1` up and `1` down, `dx` is `-1` left and `1` right |

### `popover`

The card that opens from the indicator, with an optional header above its body. The body scrolls
once it passes 60% of the monitor's height.

::: code-group

```tsx [TypeScript]
<Popover
    icon="mail-unread-symbolic"
    title="Mail"
    subtitle="3 unread"
>
    <Row
        title="Lunch with Maya"
        subtitle="Maya Lindqvist"
    />
</Popover>
```

```python [Python]
Popover(
    Row(
        title="Lunch with Maya",
        subtitle="Maya Lindqvist",
    ),
    icon="mail-unread-symbolic",
    title="Mail",
    subtitle="3 unread",
)
```

```go [Go]
sdk.Popover(
	sdk.PopoverProps{
		Icon:     "mail-unread-symbolic",
		Title:    "Mail",
		Subtitle: "3 unread",
	},
	sdk.Row(sdk.RowProps{
		Title:    "Lunch with Maya",
		Subtitle: "Maya Lindqvist",
	}),
)
```

```rust [Rust]
popover()
    .icon("mail-unread-symbolic")
    .title("Mail")
    .subtitle("3 unread")
    .child(
        row()
            .title("Lunch with Maya")
            .subtitle("Maya Lindqvist"),
    )
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `title` | string | | The header's title; without it the header is hidden |
| `subtitle` | string | | A line under the title |
| `icon` | string | | The header's icon |
| `size` | `regular`, `wide` | `regular` | The card's width |
| `className` | class | | A style class from [Class names](#class-names) |

### `addon`

One control in the popover header's end, such as a power switch or a large reading. It takes no
props.

::: code-group

```tsx [TypeScript]
<Popover title="Wi-Fi">
    <Addon>
        <Switch
            active={on}
            onToggle={setOn}
        />
    </Addon>
</Popover>
```

```python [Python]
Popover(
    Addon(
        Switch(
            active=on,
            on_toggle=set_on,
        )
    ),
    title="Wi-Fi",
)
```

```go [Go]
sdk.Popover(
	sdk.PopoverProps{Title: "Wi-Fi"},
	sdk.Addon(
		sdk.AddonProps{},
		sdk.Switch(sdk.SwitchProps{
			Active:   on,
			OnToggle: setOn,
		}),
	),
)
```

```rust [Rust]
popover()
    .title("Wi-Fi")
    .child(
        addon().child(
            switch()
                .active(on)
                .on_toggle(Msg::SetOn),
        ),
    )
```

:::

### `container`

A box of a set size and color around one element, for a colored tile or a fixed width.

::: code-group

```tsx [TypeScript]
<Container
    width={48}
    height={48}
    background="blue"
    align="center"
>
    <Label text="21°" />
</Container>
```

```python [Python]
Container(
    Label(text="21°"),
    width=48,
    height=48,
    background="blue",
    align="center",
)
```

```go [Go]
sdk.Container(
	sdk.ContainerProps{
		Width:      sdk.Ptr(48),
		Height:     sdk.Ptr(48),
		Background: "blue",
		Align:      "center",
	},
	sdk.Label(sdk.LabelProps{Text: "21°"}),
)
```

```rust [Rust]
container()
    .width(Some(48))
    .height(Some(48))
    .background(Paint::Blue)
    .align(Align::Center)
    .child(label().text("21°"))
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `width` | number, `-1..1024` | `-1` | The width in pixels; `-1` follows the child |
| `height` | number, `-1..1024` | `-1` | The height in pixels; `-1` follows the child |
| `background` | `red`, `orange`, `yellow`, `green`, `blue`, `purple`, `light`, `dark` | | The fill color |
| `color` | the same values as `background` | | The color of the text inside |
| `align` | `fill`, `start`, `end`, `center`, `baseline` | `fill` | Where the child sits inside the box |

### `section`

A titled group of elements, such as the networks in range.

::: code-group

```tsx [TypeScript]
<Section
    title="Inbox"
    count="3"
>
    <Row title="Lunch with Maya" />
</Section>
```

```python [Python]
Section(
    Row(title="Lunch with Maya"),
    title="Inbox",
    count="3",
)
```

```go [Go]
sdk.Section(
	sdk.SectionProps{
		Title: "Inbox",
		Count: "3",
	},
	sdk.Row(sdk.RowProps{Title: "Lunch with Maya"}),
)
```

```rust [Rust]
section()
    .title("Inbox")
    .count("3")
    .child(row().title("Lunch with Maya"))
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `title` | string | | The heading above the group |
| `count` | string | | Short text beside the title, such as `3` |
| `className` | class | | A style class from [Class names](#class-names) |

### `row`

A line of information that does nothing when clicked: an icon, a title, a subtitle and a value.
`actionrow`, `togglerow`, `meterrow` and `expanderrow` take these props too.

::: code-group

```tsx [TypeScript]
<Row
    icon="network-wireless-symbolic"
    title="Lighthouse"
    value="5 GHz"
/>
```

```python [Python]
Row(
    icon="network-wireless-symbolic",
    title="Lighthouse",
    value="5 GHz",
)
```

```go [Go]
sdk.Row(sdk.RowProps{
	Icon:  "network-wireless-symbolic",
	Title: "Lighthouse",
	Value: "5 GHz",
})
```

```rust [Rust]
row()
    .icon("network-wireless-symbolic")
    .title("Lighthouse")
    .value("5 GHz")
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `title` | string | | The main text |
| `subtitle` | string | | A second line under the title |
| `icon` | string | | An icon at the start |
| `value` | string | | Short text at the end, such as `5 GHz` |
| `tooltip` | string | | Text shown on hover |
| `tone` | `warning`, `error`, `destructive` | | `warning` and `error` color the icon, subtitle and value; `destructive` colors the title red |
| `typography` | `numeric`, `monospace` | | `numeric` gives every digit the same width, so changing figures stay put; `monospace` sets the row in a fixed-width font |
| `inactive` | boolean | `false` | Fades the row |

### `actionrow`

A row the user clicks to do something, such as Connect. It takes the [`row`](#row) props and these:

::: code-group

```tsx [TypeScript]
<ActionRow
    icon="view-refresh-symbolic"
    title="Sync now"
    busy={syncing}
    onActivate={sync}
/>
```

```python [Python]
ActionRow(
    icon="view-refresh-symbolic",
    title="Sync now",
    busy=syncing,
    on_activate=sync,
)
```

```go [Go]
sdk.ActionRow(sdk.ActionRowProps{
	Icon:       "view-refresh-symbolic",
	Title:      "Sync now",
	Busy:       syncing,
	OnActivate: sync,
})
```

```rust [Rust]
action_row()
    .icon("view-refresh-symbolic")
    .title("Sync now")
    .busy(syncing)
    .on_activate(|| Msg::Sync)
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `busy` | boolean | `false` | Shows a spinner at the end in place of the value |
| `inactive` | boolean | `false` | Fades the row and ignores clicks |
| `onActivate` | `()` | | Called when the user clicks the row |

### `togglerow`

A row with a switch or a check box, for a setting the user turns on and off. It takes the
[`row`](#row) props and these:

::: code-group

```tsx [TypeScript]
<ToggleRow
    title="Do not disturb"
    active={quiet}
    onToggle={setQuiet}
/>
```

```python [Python]
ToggleRow(
    title="Do not disturb",
    active=quiet,
    on_toggle=set_quiet,
)
```

```go [Go]
sdk.ToggleRow(sdk.ToggleRowProps{
	Title:    "Do not disturb",
	Active:   quiet,
	OnToggle: setQuiet,
})
```

```rust [Rust]
toggle_row()
    .title("Do not disturb")
    .active(quiet)
    .on_toggle(Msg::SetQuiet)
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `active` | boolean | `false` | Whether the switch or check box is on |
| `control` | `switch`, `check` | `switch` | Draws a switch or a check box |
| `busy` | boolean | `false` | Shows a spinner at the end in place of the value |
| `inactive` | boolean | `false` | Fades the row and ignores input |
| `onToggle` | `(active: boolean)` | | Called when the user flips it, with the new state |

### `meterrow`

A row with a level bar, such as disk use. It takes the [`row`](#row) props and these:

::: code-group

```tsx [TypeScript]
<MeterRow
    icon="drive-harddisk-symbolic"
    title="Home"
    value="72%"
    level={0.72}
/>
```

```python [Python]
MeterRow(
    icon="drive-harddisk-symbolic",
    title="Home",
    value="72%",
    level=0.72,
)
```

```go [Go]
sdk.MeterRow(sdk.MeterRowProps{
	Icon:  "drive-harddisk-symbolic",
	Title: "Home",
	Value: "72%",
	Level: sdk.Ptr(0.72),
})
```

```rust [Rust]
meter_row()
    .icon("drive-harddisk-symbolic")
    .title("Home")
    .value("72%")
    .level(Some(0.72))
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `level` | number, `0..1` | | How full the bar is; without it no bar is drawn |
| `tone` | `warning`, `error`, `destructive` | | Also colors the bar |
| `busy` | boolean | `false` | Shows a spinner at the end in place of the value |

### `expanderrow`

A row that opens a drawer of more rows below it, such as the details of one network. Opening one
drawer closes the others in the popover. It takes the [`row`](#row) props and these:

::: code-group

```tsx [TypeScript]
<ExpanderRow
    title="Lighthouse"
    subtitle="Connected"
    open={expanded}
    onToggle={setExpanded}
>
    <Row
        title="Signal"
        value="82%"
    />
</ExpanderRow>
```

```python [Python]
ExpanderRow(
    Row(
        title="Signal",
        value="82%",
    ),
    title="Lighthouse",
    subtitle="Connected",
    open=expanded,
    on_toggle=set_expanded,
)
```

```go [Go]
sdk.ExpanderRow(
	sdk.ExpanderRowProps{
		Title:    "Lighthouse",
		Subtitle: "Connected",
		Open:     expanded,
		OnToggle: setExpanded,
	},
	sdk.Row(sdk.RowProps{
		Title: "Signal",
		Value: "82%",
	}),
)
```

```rust [Rust]
expander_row()
    .title("Lighthouse")
    .subtitle("Connected")
    .open(expanded)
    .on_toggle(Msg::SetExpanded)
    .child(
        row()
            .title("Signal")
            .value("82%"),
    )
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `open` | boolean | `false` | Whether the drawer is open |
| `titleLines` | number, `1..4` | `1` | How many lines the title may wrap onto |
| `tint` | boolean | `false` | `tone` colors the whole head and the open card, not only its text |
| `busy` | boolean | `false` | Shows a spinner at the end of the head in place of the value |
| `inactive` | boolean | `false` | Fades the head; the drawer still opens |
| `onToggle` | `(open: boolean)` | | Called when the drawer opens or closes, with the new state, also when the panel closes it because another drawer opened |

### `end`

Puts one image, readout or spinner at the end of an expander row's head, such as a device's battery
level. It has no props, and an empty `end` draws nothing.

::: code-group

```tsx [TypeScript]
<ExpanderRow
    icon="audio-headphones-symbolic"
    title="AirPods Pro"
>
    <End>
        <Readout
            value="80"
            unit="%"
        />
    </End>
</ExpanderRow>
```

```python [Python]
ExpanderRow(
    End(
        Readout(
            value="80",
            unit="%",
        )
    ),
    icon="audio-headphones-symbolic",
    title="AirPods Pro",
)
```

```go [Go]
sdk.ExpanderRow(
	sdk.ExpanderRowProps{
		Icon:  "audio-headphones-symbolic",
		Title: "AirPods Pro",
	},
	sdk.End(
		sdk.EndProps{},
		sdk.Readout(sdk.ReadoutProps{
			Value: "80",
			Unit:  "%",
		}),
	),
)
```

```rust [Rust]
expander_row()
    .icon("audio-headphones-symbolic")
    .title("AirPods Pro")
    .child(
        end().child(
            readout()
                .value("80")
                .unit("%"),
        ),
    )
```

:::

### `start`

Puts one control at the start of a row, such as a mute button beside a volume slider. In a slider
row it sits left of the slider; in an action or expander row it takes the place of the icon. It has
no props, and an empty `start` draws nothing.

::: code-group

```tsx [TypeScript]
<SliderRow
    title="Volume"
    value={volume}
    onChange={setVolume}
>
    <Start>
        <Button
            icon="audio-volume-high-symbolic"
            tooltip="Mute"
            onClick={toggleMute}
        />
    </Start>
</SliderRow>
```

```python [Python]
SliderRow(
    Start(
        Button(
            icon="audio-volume-high-symbolic",
            tooltip="Mute",
            on_click=toggle_mute,
        )
    ),
    title="Volume",
    value=volume,
    on_change=set_volume,
)
```

```go [Go]
sdk.SliderRow(
	sdk.SliderRowProps{
		Title:    "Volume",
		Value:    volume,
		OnChange: setVolume,
	},
	sdk.Start(
		sdk.StartProps{},
		sdk.Button(sdk.ButtonProps{
			Icon:    "audio-volume-high-symbolic",
			Tooltip: "Mute",
			OnClick: toggleMute,
		}),
	),
)
```

```rust [Rust]
slider_row()
    .title("Volume")
    .value(volume)
    .on_change(Msg::Volume)
    .child(
        start().child(
            button()
                .icon("audio-volume-high-symbolic")
                .tooltip("Mute")
                .on_click(|| Msg::ToggleMute),
        ),
    )
```

:::

### `sliderrow`

A row with a slider, for a level the user drags, such as volume or brightness. A `start` child adds
a control to its left.

::: code-group

```tsx [TypeScript]
<SliderRow
    title="Brightness"
    value={brightness}
    onChange={setBrightness}
/>
```

```python [Python]
SliderRow(
    title="Brightness",
    value=brightness,
    on_change=set_brightness,
)
```

```go [Go]
sdk.SliderRow(sdk.SliderRowProps{
	Title:    "Brightness",
	Value:    brightness,
	OnChange: setBrightness,
})
```

```rust [Rust]
slider_row()
    .title("Brightness")
    .value(brightness)
    .on_change(Msg::Brightness)
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `value` | `min..max` | `min` | Where the slider's knob sits |
| `min` | number below `max` | `0` | The value at the slider's left end |
| `max` | number above `min` | `1` | The value at the slider's right end |
| `step` | number above `0` | `0.01` | How far one arrow key press moves the knob |
| `title` | string | | Names the slider for screen readers; it is not drawn |
| `subtitle` | string | | A line of text under the slider |
| `tooltip` | string | | Text shown on hover |
| `tone` | `warning`, `error`, `destructive` | | Colors the row to flag the level |
| `inactive` | boolean | `false` | Fades the slider and stops it from moving |
| `onMove` | `(value: number)` | | Called while the user drags, with the current value |
| `onChange` | `(value: number)` | | Called once when a drag ends or a key moves the knob, with the new value |

### `entry`

A one-line text field, for a name, a search or a new item.

::: code-group

```tsx [TypeScript]
<Entry
    placeholder="Add a task"
    value={draft}
    onChange={setDraft}
    onSubmit={addTask}
/>
```

```python [Python]
Entry(
    placeholder="Add a task",
    value=draft,
    on_change=set_draft,
    on_submit=add_task,
)
```

```go [Go]
sdk.Entry(sdk.EntryProps{
	Placeholder: "Add a task",
	Value:       draft,
	OnChange:    setDraft,
	OnSubmit:    addTask,
})
```

```rust [Rust]
entry()
    .placeholder("Add a task")
    .value(draft)
    .on_change(Msg::Draft)
    .on_submit(Msg::AddTask)
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `value` | string, up to 4096 characters, spaces kept | `""` | The text in the field |
| `placeholder` | string | | Hint text shown while the field is empty |
| `className` | class | | A style class from [Class names](#class-names) |
| `onChange` | `(value: string)` | | Called on every edit, with the whole text |
| `onSubmit` | `(value: string)` | | Called when the user presses Enter, with the whole text |

### `placeholder`

An empty state: an icon, a title and a line of explanation, centered, for a list with nothing in it.

::: code-group

```tsx [TypeScript]
<Placeholder
    icon="bluetooth-disabled-symbolic"
    title="No devices"
    description="Paired devices show up here"
/>
```

```python [Python]
Placeholder(
    icon="bluetooth-disabled-symbolic",
    title="No devices",
    description="Paired devices show up here",
)
```

```go [Go]
sdk.Placeholder(sdk.PlaceholderProps{
	Icon:        "bluetooth-disabled-symbolic",
	Title:       "No devices",
	Description: "Paired devices show up here",
})
```

```rust [Rust]
placeholder()
    .icon("bluetooth-disabled-symbolic")
    .title("No devices")
    .description("Paired devices show up here")
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `icon` | string | | An icon above the title |
| `title` | string | | The main line, such as what is missing |
| `description` | string | | A smaller line under the title |
| `className` | class | | A style class from [Class names](#class-names) |

### `box`

Lays out its children in a row or a column, such as a set of buttons side by side.

::: code-group

```tsx [TypeScript]
<Box
    spacing={6}
    halign="center"
>
    <Button
        icon="media-skip-backward-symbolic"
        tooltip="Previous"
        onClick={previous}
    />
    <Button
        icon="media-playback-pause-symbolic"
        tooltip="Pause"
        onClick={pause}
    />
</Box>
```

```python [Python]
Box(
    Button(
        icon="media-skip-backward-symbolic",
        tooltip="Previous",
        on_click=previous,
    ),
    Button(
        icon="media-playback-pause-symbolic",
        tooltip="Pause",
        on_click=pause,
    ),
    spacing=6,
    halign="center",
)
```

```go [Go]
sdk.Box(
	sdk.BoxProps{
		Spacing: 6,
		Halign:  "center",
	},
	sdk.Button(sdk.ButtonProps{
		Icon:    "media-skip-backward-symbolic",
		Tooltip: "Previous",
		OnClick: previous,
	}),
	sdk.Button(sdk.ButtonProps{
		Icon:    "media-playback-pause-symbolic",
		Tooltip: "Pause",
		OnClick: pause,
	}),
)
```

```rust [Rust]
bx().spacing(6)
    .halign(Align::Center)
    .child(
        button()
            .icon("media-skip-backward-symbolic")
            .tooltip("Previous")
            .on_click(|| Msg::Previous),
    )
    .child(
        button()
            .icon("media-playback-pause-symbolic")
            .tooltip("Pause")
            .on_click(|| Msg::Pause),
    )
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `orientation` | `horizontal`, `vertical` | `horizontal` | Lines the children up side by side or one under another |
| `spacing` | `0..24` | `0` | Pixels between children |
| `homogeneous` | boolean | `false` | Gives every child the same size |
| `halign` | `fill`, `start`, `end`, `center`, `baseline` | `fill` | Where the box sits across the width it is given |
| `valign` | `fill`, `start`, `end`, `center`, `baseline` | `fill` | Where the box sits across the height it is given |
| `hexpand` | boolean | `false` | Takes any spare width |
| `vexpand` | boolean | `false` | Takes any spare height |
| `className` | class | | A style class from [Class names](#class-names) |

### `label`

A piece of plain text, such as a caption or a status line. Every label is about 48 characters wide at most.

::: code-group

```tsx [TypeScript]
<Label
    text="Last synced 5 minutes ago"
    className="dim-label"
    wrap
/>
```

```python [Python]
Label(
    text="Last synced 5 minutes ago",
    class_name="dim-label",
    wrap=True,
)
```

```go [Go]
sdk.Label(sdk.LabelProps{
	Text:      "Last synced 5 minutes ago",
	ClassName: "dim-label",
	Wrap:      true,
})
```

```rust [Rust]
label()
    .text("Last synced 5 minutes ago")
    .class_name(ClassName::DimLabel)
    .wrap(true)
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `text` | string | | The words shown, never read as markup |
| `wrap` | boolean | `false` | Breaks long text onto more lines |
| `lines` | `0..8` | `0` | With `wrap`, the most lines shown before the text is cut with `…`; `0` shows every line |
| `ellipsize` | `none`, `start`, `middle`, `end` | `end` | Without `wrap`, where text that does not fit is cut with `…` |
| `xalign` | `0..1` | `0.5` | Horizontal position of the text: `0` left, `0.5` centered, `1` right |
| `className` | class | | A style class from [Class names](#class-names) |

### `image`

A themed icon on its own, such as a weather symbol. It shows an icon from the icon theme, never a
file.

::: code-group

```tsx [TypeScript]
<Image
    icon="weather-clear-symbolic"
    pixelSize={32}
    tooltip="Clear sky"
/>
```

```python [Python]
Image(
    icon="weather-clear-symbolic",
    pixel_size=32,
    tooltip="Clear sky",
)
```

```go [Go]
sdk.Image(sdk.ImageProps{
	Icon:      "weather-clear-symbolic",
	PixelSize: sdk.Ptr(32),
	Tooltip:   "Clear sky",
})
```

```rust [Rust]
image()
    .icon("weather-clear-symbolic")
    .pixel_size(Some(32))
    .tooltip("Clear sky")
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `icon` | string | | The icon name |
| `pixelSize` | `8..64` | the theme's size | The icon's size in pixels |
| `tooltip` | string | | Text shown on hover, also read by screen readers |
| `className` | class | | A style class from [Class names](#class-names) |

### `swatch`

A chip filled with one color, such as a preview of a color temperature.

::: code-group

```tsx [TypeScript]
<Addon>
    <Swatch
        color="#e0563f"
        size="large"
        tooltip="Night light color"
    />
</Addon>
```

```python [Python]
Addon(
    Swatch(
        color="#e0563f",
        size="large",
        tooltip="Night light color",
    )
)
```

```go [Go]
sdk.Addon(
	sdk.AddonProps{},
	sdk.Swatch(sdk.SwatchProps{
		Color:   "#e0563f",
		Size:    "large",
		Tooltip: "Night light color",
	}),
)
```

```rust [Rust]
addon().child(
    swatch()
        .color("#e0563f")
        .size(SwatchSize::Large)
        .tooltip("Night light color"),
)
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `color` | CSS color, up to 256 characters | | The fill, such as `#e0563f`, `rgb(224 86 63)` or `orange`. Without a color, or with one that does not parse, the chip is empty |
| `size` | `regular`, `large` | `regular` | `large` is the bigger chip for the popover header |
| `tooltip` | string | | Text shown on hover |

### `button`

A push button with text, an icon or both, for a single action.

::: code-group

```tsx [TypeScript]
<Button
    icon="view-refresh-symbolic"
    tooltip="Refresh"
    onClick={refresh}
/>
```

```python [Python]
Button(
    icon="view-refresh-symbolic",
    tooltip="Refresh",
    on_click=refresh,
)
```

```go [Go]
sdk.Button(sdk.ButtonProps{
	Icon:    "view-refresh-symbolic",
	Tooltip: "Refresh",
	OnClick: refresh,
})
```

```rust [Rust]
button()
    .icon("view-refresh-symbolic")
    .tooltip("Refresh")
    .on_click(|| Msg::Refresh)
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `text` | string | | The button's label |
| `icon` | string | | An icon, before the text when both are set |
| `tooltip` | string | `text` | Text shown on hover, also read by screen readers; give an icon-only button one |
| `sensitive` | boolean | `true` | `false` grays the button out and ignores clicks |
| `className` | class | | A style class from [Class names](#class-names) |
| `onClick` | `()` | | Called when the user clicks the button |

### `switch`

An on and off switch on its own, such as one in the popover header. In a list, a `togglerow` gives
the switch a title.

::: code-group

```tsx [TypeScript]
<Switch
    active={enabled}
    onToggle={setEnabled}
/>
```

```python [Python]
Switch(
    active=enabled,
    on_toggle=set_enabled,
)
```

```go [Go]
sdk.Switch(sdk.SwitchProps{
	Active:   enabled,
	OnToggle: setEnabled,
})
```

```rust [Rust]
switch()
    .active(enabled)
    .on_toggle(Msg::Enabled)
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `active` | boolean | `false` | Whether the switch is on |
| `sensitive` | boolean | `true` | `false` grays the switch out and ignores clicks |
| `className` | class | | A style class from [Class names](#class-names) |
| `onToggle` | `(active: boolean)` | | Called when the user flips the switch, with the new state |

### `scale`

A bare slider with no title or row around it. In a list, a `sliderrow` gives it a row.

::: code-group

```tsx [TypeScript]
<Scale
    value={opacity}
    min={0}
    max={100}
    step={5}
    onChange={setOpacity}
/>
```

```python [Python]
Scale(
    value=opacity,
    min=0,
    max=100,
    step=5,
    on_change=set_opacity,
)
```

```go [Go]
sdk.Scale(sdk.ScaleProps{
	Value:    opacity,
	Min:      0,
	Max:      100,
	Step:     5,
	OnChange: setOpacity,
})
```

```rust [Rust]
scale()
    .value(opacity)
    .min(0.0)
    .max(100.0)
    .step(5.0)
    .on_change(Msg::Opacity)
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `value` | number, clamped to `min..max` | `0` | Where the slider's knob sits |
| `min` | number below `max` | `0` | The value at the slider's left end |
| `max` | number above `min` | `1` | The value at the slider's right end |
| `step` | number, `0` or more | `0` | How far one arrow key press moves the knob; `0` moves it a hundredth of the range |
| `sensitive` | boolean | `true` | `false` grays the slider out and ignores input |
| `className` | class | | A style class from [Class names](#class-names) |
| `onChange` | `(value: number)` | | Called each time the user moves the knob, with the new value |

### `spinner`

A spinning busy indicator, for work in progress.

::: code-group

```tsx [TypeScript]
<Spinner />
```

```python [Python]
Spinner()
```

```go [Go]
sdk.Spinner(sdk.SpinnerProps{})
```

```rust [Rust]
spinner()
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `className` | class | | A style class from [Class names](#class-names) |

### `factlist`

A list of label and value pairs, such as a connection's details.

::: code-group

```tsx [TypeScript]
<FactList
    facts={[
        { label: "IP address", value: "192.168.1.24" },
        { label: "Signal", value: "Weak", warning: true },
    ]}
/>
```

```python [Python]
FactList(
    facts=[
        {"label": "IP address", "value": "192.168.1.24"},
        {"label": "Signal", "value": "Weak", "warning": True},
    ]
)
```

```go [Go]
sdk.FactList(sdk.FactListProps{Facts: []sdk.Fact{
	{
		Label: "IP address",
		Value: "192.168.1.24",
	},
	{
		Label:   "Signal",
		Value:   "Weak",
		Warning: true,
	},
}})
```

```rust [Rust]
fact_list().facts([
    Fact::new("IP address", "192.168.1.24"),
    Fact::new("Signal", "Weak").warning(true),
])
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `facts` | array of `{label, value, warning?}`, `0..64` items | | One row per fact, the label at the start and the value at the end. `warning` colors that row as a warning |

### `choicelist`

A list of choices with a check mark on the selected one, for picking one option while seeing them
all, such as a power profile.

::: code-group

```tsx [TypeScript]
<ChoiceList
    choices={[
        { label: "Balanced" },
        {
            label: "Power saver",
            detail: "Dims the screen and slows the processor",
        },
    ]}
    selected={profile}
    onSelect={setProfile}
/>
```

```python [Python]
ChoiceList(
    choices=[
        {"label": "Balanced"},
        {
            "label": "Power saver",
            "detail": "Dims the screen and slows the processor",
        },
    ],
    selected=profile,
    on_select=set_profile,
)
```

```go [Go]
sdk.ChoiceList(sdk.ChoiceListProps{
	Choices: []sdk.Choice{
		{Label: "Balanced"},
		{
			Label:  "Power saver",
			Detail: "Dims the screen and slows the processor",
		},
	},
	Selected: sdk.Ptr(profile),
	OnSelect: setProfile,
})
```

```rust [Rust]
choice_list()
    .choices([
        Choice::new("Balanced"),
        Choice::new("Power saver")
            .detail("Dims the screen and slows the processor"),
    ])
    .selected(Some(profile))
    .on_select(Msg::Profile)
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `choices` | array of `{label, detail?, icon?, warning?}`, `0..64` items | | One row per choice: `label` is its title, `detail` a line under it, `icon` an icon before it, and `warning` colors it as a warning |
| `selected` | index | | Which choice is checked; absent or out of range checks none |
| `className` | class | | A style class from [Class names](#class-names) |
| `onSelect` | `(index: number)` | | Called when the user picks a choice, with its index |

### `readout`

A large value with a smaller unit beside it, such as a temperature.

::: code-group

```tsx [TypeScript]
<Readout
    value="21"
    unit="°C"
/>
```

```python [Python]
Readout(
    value="21",
    unit="°C",
)
```

```go [Go]
sdk.Readout(sdk.ReadoutProps{
	Value: "21",
	Unit:  "°C",
})
```

```rust [Rust]
readout()
    .value("21")
    .unit("°C")
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `value` | string | | The number or word in large type |
| `unit` | string | | The smaller text after it |
| `className` | class | | A style class from [Class names](#class-names) |

### `notice`

A highlighted message, such as a warning the user should see first. With `onActivate` it becomes a
button with a chevron.

::: code-group

```tsx [TypeScript]
<Notice
    icon="battery-caution-symbolic"
    title="Battery low"
    subtitle="12% left"
    severity="warning"
    onActivate={openPowerSettings}
/>
```

```python [Python]
Notice(
    icon="battery-caution-symbolic",
    title="Battery low",
    subtitle="12% left",
    severity="warning",
    on_activate=open_power_settings,
)
```

```go [Go]
sdk.Notice(sdk.NoticeProps{
	Icon:       "battery-caution-symbolic",
	Title:      "Battery low",
	Subtitle:   "12% left",
	Severity:   "warning",
	OnActivate: openPowerSettings,
})
```

```rust [Rust]
notice()
    .icon("battery-caution-symbolic")
    .title("Battery low")
    .subtitle("12% left")
    .severity(Severity::Warning)
    .on_activate(|| Msg::OpenPowerSettings)
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `title` | string | | The message |
| `subtitle` | string | | A smaller line under the title |
| `icon` | string | | An icon before the text |
| `severity` | `info`, `warning`, `error` | `info` | Colors the notice; `info` is the plain style |
| `className` | class | | A style class from [Class names](#class-names) |
| `onActivate` | `()` | | Makes the notice clickable, with a chevron, and is called when the user clicks it |

### `calendar`

A month grid the user pages through and picks a day from, with colored dots on the days that have
events.

::: code-group

```tsx [TypeScript]
<Calendar
    year={year}
    month={month}
    today="2026-10-18"
    selected={day}
    events={[{ date: "2026-10-21", colors: ["accent"] }]}
    onMonth={showMonth}
    onSelect={setDay}
/>
```

```python [Python]
Calendar(
    year=year,
    month=month,
    today="2026-10-18",
    selected=day,
    events=[{"date": "2026-10-21", "colors": ["accent"]}],
    on_month=show_month,
    on_select=set_day,
)
```

```go [Go]
sdk.Calendar(sdk.CalendarProps{
	Year:     year,
	Month:    month,
	Today:    "2026-10-18",
	Selected: day,
	Events: []sdk.CalendarEvent{{
		Date:   "2026-10-21",
		Colors: []sdk.Tone{"accent"},
	}},
	OnMonth:  showMonth,
	OnSelect: setDay,
})
```

```rust [Rust]
calendar()
    .year(year)
    .month(month)
    .today("2026-10-18")
    .selected(day)
    .events([CalendarEvent::new("2026-10-21").color(Tone::Accent)])
    .on_month(Msg::Month)
    .on_select(Msg::Day)
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `year` | number, `1..9999` | | With `month`, the month the grid shows |
| `month` | number, `1..12` | | With `year`, the month the grid shows |
| `today` | `YYYY-MM-DD` | | The day marked as today. The grid's today button pages to it and selects it |
| `selected` | `YYYY-MM-DD` | | The highlighted day. Absent, no day is highlighted |
| `events` | array of `{date, colors?}`, first 3 tones shown | | Marks each date with colored dots |
| `className` | class | | A style class for the grid |
| `onMonth` | `(year: number, month: number)` | | Called when the user pages to another month |
| `onSelect` | `(date: string)` | | Called when the user picks a day, with it as `YYYY-MM-DD` |

### `dots`

A short row of colored dots, for a status at a glance, such as which of three devices are
connected.

::: code-group

```tsx [TypeScript]
<Dots colors={["success", "error"]} />
```

```python [Python]
Dots(
    colors=[
        "success",
        "error",
    ]
)
```

```go [Go]
sdk.Dots(sdk.DotsProps{Colors: []sdk.Tone{"success", "error"}})
```

```rust [Rust]
dots().colors([Tone::Success, Tone::Error])
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `colors` | array of `accent`, `dim`, `success`, `warning`, `error` | | One dot per tone, in order |
| `max` | number, `1..3` | `3` | The most dots shown; tones past it are not drawn |
| `size` | number, `2..16` | `4` | Dot diameter in pixels |
| `uniform` | boolean | `false` | Draws every dot in the faded text color instead of its tone |
| `className` | class | | A style class for the dots |

### `dropdown`

A compact button that opens a list and shows the chosen option, for picking one of a few named
choices.

::: code-group

```tsx [TypeScript]
<DropDown
    options={["Celsius", "Fahrenheit"]}
    selected={unit}
    onSelect={setUnit}
/>
```

```python [Python]
DropDown(
    options=[
        "Celsius",
        "Fahrenheit",
    ],
    selected=unit,
    on_select=set_unit,
)
```

```go [Go]
sdk.DropDown(sdk.DropDownProps{
	Options:  []string{"Celsius", "Fahrenheit"},
	Selected: sdk.Ptr(unit),
	OnSelect: setUnit,
})
```

```rust [Rust]
drop_down()
    .options(["Celsius", "Fahrenheit"])
    .selected(Some(unit))
    .on_select(Msg::Unit)
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `options` | array of string | | The choices in the list, in order |
| `selected` | number | | The index of the option shown on the button |
| `className` | class | | A style class for the button |
| `onSelect` | `(index: number)` | | Called when the user picks an option, with its index |

### `togglebutton`

A button that stays pressed in until it is clicked again, for an on and off choice that reads
better as a button than as a switch.

::: code-group

```tsx [TypeScript]
<ToggleButton
    label="Mute"
    active={muted}
    onToggle={setMuted}
/>
```

```python [Python]
ToggleButton(
    label="Mute",
    active=muted,
    on_toggle=set_muted,
)
```

```go [Go]
sdk.ToggleButton(sdk.ToggleButtonProps{
	Label:    "Mute",
	Active:   muted,
	OnToggle: setMuted,
})
```

```rust [Rust]
toggle_button()
    .label("Mute")
    .active(muted)
    .on_toggle(Msg::Mute)
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `label` | string | | The button's text, and its accessible name |
| `icon` | string | | A themed icon on the button; the accessible name when there is no `label` |
| `active` | boolean | `false` | Whether the button is pressed in |
| `className` | class | | A style class for the button |
| `onToggle` | `(active: boolean)` | | Called when the user clicks the button, with the new state |

### `checkbutton`

A check box with a label, for an option the user ticks on or off.

::: code-group

```tsx [TypeScript]
<CheckButton
    label="Show seconds"
    active={seconds}
    onToggle={setSeconds}
/>
```

```python [Python]
CheckButton(
    label="Show seconds",
    active=seconds,
    on_toggle=set_seconds,
)
```

```go [Go]
sdk.CheckButton(sdk.CheckButtonProps{
	Label:    "Show seconds",
	Active:   seconds,
	OnToggle: setSeconds,
})
```

```rust [Rust]
check_button()
    .label("Show seconds")
    .active(seconds)
    .on_toggle(Msg::Seconds)
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `label` | string | | The text beside the box, and its accessible name |
| `active` | boolean | `false` | Whether the box is ticked |
| `className` | class | | A style class for the check box |
| `onToggle` | `(active: boolean)` | | Called when the user ticks or clears the box, with the new state |

### `revealer`

Shows or hides its children with an animation, for content that appears on demand, such as extra
details under a row.

::: code-group

```tsx [TypeScript]
<Revealer revealed={details}>
    <Row
        title="Channel"
        value="36"
    />
</Revealer>
```

```python [Python]
Revealer(
    Row(
        title="Channel",
        value="36",
    ),
    revealed=details,
)
```

```go [Go]
sdk.Revealer(
	sdk.RevealerProps{Revealed: details},
	sdk.Row(sdk.RowProps{
		Title: "Channel",
		Value: "36",
	}),
)
```

```rust [Rust]
revealer()
    .revealed(details)
    .child(
        row()
            .title("Channel")
            .value("36"),
    )
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `revealed` | boolean | `false` | Whether the children are shown |
| `transition` | `slide-down`, `crossfade`, `none` | `slide-down` | The animation that shows and hides them |
| `className` | class | | A style class for the revealer |

### `stack`

Holds several children and shows one at a time, for switching between views in the same space.

::: code-group

```tsx [TypeScript]
<Stack visible={page}>
    <Label text="Today" />
    <Label text="This week" />
</Stack>
```

```python [Python]
Stack(
    Label(text="Today"),
    Label(text="This week"),
    visible=page,
)
```

```go [Go]
sdk.Stack(sdk.StackProps{Visible: page},
	sdk.Label(sdk.LabelProps{Text: "Today"}),
	sdk.Label(sdk.LabelProps{Text: "This week"}),
)
```

```rust [Rust]
stack()
    .visible(page)
    .child(label().text("Today"))
    .child(label().text("This week"))
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `visible` | number | `0` | The index of the child shown. An index with no child keeps the current one |
| `className` | class | | A style class for the stack |

### `rangebar`

A bar that marks a span on a scale, with an optional marker for the current value, such as
today's low and high temperature and the temperature now.

::: code-group

```tsx [TypeScript]
<RangeBar
    min={-5}
    max={25}
    low={8}
    high={17}
    now={12}
/>
```

```python [Python]
RangeBar(
    min=-5,
    max=25,
    low=8,
    high=17,
    now=12,
)
```

```go [Go]
sdk.RangeBar(sdk.RangeBarProps{
	Min:  -5,
	Max:  25,
	Low:  8,
	High: 17,
	Now:  sdk.Ptr(12.0),
})
```

```rust [Rust]
range_bar()
    .min(-5.0)
    .max(25.0)
    .low(8.0)
    .high(17.0)
    .now(Some(12.0))
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `min` | number | `0` | The value at the start of the scale |
| `max` | number above `min`, else the scale is `0..1` | `1` | The value at the end of the scale |
| `low` | number | `0`, clamped to `min..max` | Where the marked span starts |
| `high` | number | `0`, clamped to `low..max` | Where the marked span ends |
| `now` | number, `min..max` | | Draws a marker at this value |
| `className` | class | | A style class for the bar |

### `bars`

A small bar chart, for a history of amounts, such as data used per day.

::: code-group

```tsx [TypeScript]
<Bars
    values={usage}
    label="Data used per day"
/>
```

```python [Python]
Bars(
    values=usage,
    label="Data used per day",
)
```

```go [Go]
sdk.Bars(sdk.BarsProps{
	Values: usage,
	Label:  "Data used per day",
})
```

```rust [Rust]
bars()
    .values(usage)
    .label("Data used per day")
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `values` | array of number or `null`, up to 366 | | One bar per value. A `null` or negative value draws a faint tick. When there are more values than pixels, neighbors merge into one bar showing their largest |
| `tooltips` | array of string or `null`, up to 366 | | The hover text of the bar at the same index |
| `maximum` | number, above `0` | the largest value | The value a full-height bar stands for |
| `highlightLast` | boolean | `true` | Dims every bar but the last |
| `label` | string | | The chart's accessible name |
| `className` | class | | A style class for the chart |

### `sparkline`

A small line chart, for a trend over time, such as processor load.

::: code-group

```tsx [TypeScript]
<Sparkline
    values={load}
    fill
    showLast
    label="Processor load"
/>
```

```python [Python]
Sparkline(
    values=load,
    fill=True,
    show_last=True,
    label="Processor load",
)
```

```go [Go]
sdk.Sparkline(sdk.SparklineProps{
	Values:   load,
	Fill:     true,
	ShowLast: true,
	Label:    "Processor load",
})
```

```rust [Rust]
sparkline()
    .values(load)
    .fill(true)
    .show_last(true)
    .label("Processor load")
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `values` | array of number or `null`, up to 366 | | The points of the line, scaled from the lowest to the highest value. A `null` leaves a gap; a value alone between gaps is a dot |
| `fill` | boolean | `false` | Shades the area under the line |
| `showLast` | boolean | `false` | Puts a dot on the last value |
| `label` | string | | The chart's accessible name |
| `className` | class | | A style class for the chart |

### `separator`

A thin line between groups of content.

::: code-group

```tsx [TypeScript]
<Separator />
```

```python [Python]
Separator()
```

```go [Go]
sdk.Separator(sdk.SeparatorProps{})
```

```rust [Rust]
separator()
```

:::

| Prop | Type | Default | What it does |
| --- | --- | --- | --- |
| `orientation` | `horizontal`, `vertical` | `horizontal` | The direction of the line |
| `className` | class | | A style class for the line |

### `unsupported`

What the panel draws for a `type` it does not know: a faded row with a warning icon, titled
"Unsupported:" and the type name cut to 64 characters, and the subtitle "This applet asked for an
element glimpse does not render". The SDKs have no builder for it, and it takes no props.
