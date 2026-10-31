# glimpse

A desktop shell suite for Wayland: panel, wallpaper, lock screen, launcher and standalone providers of system state.

## Language

### Launcher

**Launcher**:
The keyboard-driven search surface that finds and runs apps, commands and answers from one query.
_Avoid_: Spotlight, runner, palette

**Provider**:
A source of launcher results for a query, returning zero or more results each time it is asked.
_Avoid_: Extension, plugin, runner, module

**Built-in provider**:
A provider that ships inside the launcher itself.
_Avoid_: Core plugin, native extension

**External provider**:
A provider that runs as its own program and talks to the launcher over the provider protocol.
_Avoid_: Extension, user plugin, third-party extension

**Result**:
One row a provider returns for a query.
_Avoid_: Item, match, hit, entry

**Answer**:
A result that is the computed reply to the query itself, shown large, such as a calculation or a conversion.
_Avoid_: Inline result, instant answer, card

**Prefix**:
The leading symbol, or word followed by a space, that routes a query to one provider only.
_Avoid_: Trigger, keyword, bang, mode

**Page**:
A screen a provider pushes in place of the result list after a result is chosen.
_Avoid_: View, panel, detail

**Action**:
Something that can be done with a result; the primary action runs on Enter, the secondary on Shift+Enter.
_Avoid_: Command, verb, operation

### On-screen display

**OSD**:
A short-lived, click-through display of one change, showing an icon, a label, a level or any mix of them; it keeps no history and takes no input.
_Avoid_: toast, notification, HUD, popup

**Producer**:
A program that decides an OSD should appear and sends what it shows.
_Avoid_: source, client, watcher
