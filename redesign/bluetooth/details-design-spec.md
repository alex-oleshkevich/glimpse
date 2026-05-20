# Bluetooth Details Variant

## Goal

This variant keeps the simple Bluetooth popover shape, but adds a separate trailing disclosure target for device details and user-facing management actions.

## Constraints

- Hero subtitle reflects Bluetooth adapter state: `On`, `Off`, or `On, discoverable`.
- Device row main area is the primary action: connect or disconnect.
- Trailing disclosure opens details/actions without triggering the primary action.
- First-level actions use user-facing labels: `Disconnect`, `Forget Device`, and `Details`.
- Technical fields such as address, profile, UUID, and trust state are shown only inside the details area or in Bluetooth Settings.

## ASCII Mockup

```text
+--------------------------------------+
| Bluetooth                    switch  |
| On, discoverable                     |
| ------------------------------------ |
| Devices                              |
| [mouse] MX Master 3S Connected 84% > |
|   Disconnect                         |
|   Details                            |
|   Battery              84%           |
|   Profile              HID           |
| [keys]  Keychron K3  Connected 67% > |
| [audio] AirPods Pro  Paired       >  |
| Other Devices                    >   |
| ------------------------------------ |
| Bluetooth Settings                >  |
+--------------------------------------+
```

## Component Layout

```text
PopoverShell(.bluetooth-popover.popover-size-medium)
  PopoverShellContent
    HeroRow(title: "Bluetooth", subtitle: "On, discoverable", trailing: Switch("Bluetooth"))
    Separator
    SectionHeader("Devices")
    DeviceDetailRow(MX Master 3S, icon: mouse, secondary: "Connected", trailing: "84%", open)
      ActionRow("Disconnect")
      ActionRow("Details")
      PropertyList(Battery: "84%", Profile: "HID")
    DeviceDetailRow(Keychron K3, icon: keyboard, secondary: "Connected", trailing: "67%", collapsed)
      ActionRow("Disconnect")
      ActionRow("Details")
      PropertyList(Battery: "67%", Profile: "Keyboard")
    DeviceDetailRow(AirPods Pro, icon: audio, secondary: "Paired", collapsed)
      ActionRow("Connect")
      ActionRow("Forget Device")
      ActionRow("Details")
    CollapsibleSection(Other Devices, collapsed)
    Separator
    ActionRow("Bluetooth Settings")
```

## Interaction Notes

- Bluetooth switch toggles `aria-checked` and updates the hero subtitle between `On` and `Off`.
- Device row main buttons are direct primary actions and do not expand the row.
- Device row chevrons expand and collapse inline detail content.
- Paired but disconnected devices expose `Connect` and `Forget Device`; connected devices expose `Disconnect` and `Details`.
- Technical details stay visually secondary.
