# Battery Applet Redesign

## Goal

The Battery popover should be a compact control menu for battery state, power source, power mode, and nearby device batteries. It should use the same applet structure as Audio, Network, and Bluetooth.

## Constraints

- Use shared mockup components from `../components.html` and `../base.css`.
- Follow the common applet structure: hero, separator, section headers, rows/meters/switches, and a settings action.
- Keep the hero subtitle to current battery state, such as `76%, charging`.
- Use `PropertyList` for static power details.
- Expose switchable power profiles as selectable rows.
- Avoid charts, history graphs, cards, and diagnostics in the compact popover.
- External battery devices are secondary rows under `Devices`, not a separate dashboard.

## ASCII Mockup

```text
+--------------------------------------+
| Battery                              |
| 76%, charging                        |
| ------------------------------------ |
| Power                                |
| Battery                    meter 76% |
| Power Source          Power Adapter  |
| Time Until Full              34 min  |
| Health                      Normal   |
| Low Power Mode             switch    |
| ------------------------------------ |
| Power Profile                        |
| Power Saver                          |
| Balanced                         ✓   |
| Performance                          |
| ------------------------------------ |
| Devices                              |
| [mouse] MX Master 3S       meter 68% |
| [keys]  Keychron K3        meter 54% |
| ------------------------------------ |
| Battery Settings                 >   |
+--------------------------------------+
```

## Component Layout

```text
PopoverShell(.battery-popover.popover-size-medium)
  PopoverShellContent
    HeroRow(title: "Battery", subtitle: "76%, charging")
    Separator
    SectionHeader("Power")
    MeterRow("Battery", value: "76%")
    PropertyList(Power Source: "Power Adapter", Time Until Full: "34 min", Health: "Normal")
    ActionRow(SwitchRow: "Low Power Mode")
    Separator
    SectionHeader("Power Profile")
    ListItem("Power Saver", secondary: "Longer battery life")
    ListItem("Balanced", secondary: "Recommended", selected)
    ListItem("Performance", secondary: "Higher power use")
    Separator
    SectionHeader("Devices")
    ListItem(MX Master 3S, icon: mouse, secondary: "Mouse", trailing: Meter("68%"))
    ListItem(Keychron K3, icon: keyboard, secondary: "Keyboard", trailing: Meter("54%"))
    Separator
    ActionRow("Battery Settings")
```

## Interaction Notes

- Low Power Mode switch toggles `aria-checked`.
- Power Profile rows update the selected row checkmark.
- Device rows are informational and use compact meters.
- Battery Settings is a simple action row with pressed feedback.
- If no external devices have battery data, omit the `Devices` section.
