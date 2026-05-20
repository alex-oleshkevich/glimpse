# Network Applet Redesign

## Goal

The network popover should be a compact control menu for common connection tasks. It should show the connected Wi-Fi network directly, keep available networks behind one disclosure, expose Ethernet status, and provide VPN/settings without becoming a full network configuration page.

## Constraints

- Use the shared HTML mockup components from `../components.html` and `../base.css`.
- Mirror the current shell component anatomy: `popover-shell`, `hero-row`, `action-row`, `collapsible-section`, `list-item`, `badge`, and `separator`.
- Keep first-level content simple: rows, disclosures, switches, badges, and separators.
- Avoid raw implementation prefixes or technical details in the default view.
- Credentials, IP details, DNS, MAC address, and advanced configuration belong outside this compact popover.

## ASCII Mockup

```text
+--------------------------------------+
| Network                              |
| Home Wi-Fi                    switch |
| ------------------------------------ |
| Wi-Fi                                |
| Home Wi-Fi    Connected      86%     |
| Other Networks                    >  |
| ------------------------------------ |
| Ethernet                             |
| Wired               Disconnected     |
| ------------------------------------ |
| Hotspot                              |
| Enabled                   switch     |
| Glimpse Hotspot              Off     |
| ------------------------------------ |
| VPN                                  |
| Work VPN              Disconnected   |
| Personal VPN             Connected ✓ |
| ------------------------------------ |
| Network Settings                     |
+--------------------------------------+
```

## Component Layout

```text
PopoverShell(.network-popover.popover-size-medium)
  PopoverShellContent
    HeroRow(title: "Network", subtitle: "Home Wi-Fi", trailing: Switch("Wi-Fi"))
    Separator
    SectionHeader("Wi-Fi")
    ActionRow(Item: "Home Wi-Fi", subtitle: "Connected", trailing: "86%")
    CollapsibleSection(Other Networks, collapsed)
      ListItem(Office)
      ListItem(Phone Hotspot)
      ListItem(Cafe Net, trailing: "Secured")
      ListItem(Airport Free Wi-Fi)
      ListItem(Printer Setup, trailing: "Secured")
    Separator
    SectionHeader("Ethernet")
    ActionRow(Item: "Wired", subtitle: "Disconnected")
    Separator
    SectionHeader("Hotspot")
    ActionRow(SwitchRow: "Enabled")
    ActionRow(Item: "Glimpse Hotspot", subtitle: "Off")
    Separator
    SectionHeader("VPN")
    ListItem(Work VPN, trailing: "Disconnected")
    ListItem(selected: Personal VPN, trailing: "Connected", check mark)
    Separator
    ActionRow("Network Settings")
```

## Interaction Notes

- Wi-Fi switch toggles `aria-checked`.
- Hotspot switch toggles `aria-checked`.
- Other network selection updates the hero subtitle, connected Wi-Fi row, selected row state, and check mark.
- Other Networks expands and collapses in place.
- Other networks are selectable rows, but connecting to secured networks should later open a prompt rather than expanding inline password fields.
- VPN rows are represented as simple rows in this first mockup; a future version can make them selectable/toggleable if provider behavior is clear.
- `Network Settings` is a simple action row with pressed feedback.
