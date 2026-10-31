# Win-key board: research

This note covers the Win-key board: a Super-tap toggled full-screen overlay on the focused output
that holds tasks, text notes, sticky notes and status cards from existing applets. It gathers the
trigger, surface, prior-art and storage evidence. The companion note,
[desktop-space-research.md](desktop-space-research.md), holds the UX evidence, the failed products
and the card ideas. Neither is repeated here beyond one-line pointers.

## Verdict

The board is worth having only if it beats the tools it replaces on the first second after the
key press. Six things decide that.

- **Typing starts the moment the board opens.** Median capture took 14.2 s (captures under a
  minute) in a two-year log study of 420 users
  ([Van Kleek et al., CHI 2011](http://people.csail.mit.edu/emax/papers/chi2011-finders-keepers.pdf)).
  WidgetKit has no text field
  ([Apple](https://developer.apple.com/documentation/widgetkit/swiftui-views.md)) and RemoteViews
  has no EditText
  ([Android](https://developer.android.com/reference/android/widget/RemoteViews)), so a native text
  input in a card is the differentiator. Land the caret in the last-edited note, as the KDE Notes
  applet does
  ([source](https://invent.kde.org/plasma/kdeplasma-addons/-/raw/10b5f2450159612bae03fd854f29a9da386208ca/applets/notes/package/contents/ui/main.qml)).
- **The trigger must be bare Super and must never fire on a chord.** niri 26.04, installed here,
  has no bare-modifier bind; the feature merged 2026-10-25 and is "Since: next release"
  ([PR #3621](https://github.com/niri-wm/niri/pull/3621),
  [v26.04](https://github.com/niri-wm/niri/releases/tag/v26.04)). One user reported Hyprland's
  release bind firing after every `SUPER+key` chord following the keybind rewrite
  ([#15568 comment](https://github.com/hyprwm/Hyprland/pull/15568#issuecomment-5230819813)); a
  fix is in v0.57.0 ([#15904](https://github.com/hyprwm/Hyprland/pull/15904)). A board that opens
  on `Super+Return` is worse than none.
- **No feed, no push.** Windows now separates the Discover feed from widget dashboards and tests
  quiet defaults
  ([Insider blog](https://blogs.windows.com/windows-insider/2025/11/04/announcing-windows-11-insider-preview-build-27982-canary-channel),
  [Windows Latest](https://www.windowslatest.com/2026/05/03/microsoft-is-finally-turning-off-the-msn-feed-and-ads-in-windows-11-widgets-by-default)).
  Batchers rated their productivity higher than notification-driven users
  ([Mark et al., CHI 2016](http://dx.doi.org/10.1145/2858036.2858262)).
- **Data must survive the board.** Each widget's file has to be readable without the board. An
  autosave overwrote an externally edited file in DMS
  ([#2631](https://github.com/duxlabs-dev/dux-shell/issues/2631)), and Windows Sticky Notes deletes
  permanently with only a confirm prompt as mitigation
  ([Microsoft FAQ](https://support.microsoft.com/en-us/windows/apps/stickynotes/faq-for-sticky-notes-administrators)).
- **Notes and tasks are the gap.** Of the Linux shells read, only end-4 ships a notes card, and its
  todo list lives in a sidebar
  ([Overlay.qml](https://github.com/end-4/dots-hyprland/blob/2f2a376fdb2ad738bf55f693dd0b474b64bb0ab8/dots/.config/quickshell/ii/modules/ii/overlay/OverlayContext.qml#L10),
  [Todo.qml](https://github.com/end-4/dots-hyprland/blob/2f2a376fdb2ad738bf55f693dd0b474b64bb0ab8/dots/.config/quickshell/ii/services/Todo.qml)).
  Caelestia, HyprPanel and DMS's dash ship neither
  ([Caelestia](https://github.com/caelestia-dots/shell/blob/b51fdb35ca7a14ac5a49e0d94fc5de2d79756aed/modules/dashboard/Content.qml#L19),
  [HyprPanel](https://github.com/Jas-SinghFSU/HyprPanel/blob/1961ba86ad5ab880beb639e5454054b2b5037e0d/src/components/menus/dashboard/index.tsx#L14),
  [DMS](https://github.com/duxlabs-dev/dux-shell/blob/40a129c9d6f75566073d1a5eb1d4fb3ca48a79e4/quickshell/Common/SettingsData.qml#L536)).
- **Open risk: the board is not in view.** A task list beside a second monitor worked, and a list
  hidden in a Gmail pane was forgotten
  ([Haraty et al., GI 2012](https://www.cs.ubc.ca/labs/spin/sites/default/files/local_pdfs/2012_IndividualDifferencesInPersonalTaskManagement.pdf)).
  Always-visible reminders helped most under high load, and that study did not test a summoned
  overlay
  ([Ball et al. 2025](https://pmc.ncbi.nlm.nih.gov/articles/PMC11781985/)). No peer-reviewed study
  of desktop or home-screen widget use was found. Plan opt-in local counting of card opens.

## Trigger

| Compositor | Bare Super tap | Mechanism | Caveats |
| --- | --- | --- | --- |
| niri `main` (after 2026-10-25) | Yes, native | `Mod { release { spawn ...; } }` ([docs](https://github.com/niri-wm/niri/blob/main/docs/wiki/Configuration:-Key-Bindings.md)) | Not in v26.04. The Super press is still forwarded to the focused app. Any other key or mouse button cancels it. A shortcut inhibitor skips it ([`src/input/mod.rs`](https://github.com/niri-wm/niri/blob/main/src/input/mod.rs)). Pass-through ([#4755](https://github.com/niri-wm/niri/pull/4755)) is open |
| niri 26.04 (installed) | No | keyd, kanata or xremap emit F13 on tap; bind F13 ([keyd](https://github.com/rvaiya/keyd/blob/master/docs/keyd.scdoc), [kanata](https://github.com/jtroo/kanata/blob/main/docs/config.adoc), [xremap](https://github.com/xremap/xremap/blob/master/doc/reference_multipurpose_key.md)) | keyd is a system daemon under `/etc/keyd/`, a packaging burden. The only workaround offered in the niri discussion is keyd ([#4142](https://github.com/niri-wm/niri/discussions/4142)) |
| Hyprland 0.55+ (v0.57.0 latest) | Yes, native, fragile | `hl.bind("SUPER_L", ..., {release=true})`; legacy `bindr = SUPER, SUPER_L` ([Binds](https://wiki.hypr.land/configuring/core/binds/)) | One user reported the release bind firing after every `SUPER+key` chord after the 0.56 keybind rewrite ([#15568 comment](https://github.com/hyprwm/Hyprland/pull/15568#issuecomment-5230819813)); v0.57.0 lists the release-bind subchord fix ([#15904](https://github.com/hyprwm/Hyprland/pull/15904), [v0.57.0](https://github.com/hyprwm/Hyprland/releases/tag/v0.57.0)). A comment reports a bare-modifier release never firing on 0.56.2 ([#6946](https://github.com/hyprwm/Hyprland/issues/6946)); 0.57.0 bare-modifier behavior is untested. 0.55.3 reversed catchall versus specific bind order and broke a tap-Super hack ([caelestia #1545](https://github.com/caelestia-dots/shell/issues/1545)). A release is lost when another key is released between ([#11483](https://github.com/hyprwm/Hyprland/discussions/11483)) |
| GNOME (mutter) | Yes, built in | `org.gnome.mutter overlay-key` ([schema](https://gitlab.gnome.org/GNOME/mutter/-/blob/main/data/org.gnome.mutter.gschema.xml.in)) | No time limit on the hold; pointer and scroll cancel it ([keybindings.c](https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/keybindings.c)). Not a glimpse target |

### niri

- **Target the merged bind, with an F13 fallback.** The config is `Mod { release { spawn "glimpsectl"
  "board" "toggle"; } }`. It is invalid on 26.04
  ([PR #3621 docs](https://github.com/niri-wm/niri/blob/main/docs/wiki/Configuration:-Key-Bindings.md),
  [discussion #4142](https://github.com/niri-wm/niri/discussions/4142)). No release date for the
  version containing it was found. Ship both paths from day one.
- **The fire-on-release contract is the maintainer's.** If a press action of a press-release binding
  triggered, its release must trigger
  ([YaLTeR](https://github.com/niri-wm/niri/pull/3621#issuecomment-5778917202)). Any other key or
  mouse button cancels a release-only bind
  ([docs](https://github.com/niri-wm/niri/blob/main/docs/wiki/Configuration:-Key-Bindings.md)), so
  the board maps after the release, not on press.

### Hyprland

- **Use the native release bind, but tell users to test it.** The documented form is
  `hl.bind("SUPER_L", hl.dsp.exec_cmd("glimpsectl board toggle"), { release = true })`
  ([Binds](https://wiki.hypr.land/configuring/core/binds/)). Run the manual test "hold Super, press
  Return, release Super" before declaring it good, because the one chord report was on the rewrite
  ([#15568 comment](https://github.com/hyprwm/Hyprland/pull/15568#issuecomment-5230819813)) and its
  fix landed in [#15904](https://github.com/hyprwm/Hyprland/pull/15904), whose reporter confirmed
  it worked. Bare-modifier behavior on 0.57.0 is untested.
- **Ship an alternative that avoids the bare-modifier path.** Use the same F13 remapper as on niri,
  or a plain chord such as `SUPER + B`
  ([keyd.scdoc](https://github.com/rvaiya/keyd/blob/master/docs/keyd.scdoc)).

### IPC toggle

- **Put the toggle logic in the board process.** `glimpsectl board toggle|show|hide` is then the same
  for every trigger, so an F13 bind and a native release bind coexist. end-4, DMS, Noctalia and
  Caelestia all expose an IPC toggle
  ([end-4](https://github.com/end-4/dots-hyprland/blob/2f2a376fdb2ad738bf55f693dd0b474b64bb0ab8/dots/.config/quickshell/ii/modules/ii/overlay/Overlay.qml#L78),
  [DMS](https://github.com/duxlabs-dev/dux-shell/blob/40a129c9d6f75566073d1a5eb1d4fb3ca48a79e4/quickshell/DMSShellIPC.qml#L436),
  [Caelestia](https://github.com/caelestia-dots/shell/blob/b51fdb35ca7a14ac5a49e0d94fc5de2d79756aed/daemons/Shortcuts.qml#L40)).
- **Capture without opening.** Linux Mint Sticky exposes `NewNote(text)`, `NewNoteBlank`, `ShowNotes`
  and a `NotesChanged` signal over D-Bus
  ([README](https://github.com/linuxmint/sticky/blob/master/README.md)). A `task add "..."` command
  fills the same role for a compositor bind, and DMS ships per-card commands such as `notepad`
  ([DMSShellIPC.qml](https://github.com/duxlabs-dev/dux-shell/blob/40a129c9d6f75566073d1a5eb1d4fb3ca48a79e4/quickshell/DMSShellIPC.qml#L559)).
- **Chords to test on both compositors.** Super+drag, Super+scroll and a fast `Super+key` roll-over:
  niri cancels on keys and mouse buttons, mutter also on scroll, and sway's mouse bug is the failure
  case
  ([niri docs](https://github.com/niri-wm/niri/blob/main/docs/wiki/Configuration:-Key-Bindings.md),
  [mutter](https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/keybindings.c),
  [sway #4505](https://github.com/swaywm/sway/issues/4505)).

## Surface and input

- **One full-screen layer surface, not a window per card.** end-4's overlay is one `PanelWindow` on
  `WlrLayer.Overlay`, anchored to all four edges
  ([Overlay.qml](https://github.com/end-4/dots-hyprland/blob/2f2a376fdb2ad738bf55f693dd0b474b64bb0ab8/dots/.config/quickshell/ii/modules/ii/overlay/Overlay.qml#L27)).
  Per-card surfaces in DMS, Noctalia and eww multiply bookkeeping, and `Bottom`-layer surfaces stay
  under windows
  ([DMS](https://github.com/duxlabs-dev/dux-shell/blob/40a129c9d6f75566073d1a5eb1d4fb3ca48a79e4/quickshell/Modules/Plugins/DesktopPluginWrapper.qml#L68),
  [Noctalia](https://github.com/noctalia-dev/noctalia/blob/a531428700c0c06ce68be3f1f80ed6590d3f4034/src/shell/desktop/desktop_widgets_host.cpp#L345),
  [eww](https://github.com/adi1090x/widgets/blob/c16e32c8786d67d91d6c11b50c2e183d26054186/eww/dashboard/eww.yuck#L248)).
- **Keyboard: `exclusive` while open.** Exclusive is meant for lock screens and password prompts,
  and the protocol leaves the winner between several exclusive surfaces undefined
  ([layer-shell XML](https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-layer-shell-unstable-v1.xml)).
  On-demand focus arrives only with a click, so keys would reach the window behind. glimpse's popover
  catcher already switches to `KeyboardMode::Exclusive` for typing, and `glimpse-lens`,
  `glimpse-shot` and `glimpse-agents` use `Layer::Overlay` with it
  (`crates/glimpse-panel/src/applet/catcher.rs`).
- **Never leave a mapped invisible surface.** DMS's notepad left an invisible overlay over open apps
  ([#3802](https://github.com/duxlabs-dev/dux-shell/issues/3802)), and its slideout stayed mapped
  after closing with Reduce Motion
  ([#3822](https://github.com/duxlabs-dev/dux-shell/issues/3822)). Destroy the surface on close
  (or set an empty input region) and test with reduced motion.
- **Overview interplay.** niri hit-tests the overlay layer first with no overview condition, and
  focuses an exclusive overlay surface before `KeyboardFocus::Overview`, so the board is
  interactive above the overview
  ([`contents_under`, `update_keyboard_focus`](https://github.com/niri-wm/niri/blob/main/src/niri.rs)).
  That resolves the "untested" note in the companion note.
- **Multi-monitor: one output only.** A layer surface with no output goes to niri's
  `layout.active_output()` and Hyprland's `focusState()->monitor()`
  ([niri](https://github.com/niri-wm/niri/blob/main/src/handlers/layer_shell.rs),
  [Hyprland](https://github.com/hyprwm/Hyprland/blob/main/src/desktop/view/LayerSurface.cpp)).
  Map nothing on the other outputs. glimpse already reads niri's `FocusedOutput`
  ([niri-ipc](https://github.com/niri-wm/niri/blob/main/niri-ipc/src/lib.rs),
  `crates/glimpse-compositor-niri/src/niri.rs`). end-4's overlay duplicated or stuck widgets across
  two monitors ([#2946](https://github.com/end-4/dots-hyprland/issues/2946)).
- **IME: let GTK4 do it.** GTK3 and GTK4 support text-input-v3 natively
  ([Fcitx wiki](https://fcitx-im.org/wiki/Using_Fcitx_5_on_Wayland)), and niri renders IME popups
  for layer surfaces
  ([layer/mapped.rs](https://github.com/niri-wm/niri/blob/main/src/layer/mapped.rs)). A
  toolkit-less shell lost IME in its rewrite and has reversed-scroll bugs in its Notes input
  ([Noctalia #2786](https://github.com/noctalia-dev/noctalia/issues/2786),
  [#3986](https://github.com/noctalia-dev/noctalia/issues/3986)). Use GTK `TextView`/`Entry`, never
  custom-drawn text. Watch the open niri bug where a quick Tab press-release repeats forever when an
  fcitx5 session ends in between
  ([#4737](https://github.com/niri-wm/niri/issues/4737)).

## Prior art

### Other OSes

| Surface | Invocation | Layout | Typing in place | Data owner |
| --- | --- | --- | --- | --- |
| macOS desktop widgets | Always on wallpaper; Edit Widgets ([Apple](https://support.apple.com/guide/mac-help/add-and-customize-widgets-mchl52be5da5/26/mac/26)) | Free placement, 4 sizes ([HIG](https://developer.apple.com/design/human-interface-guidelines/widgets)) | No: buttons and toggles ([WidgetKit](https://developer.apple.com/documentation/widgetkit/adding-interactivity-to-widgets-and-live-activities)) | The app |
| macOS Quick Note | Fn/Globe+Q or hot corner ([Apple](https://support.apple.com/guide/notes/create-a-quick-note-apdf028f7034/mac)) | One window; resumes the last note | Yes | Notes app |
| Windows 11 Widgets | Win+W ([Support](https://support.microsoft.com/en-us/windows/experience/personalization/stay-up-to-date-with-widgets-in-windows)) | Cards in 3 sizes | No, by guidance ([interaction design](https://learn.microsoft.com/en-us/windows/apps/design/widgets/widgets-interaction-design)) | Each card's app or service |
| Windows Sticky Notes | Start menu, taskbar, Ctrl+N ([Support](https://support.microsoft.com/en-us/windows/apps/stickynotes/get-started-with-sticky-notes)) | Free windows plus a list | Yes | Microsoft account sync |
| KDE Plasma 6 | Always on desktop | Free, rotatable ([Handbook](https://docs.kde.org/stable_kf6/en/plasma-desktop/plasma-desktop/desktop.html)) | Yes, Notes plasmoid | The plasmoid; UUID files |
| GNOME | Super opens the overview ([Help](https://help.gnome.org/gnome-help/keyboard-key-super.html)) | Windows, dash, search | Search box only | The shell |

- **Glance is not typing.** Windows tells developers a call to action should launch the app instead
  of acting in the widget
  ([interaction design](https://learn.microsoft.com/en-us/windows/apps/design/widgets/widgets-interaction-design)).
  Apple's allowed interactions are buttons and toggles
  ([HIG](https://developer.apple.com/design/human-interface-guidelines/widgets)).
- **Plasma ids orphan notes.** Plasma maps a note by `noteId` in applet config to a UUID file, and a
  broken mapping orphaned a user's notes
  ([Manjaro forum](https://forum.manjaro.org/t/howto-recover-sticky-notes-on-the-desktop/164845)).

### Linux shells

| Project | Surface / layer / keyboard | Toggle | Notes / todo |
| --- | --- | --- | --- |
| [end-4 `ii`](https://github.com/end-4/dots-hyprland/tree/2f2a376fdb2ad738bf55f693dd0b474b64bb0ab8) | One full-screen `Overlay` panel; `Exclusive` when open, `OnDemand` with pinned widgets, else `None`; input mask ([Overlay.qml](https://github.com/end-4/dots-hyprland/blob/2f2a376fdb2ad738bf55f693dd0b474b64bb0ab8/dots/.config/quickshell/ii/modules/ii/overlay/Overlay.qml#L29)) | `SUPER+G` or `qs ipc` | `notes.txt`; todo in `todo.json` in the sidebar |
| [DMS dash](https://github.com/duxlabs-dev/dux-shell/tree/40a129c9d6f75566073d1a5eb1d4fb3ca48a79e4) | Popout `dms:dash` attached to the bar ([DDashPopout.qml](https://github.com/duxlabs-dev/dux-shell/blob/40a129c9d6f75566073d1a5eb1d4fb3ca48a79e4/quickshell/Modules/DDash/DDashPopout.qml#L15)) | `dms ipc call dash toggle` | None; layout in settings |
| [Noctalia](https://github.com/noctalia-dev/noctalia/tree/a531428700c0c06ce68be3f1f80ed6590d3f4034) | One `Bottom` surface per widget, keyboard `None` | `noctalia msg desktop-widgets-edit` | Plugin only; layout in `settings.toml` |

- **Closest analogue is end-4's overlay, not a sidebar.** It has a scrim, a scale-in from 1.08, an
  `OverlayTaskbar` for opening cards, a fixed registry of seven game and utility widgets, and no
  grid. Each widget stores absolute `x, y, width, height` in `states.json`
  ([OverlayContext.qml](https://github.com/end-4/dots-hyprland/blob/2f2a376fdb2ad738bf55f693dd0b474b64bb0ab8/dots/.config/quickshell/ii/modules/ii/overlay/OverlayContext.qml#L10),
  [Persistent.qml](https://github.com/end-4/dots-hyprland/blob/2f2a376fdb2ad738bf55f693dd0b474b64bb0ab8/dots/.config/quickshell/ii/modules/common/Persistent.qml#L86)).
  A pinned corner widget enlarged and left the screen when the overlay closed
  ([#3455](https://github.com/end-4/dots-hyprland/issues/3455)).
- **DMS has the best card editor.** Its dash popout grid defaults to 6 columns (3 to 8) with a 96 px
  row unit, cards carry integer `w` and `h`, and `packCards` places them in order rather than at
  absolute cells
  ([DashMetrics.qml](https://github.com/duxlabs-dev/dux-shell/blob/40a129c9d6f75566073d1a5eb1d4fb3ca48a79e4/quickshell/Modules/DDash/DashMetrics.qml#L11),
  [GridLayout.js](https://github.com/duxlabs-dev/dux-shell/blob/40a129c9d6f75566073d1a5eb1d4fb3ca48a79e4/quickshell/Common/GridLayout.js#L95)).
  Its keyboard navigation and edit mode both have open complaints
  ([#3997](https://github.com/duxlabs-dev/dux-shell/issues/3997),
  [#3532](https://github.com/duxlabs-dev/dux-shell/issues/3532)).
- **Noctalia's editor.** It offers an optional snap grid (`cell_size` 16), lasso, copy and paste, and
  asks the user to switch to an empty workspace to see it, a consequence of the `Bottom` layer
  ([widgets.mdx](https://github.com/noctalia-dev/noctalia/blob/a531428700c0c06ce68be3f1f80ed6590d3f4034/docs/user/desktop/widgets.mdx#L48)).
  Its scope file pushes notes and tasks to plugins
  ([SCOPE.md](https://github.com/noctalia-dev/noctalia/blob/a531428700c0c06ce68be3f1f80ed6590d3f4034/SCOPE.md#L23)).

### Sticky-note apps

| App | Position model | Storage | Wayland status |
| --- | --- | --- | --- |
| [Xpad](https://git.launchpad.net/xpad/tree/?id=230ab4bd6df7c02cde34417c111917f03cbf6618) | `gtk_window_move`, `gtk_window_stick` ([xpad-pad.c](https://git.launchpad.net/xpad/tree/src/xpad-pad.c?id=230ab4bd6df7c02cde34417c111917f03cbf6618#n519)) | `info-*` and `content-*` per pad ([xpad-app.c](https://git.launchpad.net/xpad/tree/src/xpad-app.c?id=230ab4bd6df7c02cde34417c111917f03cbf6618#n310)) | Placement not restored ([ChangeLog](https://git.launchpad.net/xpad/tree/ChangeLog?id=230ab4bd6df7c02cde34417c111917f03cbf6618#n37)) |
| [Linux Mint Sticky](https://github.com/linuxmint/sticky/tree/0e780c2c9ad847e88d08a0d3fa4cd4679202d513) | `move(x, y)`, `stick()`, `set_keep_above()` | One `notes.json`, 3 s save delay, timed backups ([common.py](https://github.com/linuxmint/sticky/blob/0e780c2c9ad847e88d08a0d3fa4cd4679202d513/usr/lib/sticky/common.py#L11)) | Guesses monitor 0 ([sticky.py](https://github.com/linuxmint/sticky/blob/0e780c2c9ad847e88d08a0d3fa4cd4679202d513/usr/lib/sticky/sticky.py#L1050)); Wayland is "low priority" ([#129](https://github.com/linuxmint/sticky/issues/129)) |
| [macOS Stickies](https://support.apple.com/guide/stickies/welcome/mac) | Free windows; Arrange By; collapse to first line | Local; plain text, RTF, RTFD export | Notes vanish while the app is closed |

- **A toplevel cannot place itself on Wayland.** `xdg_shell` has no client position request, which is
  why each sticky app stores coordinates it cannot apply
  ([xdg-shell XML](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/raw/main/stable/xdg-shell/xdg-shell.xml)).
  Stickies drawn inside the board's own surface have no such limit.
- **The durable storage pattern is one file pair per note.** Metadata and text are separate and
  readable by other tools such as
  [xpad-trigger](https://github.com/langenhagen/xpad-trigger) and
  [xpad-tui](https://github.com/pistom/xpad-tui).

### Note and task apps

| App | Capture | Date input | Ordering | Completed | Daily concept |
| --- | --- | --- | --- | --- | --- |
| [Drafts](https://docs.getdrafts.com/gettingstarted) | Opens ready to type | None | Sort options, flag | Dimmed and struck through ([Tasks](https://docs.getdrafts.com/docs/editor/tasks)) | Workspace date filter ([Workspaces](https://docs.getdrafts.com/docs/drafts/workspaces)) |
| [Things](https://culturedcode.com/things/support/articles/2249437) | Ctrl+Space, type, Cmd+Return; Esc discards | `17d`, `2p`, `3rd wed jul` ([NL](https://culturedcode.com/things/support/articles/9780167/)) | Drag order in Today | Logbook kept forever ([lists](https://culturedcode.com/things/support/articles/4001304)) | Today is a date filter |
| [Todoist](https://www.todoist.com/help/todoist/features/use-task-quick-add-in-todoist-va4Lhpzz) | `Q` or a global shortcut; inline tokens | Extensive; click to untag | By date, priority or project | Hidden; Display toggle ([completed](https://www.todoist.com/help/todoist/features/view-completed-tasks-in-todoist-J19h2s)) | Today view |
| [MS To Do](https://support.microsoft.com/en-us/todo/my-day-and-suggestions) | Add in My Day | Highlight; Backspace undoes ([smart date](https://support.microsoft.com/en-us/todo/smart-due-date-reminder-recognition-in-microsoft-to-do)) | None found | None found | My Day resets nightly |
| [todo.txt](https://raw.githubusercontent.com/todotxt/todo.txt/master/README.md) | Append a line | `due:` literal | Priority letter, file order | `x date` prefix sorts last | None found |
| [Obsidian daily notes](https://github.com/obsidianmd/obsidian-help/blob/master/en/Plugins/Daily%20notes.md) | Command or hotkey | None | One file per day | None found | New file per day, no reset |

- **"Daily reset" has three implementations.** (1) Mutate nightly: My Day resets and offers
  leftovers as suggestions ([MS](https://support.microsoft.com/en-us/todo/my-day-and-suggestions)).
  (2) Derive from dates: Things' Today filters on start date, deadline or repeat
  ([Things](https://culturedcode.com/things/support/articles/4001304)), and a VTODO with neither
  `DTSTART` nor `DUE` is associated with each successive date until completed
  ([RFC 5545](https://www.rfc-editor.org/rfc/rfc5545.txt)). (3) New file per day
  ([Obsidian](https://github.com/obsidianmd/obsidian-help/blob/master/en/Plugins/Daily%20notes.md)).
  Option 2 needs no scheduled job [INFERENCE].
- **Date parsing needs an escape hatch.** Todoist highlights "monthly" in "Create monthly report" and
  lets the user click it back to text
  ([Quick Add](https://www.todoist.com/help/todoist/features/use-task-quick-add-in-todoist-va4Lhpzz)).
  "Next Tuesday" skips this week's Tuesday if it has not yet happened
  ([Todoist](https://www.todoist.com/help/todoist/features/schedule-a-date-and-time-for-your-todoist-tasks-q7VobO)).

## Research

Papers not in the companion note.

- **Notes are mostly reference, and capture speed is the draw.** 47% of the coded sample of List-it
  notes were reference items, and 17% of users never searched
  ([Van Kleek et al. 2011](http://people.csail.mit.edu/emax/papers/chi2011-finders-keepers.pdf)).
  Implication: build the notes card as a scratchpad with plain short-query search, not a to-do list.
- **Lists in view are used, lists behind an app are forgotten.** 11 of 19 interviewees were
  do-it-yourselfers, and done tasks were deleted "to avoid cluttering the screen"
  ([Haraty et al., GI 2012](https://www.cs.ubc.ca/labs/spin/sites/default/files/local_pdfs/2012_IndividualDifferencesInPersonalTaskManagement.pdf)).
  Implication: delete-on-done is an observed default.
- **Most people will not configure a tool.** 40 of 75 survey adopters used a pre-installed tool
  ([Haraty et al., IJHCS 2016](https://www.cs.ubc.ca/labs/edapt/papers/haraty2016_2.pdf)).
  Implication: ship a useful default layout and let DIY users rearrange.
- **Cue-based intentions beat time-based ones.** Subjects forgot more time-based tasks than
  place-based ones ([Sellen et al. 1997](https://pubmed.ncbi.nlm.nih.gov/9282220)). Implication: the
  Super tap is the cue, so cards show what is due today and do not fire timers.
- **People remember locations.** With the Ribbon hidden, clicks landed a median 92 px from the
  right place ([Scarr et al., CHI 2012](https://dl.acm.org/doi/abs/10.1145/2207676.2207713)).
  Implication: a stable layout pays off with repetition, so do not rescale cards between outputs.
- **Adaptive menus lose.** 27 subjects, with some calling adaptive menus inconsistent
  ([Findlater & McGrenere, CHI 2004](https://dl.acm.org/doi/10.1145/985692.985704)). Implication: no
  auto-sorting or recency promotion. Use a separate highlight that does not move cards.
- **Batching beats notification-driven use.** 40 workers over 12 days rated productivity higher when
  they batched or self-interrupted
  ([Mark et al. 2016](http://dx.doi.org/10.1145/2858036.2858262)). Implication: pull, never push.

## Widgets

The board owns placement only. Each widget's data must be readable and writable without it
([Plasma keeps only a `noteId` in applet config](https://github.com/KDE/kdeplasma-addons/blob/acdd682885fa904d0dd2c2813cdc492561b90b82/applets/notes/main.xml)).

### Tasks

- Opening the board puts the caret in an inline add field, as Drafts opens ready to type and Things
  saves on Cmd+Return
  ([Drafts](https://docs.getdrafts.com/gettingstarted),
  [Things](https://culturedcode.com/things/support/articles/2249437)). Document what Esc does.
- A task card that cannot add in place is a status card: ChromeOS sent "add task" to the web and
  Apple's Reminders widget only toggles
  ([Android Central](https://www.androidcentral.com/chromebooks-laptops/chromeos-canary-118-calendar-tasks),
  [HIG](https://developer.apple.com/design/human-interface-guidelines/widgets)).
- Parse dates with a small deterministic grammar, resolve ambiguity forward, highlight recognized
  words and let Backspace or a click revoke them, and make morning and evening hours configurable
  ([MS To Do](https://support.microsoft.com/en-us/todo/smart-due-date-reminder-recognition-in-microsoft-to-do),
  [Todoist](https://www.todoist.com/help/todoist/features/schedule-a-date-and-time-for-your-todoist-tasks-q7VobO)).
- Address items by an id, not an array index. end-4 addresses items by index and rewrites the whole
  file on each change
  ([Todo.qml](https://github.com/end-4/dots-hyprland/blob/2f2a376fdb2ad738bf55f693dd0b474b64bb0ab8/dots/.config/quickshell/ii/services/Todo.qml#L73)).

| Format | For | Against |
| --- | --- | --- |
| todo.txt (recommended) | One line per task, so append-on-capture is trivial. `x date` sorts done lines last. `+project`, `@context` and `due:` match inline tokens ([README](https://raw.githubusercontent.com/todotxt/todo.txt/master/README.md)) | No manual-order field, so file order is the order. No stable ids or recurrence model [INFERENCE from the rules] |
| VTODO over CalDAV | Phone sync through DAVx5 with Tasks.org or jtx Board ([manual](https://manual.davx5.com/tasks_notes.html)); stable `UID` | Needs a server; no manual order without the non-standard `x-apple-sort-order`, which Tasks.org supports only partly over CalDAV ([Tasks.org](https://tasks.org/docs/manual_sort_mode)); verbose CRLF format ([RFC 5545](https://www.rfc-editor.org/rfc/rfc5545.txt)) |
| Private JSON | Stable ids and undo are easy | Opaque to other tools; Taskwarrior moved to SQLite only for sync ([TW3](https://taskwarrior.org/docs/upgrade-3/)) |

**Recommendation.** Store todo.txt lines, with file order as manual order, `(A)`-`(C)` or `pri:`
for priority and `due:` for dates. Treat VTODO as a later adapter, with order carried in
`x-apple-sort-order` ([Tasks.org](https://tasks.org/docs/manual_sort_mode)). Do not use JMAP
Tasks: the IETF draft is expired
([IETF](https://datatracker.ietf.org/doc/draft-ietf-jmap-tasks/)). Move old `x` lines to a done file
so the working file stays short
([README](https://raw.githubusercontent.com/todotxt/todo.txt/master/README.md)).

### Notes

- Focus the last-edited note on open, and resume it by default
  ([Quick Note](https://support.apple.com/guide/notes/create-a-quick-note-apdf028f7034/mac)).
- Autosave with a debounce and flush synchronously on close: end-4 uses 500 ms, Plasma 10 s plus
  state changes, Sticky 3 s
  ([NotesContent.qml](https://github.com/end-4/dots-hyprland/blob/2f2a376fdb2ad738bf55f693dd0b474b64bb0ab8/dots/.config/quickshell/ii/modules/ii/overlay/notes/NotesContent.qml#L251),
  [Plasma](https://github.com/KDE/kdeplasma-addons/blob/acdd682885fa904d0dd2c2813cdc492561b90b82/applets/notes/qml/main.qml#L83),
  [Sticky](https://github.com/linuxmint/sticky/blob/0e780c2c9ad847e88d08a0d3fa4cd4679202d513/usr/lib/sticky/common.py#L11)).
- Check the file's mtime before overwriting, and refuse to overwrite an unparsable file. DMS autosave
  overwrote an externally modified file
  ([#2631](https://github.com/duxlabs-dev/dux-shell/issues/2631)), and end-4 parses with a bare
  `JSON.parse` that crashes on bad input
  ([Todo.qml](https://github.com/end-4/dots-hyprland/blob/2f2a376fdb2ad738bf55f693dd0b474b64bb0ab8/dots/.config/quickshell/ii/services/Todo.qml#L73)).
- Storage: one scratch file plus dated `YYYY-MM-DD.md` files, the Obsidian default
  ([Obsidian](https://github.com/obsidianmd/obsidian-help/blob/master/en/Plugins/Daily%20notes.md)).

| Format | For | Against |
| --- | --- | --- |
| Markdown files (recommended) | Any editor opens the folder; matches Drafts and Tot ([Iconfactory](https://blog.iconfactory.com/2020/02/meet-tot-your-tiny-text-companion)) | No per-note metadata such as color; ids are filenames |
| Plain `.txt` per note | Plasma Notes survives widget deletion ([filesystemnoteloader.cpp](https://github.com/KDE/kdeplasma-addons/blob/acdd682885fa904d0dd2c2813cdc492561b90b82/applets/notes/filesystemnoteloader.cpp#L35)) | Same as above; the id-to-file mapping orphaned notes ([Manjaro forum](https://forum.manjaro.org/t/howto-recover-sticky-notes-on-the-desktop/164845)) |

### Stickies

- Render stickies as cards inside the board's surface, so position, overlap and tilt need no
  protocol support
  ([xdg-shell XML](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/raw/main/stable/xdg-shell/xdg-shell.xml)).
- Color per sticky, collapse to the first line, and a "use as default" template
  ([Apple](https://support.apple.com/guide/stickies/welcome/mac)). Pin to top
  ([Keep](https://support.google.com/keep/answer/6191044?hl=en&co=GENIE.Platform%3DDesktop)).
- Give each widget an in-session undo stack and a 30-day trash like Drafts
  ([Drafts](https://docs.getdrafts.com/drafts)).
- Storage: private JSON modeled on Mint Sticky (text, color, created, optional expiry, pinned,
  debounced saves, timed backups)
  ([common.py](https://raw.githubusercontent.com/linuxmint/sticky/master/usr/lib/sticky/common.py)).
  Other tools cannot read it, so offer Markdown export
  ([Apple](https://support.apple.com/guide/stickies/welcome/mac)). Position, tilt and overlap belong
  to the board's layout file [INFERENCE from the ownership rule].

### Status cards

- **Tickets, not editors.** Interactions that survive across platforms are toggle, step and open
  ([Apple](https://developer.apple.com/documentation/widgetkit/adding-interactivity-to-widgets-and-live-activities)).
  DMS's richest cards, the ones with editors, generate its keyboard-navigation bugs
  ([DMS #3997](https://github.com/duxlabs-dev/dux-shell/issues/3997)).

## Board layout

- **Grid.** Cards get integer spans on a snapped grid, with the layout stored as `{card_id, col,
  row, w, h}` per output key. DMS stores integer `w`/`h` and packs cards in order, which survives
  resolution and column-count changes
  ([GridLayout.js](https://github.com/duxlabs-dev/dux-shell/blob/40a129c9d6f75566073d1a5eb1d4fb3ca48a79e4/quickshell/Common/GridLayout.js#L95),
  [cards.js](https://github.com/duxlabs-dev/dux-shell/blob/40a129c9d6f75566073d1a5eb1d4fb3ca48a79e4/quickshell/Modules/DDash/utils/cards.js#L23)).
  end-4's absolute pixels are tied to one resolution
  ([Persistent.qml](https://github.com/end-4/dots-hyprland/blob/2f2a376fdb2ad738bf55f693dd0b474b64bb0ab8/dots/.config/quickshell/ii/modules/common/Persistent.qml#L86)).
  Clamp on load and flow-pack cards that no longer fit [INFERENCE].
- **Fixed positions.** Never reorder by recency or relevance
  ([Findlater & McGrenere](https://dl.acm.org/doi/10.1145/985692.985704)). Per-output layouts need
  hotplug handling: Noctalia keys layouts by output name, and Rainmeter picks monitors with `@N`
  ([Noctalia](https://github.com/noctalia-dev/noctalia/blob/a531428700c0c06ce68be3f1f80ed6590d3f4034/docs/user/desktop/widgets.mdx),
  [Rainmeter](https://docs.rainmeter.net/manual/settings/skin-sections)). Fall back to a template
  layout for an unknown output [INFERENCE].
- **Edit mode.** Entry has to be visible and explicit, with Save and Discard like PowerToys' Edit
  Dock ([Dock](https://learn.microsoft.com/en-us/windows/powertoys/command-palette/dock)). Snap on
  drag with a modifier to bypass it, as Noctalia (Shift) and Rainmeter (Ctrl) do
  ([Noctalia](https://github.com/noctalia-dev/noctalia/blob/a531428700c0c06ce68be3f1f80ed6590d3f4034/docs/user/desktop/widgets.mdx),
  [Rainmeter](https://docs.rainmeter.net/manual/settings/skin-sections)). Add keyboard card
  movement: DMS has directional neighbor navigation and a bug in it
  ([#3997](https://github.com/duxlabs-dev/dux-shell/issues/3997)). The default state has no drag
  handles, so typing never moves a card
  ([Plasma commit](https://invent.kde.org/plasma/plasma-desktop/-/commit/ff32d904f79858cac4a87c149cda8e768283bda2)).
- **Removing a card.** Do not delete its data. Offer it back from a "removed cards" list
  ([Plasma source](https://invent.kde.org/plasma/kdeplasma-addons/-/raw/10b5f2450159612bae03fd854f29a9da386208ca/applets/notes/package/contents/ui/main.qml))
  [INFERENCE].

## Failure modes to avoid

| Failure | Seen in | Guard |
| --- | --- | --- |
| Fires on a chord (`Super+Return`) | One Hyprland user report ([#15568 comment](https://github.com/hyprwm/Hyprland/pull/15568#issuecomment-5230819813)); KDE "Both Shifts" layout switch ([bug 489001](https://bugs.kde.org/show_bug.cgi?id=489001)); sway mouse chords ([#4505](https://github.com/swaywm/sway/issues/4505)) | Test chord, drag and scroll on both compositors; keep an F13 fallback |
| Invisible surface left mapped | DMS ([#3802](https://github.com/duxlabs-dev/dux-shell/issues/3802), [#3822](https://github.com/duxlabs-dev/dux-shell/issues/3822)) | Destroy on close or set an empty input region; test with reduced motion |
| Positions lost or scrambled | Wayland sticky apps ([xdg-shell XML](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/raw/main/stable/xdg-shell/xdg-shell.xml)); end-4 pinned widget ([#3455](https://github.com/end-4/dots-hyprland/issues/3455)) | Draw stickies inside the board; grid units, not pixels |
| No text input | WidgetKit, RemoteViews, Windows guidance ([Apple](https://developer.apple.com/documentation/widgetkit/swiftui-views.md), [Android](https://developer.android.com/reference/android/widget/RemoteViews), [Microsoft](https://learn.microsoft.com/en-us/windows/apps/design/widgets/widgets-interaction-design)) | Native GTK text widgets in cards |
| Notes orphaned from their widget | Plasma UUID files ([Manjaro forum](https://forum.manjaro.org/t/howto-recover-sticky-notes-on-the-desktop/164845)) | Board stores a card id and placement only; removing a card keeps its data |
| Notes vanish when the app is closed | macOS Stickies ([guide](https://support.apple.com/guide/stickies/welcome/mac)) | Cards live in the shell, one key away |
| Feed, hover-open and badges | Windows 11 Widgets ([Windows Latest](https://www.windowslatest.com/2026/05/03/microsoft-is-finally-turning-off-the-msn-feed-and-ads-in-windows-11-widgets-by-default)) | No feed; no hover trigger; no badges |

## Unverified

- Hyprland v0.57.0 bare-modifier behavior: untested. The only evidence is a 0.56.2 comment on
  [#6946](https://github.com/hyprwm/Hyprland/issues/6946), and the chord-firing report
  ([#15568 comment](https://github.com/hyprwm/Hyprland/pull/15568#issuecomment-5230819813)) is a
  single user's, with its fix in [#15904](https://github.com/hyprwm/Hyprland/pull/15904).
- The release date of the niri version that will contain
  [#3621](https://github.com/niri-wm/niri/pull/3621).
- Whether niri's `Mod+scroll` cancels a pending release bind. The docs name keys and mouse buttons
  only ([docs](https://github.com/niri-wm/niri/blob/main/docs/wiki/Configuration:-Key-Bindings.md)).
- End-to-end IME typing into a GTK4 entry on a niri or Hyprland layer surface.
- The exact keyd, kanata and xremap config lines. They are composed from documented primitives and
  were not run.
- Whether layer-shell input in the board survives niri's overview in practice: source reading says
  yes, and no run was done
  ([niri.rs](https://github.com/niri-wm/niri/blob/main/src/niri.rs)).
- Windows Sticky Notes on-disk format and macOS Stickies database location.
- The `t:` start-date convention in todo.txt.
- Findlater 2004's preference split and "20% faster" figure: the converted PDF is garbled
  ([PDF](https://www.cs.ubc.ca/labs/imager/tr/2004/findlater04menus/findlater04menus.pdf)).
- Search gap: no peer-reviewed study of desktop or home-screen widget use, and none of
  command-palette or launcher usage, was found. The companion note's unverified figures still stand
  ([companion](desktop-space-research.md)).
