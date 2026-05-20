# Bluetooth Applet Redesign

## Goal

The Bluetooth popover should be a compact control menu for the common device workflow: toggle Bluetooth, see connected devices, reveal nearby devices, adjust discoverability, and jump to settings.

## Constraints

- Use the shared HTML mockup components from `../components.html` and `../base.css`.
- Mirror the current shell component anatomy: `popover-shell`, `hero-row`, `section-header`, `list-item`, `action-row`, `collapsible-section`, and `separator`.
- Keep default content simple: rows, switches, one disclosure, and separators.
- Pairing flows, trust/block controls, codec details, and per-device diagnostics belong outside the default compact view.
- Keep device management actions out of the compact popover; route advanced management to Bluetooth Settings.

## ASCII Mockup

```text
+--------------------------------------+
| Bluetooth                    switch  |
| On                                   |
| ------------------------------------ |
| Devices                              |
| [mouse] MX Master 3S Connected 84%   |
| [keys]  Keychron K3  Connected 67%   |
| [audio] AirPods Pro  Paired          |
| Other Devices                    >   |
| ------------------------------------ |
| Visibility                           |
| Discoverable              switch     |
| ------------------------------------ |
| Bluetooth Settings                >  |
+--------------------------------------+
```

## Component Layout

```text
PopoverShell(.bluetooth-popover.popover-size-medium)
  PopoverShellContent
    HeroRow(title: "Bluetooth", subtitle: "On", trailing: Switch("Bluetooth"))
    Separator
    SectionHeader("Devices")
    ListItem(MX Master 3S, icon: mouse, secondary: "Connected", trailing: "84%")
    ListItem(Keychron K3, icon: keyboard, secondary: "Connected", trailing: "67%")
    ListItem(AirPods Pro, icon: audio, secondary: "Paired")
    CollapsibleSection(Other Devices, collapsed)
      ListItem(Sony WH-1000XM5, icon: audio, trailing: "Pair")
      ListItem(Pixel Buds, icon: audio, trailing: "Pair")
    Separator
    SectionHeader("Visibility")
    ActionRow(SwitchRow: "Discoverable")
    Separator
    ActionRow("Bluetooth Settings")
```

## Interaction Notes

- Bluetooth switch toggles `aria-checked`.
- Discoverable switch toggles `aria-checked`.
- Device rows are direct primary actions: connect for paired devices, disconnect for connected devices.
- Other Devices expands and collapses in place.
- Device rows use the shared left slot for compact device type icons.
- `Bluetooth Settings` is a simple action row with pressed feedback.
