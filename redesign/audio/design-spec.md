# Audio Applet Redesign

## Goal

The audio popover should feel like a compact menu, not a dashboard. It should expose the common actions directly: output volume, input volume, output mute, input mute, device selection, output streams, and stream mute.

## Constraints

- Use the shared HTML mockup components from `../components.html` and `../base.css`.
- Keep the layout row-first: headers, menu rows, separators, sliders, switches, disclosure rows, and meters.
- Do not use cards for the main flow.
- Builtin and custom applet renderers should map this structure to the same shared GTK/template widgets.

## ASCII Mockup

```text
+--------------------------------------+
| Audio                         68%    |
| Built-in Speakers                    |
|                                      |
| Output                               |
| [slider]                  68%        |
| Mute                    switch       |
| Device             Built-in        v |
|   Built-in Speakers              ✓   |
|   USB-C Dock Audio                   |
| ------------------------------------ |
| Input                                |
| [slider]                  54%        |
| Mute                    switch       |
| Device             Laptop          > |
| ------------------------------------ |
| Output Streams                       |
| Firefox                  meter  Mute |
| Spotify                  meter  Mute |
+--------------------------------------+
```

## Component Layout

```text
PopoverScaffold(size: medium)
  Hero(icon: optional audio symbol, title: "Audio", subtitle: "Built-in Speakers - 68%")
  Header("Output")
  SliderRow(accessible_label: "Output volume", visible_label: none, value: 68)
  SwitchRow(title: "Mute", active: false)
  DisclosureItem(title: "Device", subtitle: "Built-in Speakers", expanded: true)
    ActionItem(title: "Built-in Speakers", selected: true)
    ActionItem(title: "USB-C Dock Audio")
  Separator
  Header("Input")
  SliderRow(accessible_label: "Input volume", visible_label: none, value: 54)
  SwitchRow(title: "Mute", active: false)
  DisclosureItem(title: "Device", subtitle: "Laptop Microphone", expanded: false)
  Separator
  Header("Output Streams")
  Item(title: "Firefox", body: Meter(value: 42), trailing: Button("Mute"))
  Item(title: "Spotify", body: Meter(value: 76), trailing: Button("Mute"))
```

## Interaction Notes

- Output and input mute use `SwitchRow` because they are persistent binary settings.
- Device changes use `DisclosureItem` so the popover stays one menu instead of opening a secondary page.
- Stream rows use `Meter` for live activity and a compact trailing icon button for muting each stream.
- All menu-like widgets must share the same internal row renderer: `Item`, `ActionItem`, `DisclosureItem`, `SwitchRow`, `SliderRow`, and `CheckboxRow`.
- Row anatomy is fixed: icon slot, title/subtitle content slot, trailing control slot.
- Leading icons are optional semantic decoration. Never use visible abbreviation prefixes as labels.
- The default audio popover should stay compact enough to read as a control menu, not a dashboard.
- Selected device rows use a quiet check mark, not a text label.
- Section context should remove repeated labels: inside `Output` and `Input`, use `Mute` and `Device`; volume sliders keep accessible labels but no visible repeated `Output volume` or `Input volume` text.
