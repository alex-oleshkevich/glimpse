# Serve the Settings portal from glimpse-settings-daemon

| | |
| --- | --- |
| Bead | **glimpse-hb0dz** (spike). Related: glimpse-khzm8 (shipped portal configs), glimpse-qtc44 (accessibility needs `contrast`) |
| Status | Final; decisions recorded in §10. Not sliced. |
| Rule | Every external fact is tagged **measured** (command run on this machine, 2026-10-26), **source** (upstream file read at the tag named, path given) or **assumed** (documentation or inference, URL or reason given). Local claims carry `file:line` at HEAD `a397149c`. |

## 1. Goal

Today the desktop color scheme reaches apps only if a GNOME-family portal backend is installed and
selected: glimpse-settings-daemon writes GSettings, and `xdg-desktop-portal-gnome` or `-gtk` republishes
the key on `org.freedesktop.portal.Settings`. After this change:

- `glimpse-settings-daemon` serves `org.freedesktop.impl.portal.Settings` itself and owns four
  `org.freedesktop.appearance` keys: `color-scheme`, `accent-color`, `contrast`, `reduced-motion`.
- The portal frontend lists it first, with `gtk` behind it for every namespace glimpse does not own
  (`org.gnome.desktop.*`, fontconfig).
- The GSettings write of `color-scheme` stays, for readers that never ask the portal.
- Apps (GTK4/libadwaita, Qt 6.5+, Firefox, Electron) see a color scheme, accent, contrast and
  reduced-motion value that come from `config.toml`, a temporary hold, or the sun, whether or not a
  portal backend of another desktop runs.

```text
 today                                          after
 settings-daemon ──GSettings──► dconf           settings-daemon ──GSettings (color-scheme only)──► dconf ──► GTK3, no-portal readers
                                  │                  │
                                  ▼                  └─ impl.portal.Settings ──┐
                       portal-gnome / -gtk                                      ▼
                                  │                  xdg-desktop-portal  ◄── gtk (+gnome): org.gnome.desktop.*, fonts, cursor
                                  ▼                         │
                      portal.Settings ──► apps              ▼  portal.Settings ──► apps
```

## 2. Current state (tree at `a397149c`)

| What | Where |
| --- | --- |
| Settings daemon owns `me.aresa.Glimpse.Settings`, serves `me.aresa.Glimpse.Settings1` at `/me/aresa/Glimpse/Settings`: properties `Mode`, `Geolocation`, `Solar`; methods `SetTemporaryMode(u)`, `Resume()` | `crates/glimpse-settings-daemon/src/provider.rs:25-91`, `GLIMPSE_SETTINGS_*` consts in `glimpse-dbus::settings` |
| Startup order: actions served, backends built, GSettings observer started and awaited (`services.rs:53`), then the name is acquired (`services.rs:54-56`), then services start. A second instance fails at the name and exits before it writes anything | `services.rs:22-70`; `crates/glimpse-dbus/src/provider.rs:27-48`, `dbus.rs:68-73` (`DoNotQueue`); `crates/glimpse-settings-daemon/README.md` (second paragraph) |
| Desktop mode writes `org.gnome.desktop.interface color-scheme` on a dedicated GLib thread, flushes (`gio::Settings::sync`) and reads back; refuses when the key is not writable | `crates/glimpse-settings-daemon/src/mode.rs:7-8, 42-95` (write `:62-77`), `settings()` `:107-125` |
| Mode policy, holds: target = hold, else policy (`sun` phase, fixed, `external` restore); `requested` is that target, `observed` is what GSettings holds | `crates/glimpse-services/src/services/desktop_mode.rs:267-284`; payload `ModeStatus {policy, requested, observed, temporary}` `crates/glimpse-dbus/src/clients/settings.rs:44-50` |
| `Mode::code()` is **Light = 1, Dark = 2**. The portal numbers them the other way round (1 dark, 2 light) | `crates/glimpse-dbus/src/clients/settings.rs:26-32` |
| Config reload reaches the daemon: `Reload::Changed` → `services.reconfigure`; a rejected file keeps the last valid config | `crates/glimpse-settings-daemon/src/main.rs:60-66`, `services.rs:72-75` |
| `[appearance]`: `theme`, `theme-variant`, `color-scheme` (glimpse's own mode: `desktop`/`light`/`dark`), `desktop-color-scheme` (`sun`/`light`/`dark`/`external`), `blur`, `animation-speed` (0 or 0.1..10, default 1.0). **No accent, no contrast, no reduced-motion field** | `crates/glimpse-config/src/schema/appearance.rs:4-46`, `:70-77`; root field `schema/mod.rs:71` |
| Glimpse takes its accent from libadwaita, not from a theme: `adw::StyleManager::accent_color_rgba()`; themes are CSS only and carry no accent | `HEAD:crates/glimpse-agents/src/dialog/imp.rs:206`, `HEAD:crates/glimpse-panel/src/applets/exec/dom.rs:97`; `data/themes/adwaita/*.css` (no accent rule); `HEAD:crates/glimpse-widgets/styles/glimpse.css:22` (`var(--accent-color, #80d0ff)`) |
| `animation-speed` becomes `--gl-duration` (0 → `0ms`) and nothing else; CSS honors `prefers-reduced-motion` independently | `HEAD:crates/glimpse-widgets/src/theme.rs:36-52, 188-193`; `HEAD:crates/glimpse-widgets/styles/glimpse.css:767, 1622, 1769, 1996` |
| glimpse's own mode follows the desktop through libadwaita: `ColorScheme::Desktop` → `adw::ColorScheme::Default` | `HEAD:crates/glimpse-widgets/src/theme.rs:28-34`; docs `website/start/first-session.md:90-93` ("follows it through `xdg-desktop-portal` and a settings backend") |
| Shipped portal files: `glimpse.portal` (Idle, `Inhibit`), `glimpse-shot.portal` (`Screenshot`), `glimpse-wallpaper.portal` (`Wallpaper`), all `UseIn=glimpse` | `data/portals/*.portal` |
| Shipped preferences: three byte-identical files `glimpse-portals.conf`, `niri-portals.conf`, `hyprland-portals.conf`: `Inhibit=glimpse`, `Screenshot=glimpse-shot`, `Wallpaper=glimpse-wallpaper`, no `default` | `data/portals/*-portals.conf` |
| Packaging: `.portal` → `usr/share/xdg-desktop-portal/portals/`; the three confs → `etc/xdg-desktop-portal/` (deb conffiles) plus a pristine copy in `usr/share/glimpse/portals/`; rpm and openSUSE variant the same; `install.sh` replaces an edited conf only if it still matches the pristine copy; Nix sets `xdg.portal.config.{glimpse,niri,hyprland}` | `crates/glimpse-package/Cargo.toml:85-89, 119-121, 175-179, 254-258`; `scripts/install.sh:38-50`; `nix/module.nix:25-30`, `nix/package.nix:112-114` |
| Guards that forbid a `default` in our confs | `crates/glimpse-package/tests/packaging.rs:296, 365`; `nix/test.nix:39` |
| Routing check script uses `XDG_DESKTOP_PORTAL_DIR`, which makes the portal ignore every other config directory | `scripts/portal-routing.sh:36`, expectation loop `:41` |
| `glimpsectl session declare` rewrites `XDG_CURRENT_DESKTOP=niri` to `glimpse:niri` in the systemd and D-Bus activation environments and restarts `xdg-desktop-portal.service`; Hyprland is left alone | `crates/glimpsectl/src/commands/session.rs:12, 29-46`, `data/systemd/glimpse-desktop.service:11`, `crates/glimpsectl/README.md:111-113` |
| Settings unit: `Type=dbus`, `BusName=me.aresa.Glimpse.Settings`, `Requisite=glimpse-session.target`, `Restart=on-failure`, `RestartSec=2`; activation file runs the same unit; the session target `Wants=` it | `data/systemd/glimpse-settings-daemon.service:1-12`, `data/dbus-1/services/me.aresa.Glimpse.Settings.service:1-4`, `data/systemd/glimpse-session.target:6` |
| Pattern for an `impl.portal.*` server in glimpse: a second bus name beside the provider's, object at `/org/freedesktop/portal/desktop`, a `.portal` file, an activation `.service` file naming the owning systemd unit. Idle: interface `portal.rs:24`, name acquired with a degraded fallback `:255-281`; Shot: `HEAD:crates/glimpse-shot/src/portal.rs:20, 78, 452`; Wallpaper: `crates/glimpse-wallpaper/src/portal.rs:15, 185-186, 213`; activation file `data/dbus-1/services/me.aresa.Glimpse.Idle.Portal.service`, `…Wallpaper.Portal.service` | `crates/glimpse-idle/src/inhibitors/portal.rs` |
| Hand-written provider objects use `Exported::start(connection, name, path, interface, follow)` | `crates/glimpse-dbus/src/provider.rs:18-48`; a follower that re-emits on change `crates/glimpse-settings-daemon/src/provider.rs:100-146` |
| Private-bus fixture; daemon service tests | `crates/glimpse-dbus/src/testing/mod.rs:12-30`, `crates/glimpse-settings-daemon/src/services/tests.rs` |
| Config files are written by tools through `writable_file()` and `write_table()` (glimpse-shot's preferences write `[screenshot]`) | `crates/glimpse-config/src/load.rs:120-122, 149-156`; `HEAD:crates/glimpse-shot/src/gui/app.rs:920` |
| User docs that name the dependency | `website/start/first-session.md:90-93`, `website/config/services/appearance.md:40`, `website/developers/files.md:34-45` |

## 3. Facts

### 3.1 Versions and live state

| Fact | Tag |
| --- | --- |
| `xdg-desktop-portal 1.22.1`, `xdg-desktop-portal-gtk 1.15.3`, `xdg-desktop-portal-gnome 51.0` (`/usr/lib/xdg-desktop-portal --version`; `pacman -Q …`). Session bus is `dbus-broker` | measured 2026-10-26 |
| The portal process runs with `XDG_CURRENT_DESKTOP=glimpse:niri` (`/proc/<pid>/environ`) | measured |
| Live `ReadAll(["org.freedesktop.appearance"])` through the frontend: `contrast=0u, color-scheme=1u, reduced-motion=0u, accent-color=(0.2078,0.5176,0.8941)` (`gdbus call --session -d org.freedesktop.portal.Desktop -o /org/freedesktop/portal/desktop -m org.freedesktop.portal.Settings.ReadAll '["org.freedesktop.appearance"]'`) | measured |
| Per backend: **gnome** returns all four keys; **gtk** returns only `contrast` and `color-scheme` (`busctl --user call org.freedesktop.impl.portal.desktop.{gnome,gtk} /org/freedesktop/portal/desktop org.freedesktop.impl.portal.Settings ReadAll as 1 org.freedesktop.appearance`). So today the accent and reduced-motion come only from gnome | measured |
| Both backends serve the same non-appearance namespaces from GSettings: `org.gnome.desktop.{privacy,interface,sound,peripherals.mouse,calendar,wm.preferences}` and `org.gnome.fontconfig`; they accept `*` as a pattern (`ReadAll as 1 '*'`) | measured |
| Both backends expose `ReadAll(as)→a{sa{sv}}`, `Read(ss)→v`, signal `SettingChanged(ssv)`, property `version=0` (`busctl --user introspect … org.freedesktop.impl.portal.Settings`); the frontend's own `version` is 2 | measured; source `src/settings.c:298` |
| `gsettings get org.gnome.desktop.interface accent-color` → `'blue'`; range is the nine names `blue teal green yellow orange red pink purple slate`; `org.gnome.desktop.a11y.interface` has `high-contrast` (bool) and `reduced-motion` (`no-preference`/`reduce`); `color-scheme` range `default prefer-dark prefer-light` | measured |
| glimpse's daemon right now: `Mode = (true, "sun", requested 2, observed 2, temporary 0, "")` (`busctl --user get-property me.aresa.Glimpse.Settings /me/aresa/Glimpse/Settings me.aresa.Glimpse.Settings1 Mode`) — Dark (2) in glimpse codes is portal `1`, matching the live `color-scheme=1` | measured |

### 3.2 How the frontend merges backends (xdg-desktop-portal 1.22.1)

| Fact | Tag |
| --- | --- |
| `ReadAll`: every backend is called, in **reverse** list order, and keys are inserted into a dict per namespace, so the **first listed backend wins each key** and later ones only fill keys it lacks; a failing backend is skipped with a warning. `Read`/`ReadOne`: backends are tried in list order, first success wins; every error is treated as "not found, continue"; all failing returns `org.freedesktop.portal.Error.NotFound`. Namespace patterns are passed to backends untouched | source `src/settings.c:64-100, 128-238` (tag 1.22.1) |
| `SettingChanged` from **every** backend is re-emitted unchanged: no precedence, no de-duplication, no check that the emitting backend still wins the key | source `src/settings.c:240-250, 300-306` |
| The backend list for `Settings` is `xdp_portal_config_find_all`: per config file (highest precedence first) the interface-specific list is taken and, only if it yielded backends, the file's `default` is skipped; later config files still contribute. A `none` in a list drops the interface entirely. A backend listed but not installed is skipped. If nothing at all matched, `-gtk` is the last resort | source `src/xdp-portal-config.c:927-1004`, `:630-675` |
| Config files are **layered**: every search location contributes at most one file (the first existing `<desktop>-portals.conf` for the names in `XDG_CURRENT_DESKTOP`, else `portals.conf`) and `xdp_portal_config_find`/`find_all` walk all of them. Locations in order: `$XDG_CONFIG_HOME`, `$XDG_CONFIG_DIRS` (default `/etc/xdg`), `/etc`, `$XDG_DATA_HOME`, `$XDG_DATA_DIRS`, `/usr/share`. `XDG_DESKTOP_PORTAL_DIR` replaces all of them with one directory | source `src/xdp-portal-config.c:466-590`; doc `doc/portals.conf.rst.in:17-78` |
| Layering is real on this machine: with `XDG_CURRENT_DESKTOP=glimpse:niri` and our shipped files installed, a private-bus portal logs `Using portal configuration file '/etc/xdg-desktop-portal/glimpse-portals.conf' for desktop 'glimpse'` **and** `…'/usr/share/xdg-desktop-portal/niri-portals.conf' for desktop 'niri'`, then `Settings → gnome.portal, gtk.portal`, `Access → gtk`, `Notification → gtk`, `Secret → gnome-keyring`, `FileChooser/ScreenCast/… → gnome`, `Inhibit → glimpse`, `Screenshot → glimpse-shot`, `Wallpaper → glimpse-wallpaper` (`XDG_CURRENT_DESKTOP=glimpse:niri dbus-run-session -- /usr/lib/xdg-desktop-portal -r -v`; the live session is untouched). `/etc/xdg-desktop-portal/niri-portals.conf` is **not read** in that session: per directory only the first desktop name that has a file counts (`glimpse-portals.conf` here) | measured |
| Layering began in 1.17.0, backend order is respected from 1.19.0, `ReadOne` and `accent-color` date from 1.17.1, `reduced-motion` from 1.21.0 | source `NEWS.md` (tag 1.22.1) |
| A backend that is not running when the portal starts does **not** make the frontend drop it: a portal on a private bus with `Settings=ghost;gtk` where `ghost`'s name had no owner logged `Providing portal org.freedesktop.portal.Settings`, no proxy error | measured (private bus) |
| Late start, precedence, forwarding, disappearance (python/Gio fakes `A` and `B` on a private bus, list `A;B`, `A={color-scheme 2, accent}`, `B={color-scheme 1, contrast 1, cursor-size}`): only `B` running → `ReadAll` returns `B`'s values and `ReadOne accent-color` is `NotFound`; `A` starts later with no portal restart → `ReadAll` is `{contrast 1 (B), color-scheme 2 (A), accent (A)}`, `ReadOne color-scheme` is 2; `A` and `B` each emit `color-scheme` and `B` emits `accent-color` → the client sees **three** `SettingChanged`, including `B`'s accent although `A` owns that key; `A` leaves → `ReadAll` returns `B`'s values; `A` returns → 2 again | measured |
| A backend that emits `SettingChanged` from inside its `name_acquired` callback is seen by a subscribed client 8 of 8 times (no race between the owner change and the first signal) on `dbus-daemon`; the live bus is `dbus-broker`, so AC-8 repeats it live | measured (private `dbus-daemon`) |
| Impl interface: `ReadAll(as)` — empty array or an empty string matches all, globbing is trailing only (`org.example.*`); `Read(ss)→v` errors on unknown namespace or key; `SettingChanged(ssv)`; `version` property | source `data/org.freedesktop.impl.portal.Settings.xml` (tag 1.22.1) |
| Key semantics: `color-scheme u` 0 none / 1 dark / 2 light; `accent-color (ddd)` sRGB 0..1, **out of range means unset**; `contrast u` 0 normal / 1 higher; `reduced-motion u` 0 none / 1 reduce; unknown values are 0. Other keys are implementation details | source same file, `data/org.freedesktop.portal.Settings.xml` |
| libadwaita maps the portal's `accent-color` to the **nearest of its nine colors** (OKLCH hue buckets, chroma below 0.04 → slate); the nine are `#3584e4 #2190a4 #3a944a #c88800 #ed5b00 #e62d42 #d56199 #9141ac #6f8396` (blue, teal, green, yellow, orange, red, pink, purple, slate). The gnome backend's output for `'blue'` equals `#3584e4` | source libadwaita `src/adw-accent-color.c:16-24, 159-` (main); measured for blue |
| libadwaita reads `high-contrast` and the accent through its portal settings backend when a portal is reachable and falls back to GSettings otherwise | assumed (`src/adw-settings.c`, backend selection; not read to the end) |
| GTK3 and any app that never calls the portal read dconf only | assumed (GTK3 has no portal settings code) |
| `dbus-broker` holds the call that triggered an activation until the service takes the name or the activation fails | assumed (`dbus-broker` activation semantics) |

## 4. Decisions

| # | Decision | Reason |
| --- | --- | --- |
| D1 | Serve `org.freedesktop.impl.portal.Settings` from `glimpse-settings-daemon` on a new bus name `me.aresa.Glimpse.Settings.Portal`, object `/org/freedesktop/portal/desktop`, backend name **`glimpse-settings`** (file `glimpse-settings.portal`). Not `glimpse` | A backend is a `.portal` file and carries one `DBusName`; `glimpse.portal` is Idle's name (`data/portals/glimpse.portal`). A second name beside the provider's follows Idle, Shot and Wallpaper (§2 pattern row) |
| D2 | **A key is published only when glimpse has an opinion; otherwise it is absent** and the frontend fills it from the next backend (§3.2 merge). `Read` of an absent key is `org.freedesktop.portal.Error.NotFound` | Measured merge gives per-key fallback for free; glimpse never overrides a GNOME-side setting it was not asked to own |
| D3 | `color-scheme`: always published when a mode is known: `temporary`, else `requested`, else `observed`; Dark → 1, Light → 2. Unknown → absent | `requested` is the daemon's decision, so the portal keeps working when dconf is missing or read-only (`mode.rs:64-66`); `observed` covers `external` policy where another tool decides. The number swap against `Mode::code()` is named in §5.4 |
| D4 | `accent-color`: new `[appearance] accent-color`, a string: empty (default) = unpublished, a GNOME color name, or `#rrggbb`. Published as `(r/255, g/255, b/255)` | No accent source exists today (§2). `theme` is CSS and cannot be read by a daemon. Names mirror GSettings and libadwaita; hex lets Qt, Firefox and Chromium show any color. Libadwaita snaps hex to the nearest of nine (§3.2), stated in the field's doc comment |
| D5 | `contrast`: new `[appearance] contrast = "desktop" \| "normal" \| "high"`, default `desktop` (unpublished). `normal` publishes 0, `high` publishes 1 | `qtc44` names settings-daemon as the owner. `desktop` keeps today's behavior (the gnome backend's `high-contrast` GSettings key) for users who never touch it. Same word as `color-scheme = "desktop"` (`appearance.rs:51-55`) |
| D6 | `reduced-motion`: published as 1 **only** while `animation-speed == 0`; otherwise unpublished | One knob already means "animations off" (`appearance.rs:18-19`); publishing 0 would override a GNOME `reduced-motion` the user set. Decided with the user (Q2 (a)); the side effect on other apps is stated in the field's doc comment |
| D7 | The GSettings write of `color-scheme` is **permanent** and unchanged (`mode.rs`). Nothing else is written to GSettings | GTK3 and non-portal readers need it (§3.2 assumed row); it is the fallback the frontend serves while the daemon is down (AC-10); GSettings `accent-color` is a nine-name enum that cannot carry hex; `contrast` and `reduced-motion` have no non-portal reader in glimpse's scope |
| D8 | Preference line: `org.freedesktop.impl.portal.Settings=glimpse-settings;gtk`. The resulting chain on niri is `glimpse-settings, gtk, gnome` (gnome appended by the layer below, §3.2); on Hyprland `glimpse-settings, gtk` | `gtk` is listed explicitly so a session whose desktop name has no compositor file below (nothing else names a backend) still gets fonts, cursor and `org.gnome.desktop.*`. Dropping `gnome` from the chain is impossible with `none` (it drops the whole interface, §3.2) and unnecessary: both backends serve the same GSettings namespaces (§3.1), and gnome only fills `accent-color`/`reduced-motion` when glimpse leaves them unpublished |
| D9 | The daemon serves **only** `org.freedesktop.appearance`. It does not mirror `org.gnome.desktop.*`, fonts or cursor | They are GSettings reads the gtk backend already does (§3.1); copying them means owning a second view of dconf with no policy of our own. Rejected below |
| D10 | `ReadAll` matches a namespace pattern if the list is empty, an entry is empty, an entry equals the namespace, an entry is `*`, or an entry ends in `.*`/`*` and the namespace starts with the part before the `*`. At most 64 patterns of at most 256 bytes are examined; the rest are ignored. App-supplied strings are compared, never logged or rendered | Impl XML (§3.2) plus the `*` the real backends accept (§3.1); AGENTS.md:47 (text from other apps is hostile) |
| D11 | The daemon acquires `me.aresa.Glimpse.Settings.Portal` **after** `me.aresa.Glimpse.Settings` and after the first mode observation, then emits `SettingChanged` for every published key once. If the portal name is owned by another process it logs a warning and the daemon runs without the portal (Settings1 and the GSettings write continue), the way Idle degrades (`crates/glimpse-idle/src/inhibitors/portal.rs:266-280`) | The main name already makes a second daemon exit (`services.rs:54-56`). Emitting after acquiring the name was delivered 8/8 (§3.2); the frontend never replays state when a backend appears, so apps that cached the gnome/gtk value learn ours from this signal |
| D12 | When a published key becomes unpublished (accent cleared, `contrast` back to `desktop`, `animation-speed` above 0, mode unknown) the daemon emits `SettingChanged` with the neutral value: `color-scheme` 0, `contrast` 0, `reduced-motion` 0, `accent-color` `(-1,-1,-1)` (out of range = unset) | The interface has no "removed" signal and the frontend forwards ours verbatim (§3.2). An app then returns to defaults at once; apps pick up the next backend's value on their next `ReadAll` or restart. Accepted limitation |
| D13 | A shadowed backend's `SettingChanged` still reaches apps (§3.2 measured). Accepted; no filtering in glimpse | Only happens when the user changes the same key through another tool; for `color-scheme` the dual write keeps both values equal (D7). The case worth stating: `gsettings set … accent-color` while glimpse publishes an accent makes apps show the GSettings color until the next read |
| D14 | D-Bus activation: `data/dbus-1/services/me.aresa.Glimpse.Settings.Portal.service` with `Exec=/usr/bin/glimpse-settings-daemon` and `SystemdService=glimpse-settings-daemon.service`, like Idle's (`me.aresa.Glimpse.Idle.Portal.service`) | A frontend call to a stopped backend starts it (the proxy is created without `DO_NOT_AUTO_START`, source `settings.c:333-339`). Outside a glimpse session the unit's `Requisite=glimpse-session.target` makes the start fail and the frontend moves on to `gtk` (AC-9) |
| D15 | No D-Bus method to change contrast. The accessibility plan toggles it by writing `[appearance] contrast` with `writable_file("config")` + `write_table`, the way glimpse-shot's preferences do (§2); the reload path applies it (`main.rs:60-64`) | Persistence is the point of a contrast toggle; a hold like `SetTemporaryMode` is not persisted (`README` of the daemon). Decided with the user (Q3 (a)) |
| D16 | No hard package dependency on a minimum portal version. Layering needs 1.17, ordered lists 1.19, `reduced-motion` 1.21 (§3.2); older frontends ignore a backend they cannot place and fall back to GSettings through gtk/gnome | A missing key is "no preference" for every client (impl XML) |
| D17 | Failure handling: portal not up when the daemon starts → nothing to do (the frontend creates its proxies without requiring an owner, §3.2); daemon down when the portal starts → apps see gtk/gnome values until the daemon appears, then ours with no portal restart (§3.2); crash → the same, then `Restart=on-failure` after 2 s (`glimpse-settings-daemon.service:11-12`) and a startup emission (D11); portal restart (`glimpsectl session declare` does one, `session.rs:29-46`) → the frontend reconnects and re-lists backends | All measured in §3.2 |
| D18 | Code placement: pure derivation and the portal object in `crates/glimpse-settings-daemon/src/portal.rs` (+ `portal/keys.rs`, tests in sibling `portal/tests.rs`); config types in `glimpse-config`; no change to `glimpse-services` | The daemon already owns mode, solar and geolocation; the keys are a projection of `ModeStatus` plus three config values, not a service with a backend. No GTK or Wayland (AGENTS.md:44) |
| D19 | khzm8 interplay (decided, Q1 (a)): the premise "a first-found file shadows the compositor's lines" does not hold on 1.22.1 (§3.2 measured). Our confs keep only our own lines (§5.7); copying upstream lines is rejected (§9). The layering test in §5.6 guards it | Layering is the mechanism; a copied `default` would freeze lines the compositor package owns and would also be wrong in `glimpse-portals.conf`, which is compositor-agnostic and wins first on `glimpse:niri` |

## 5. Architecture

### 5.1 Who owns what

| State | Owner | Others |
| --- | --- | --- |
| `[appearance] accent-color`, `contrast`, `animation-speed`, `desktop-color-scheme` | the user's `config.toml`; written by the user or, for contrast, by an accessibility toggle through `write_table` | the daemon reads them on every reload (`main.rs:60-66`) |
| Desktop mode (`requested`, `observed`, hold) | `DesktopMode` service in the daemon (`desktop_mode.rs:267-284`) | portal derives from `ModeStatus`; GSettings adapter writes it (`mode.rs`) |
| The four appearance keys as seen by apps | the daemon (`Keys`, §5.4), derived, never stored | frontend merges them over gtk/gnome |
| `org.gnome.desktop.*`, fontconfig, any appearance key glimpse leaves unpublished | gtk / gnome backends, from GSettings | the frontend fills them behind ours |
| Glimpse's own look | libadwaita `StyleManager` fed by the frontend (`HEAD theme.rs:28-34`, `imp.rs:206`) | nobody in glimpse reads GSettings for it |

### 5.2 How the pieces connect

`config.toml` → `glimpse_config::watch_config` (`main.rs:41-46`) → `SettingsServices::reconfigure` (`services.rs:72-75`) updates the
`watch::Sender<Inputs>` held by the portal runtime; the `DesktopMode` handle's `subscribe()` (as used
by `provider.rs:105`) feeds the same `Inputs` with each `ModeStatus`. A single task selects over both,
calls `Keys::derive(&Inputs)`, and when the result differs from the last emitted `Keys` it emits one
`SettingChanged` per differing key through the object's `SignalEmitter` (the follower shape of
`provider.rs:108-133`, started by `Exported::start`, `glimpse-dbus/src/provider.rs:27-48`). The same
`Keys` value, read from a `watch::Receiver<Keys>`, answers `ReadAll` and `Read`. The GSettings write
path (`mode.rs`) is untouched and still driven by `DesktopMode`, so the portal and GSettings move from
one decision.

```text
 config.toml ─► Inputs{accent,contrast,animations_off} ┐
                                                       ├─► Keys::derive ─► watch<Keys> ─► ReadAll/Read
 DesktopMode.subscribe() ─► Inputs.mode (ModeStatus) ──┘          │
        │                                                         └─► diff ─► SettingChanged
        └─► ModeWriter ─► gio::Settings color-scheme (dconf)
```

### 5.3 Config (`[appearance]`)

Declared like the existing fields (`crates/glimpse-config/src/schema/appearance.rs:4-23`; a plain `String` with an empty default is the shape of `theme_variant`, `:8-9, :29`; a three-way enum is the shape of `DesktopColorScheme`, `:70-77`). Owner: the user's `config.toml`; reader: `glimpse-settings-daemon` only.

```toml
[appearance]
accent-color = ""          # "", "blue" | "teal" | "green" | "yellow" | "orange" | "red" | "pink" | "purple" | "slate", or "#rrggbb"
contrast = "desktop"       # "desktop" | "normal" | "high"
```

```rust
pub struct Appearance {
    // existing fields unchanged; the doc comment of `animation_speed` gains the line:
    //   /// With `0`, apps that follow your desktop also reduce their own animations.
    /// Choose the accent color for apps that follow your desktop: `blue`, `teal`, `green`, `yellow`, `orange`, `red`, `pink`, `purple`, `slate`, or a `#rrggbb` color.
    /// Leave it empty to keep your desktop's accent. GNOME apps use the nearest of the nine names.
    #[serde(deserialize_with = "accent")]
    pub accent_color: String,
    /// Choose `desktop` to keep your desktop's contrast setting, or `normal` or `high` to set it for every app that follows your desktop.
    pub contrast: Contrast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Contrast {
    Desktop,
    Normal,
    High,
}

pub const ACCENTS: [(&str, [u8; 3]); 9] = [
    ("blue", [0x35, 0x84, 0xe4]),
    ("teal", [0x21, 0x90, 0xa4]),
    ("green", [0x3a, 0x94, 0x4a]),
    ("yellow", [0xc8, 0x88, 0x00]),
    ("orange", [0xed, 0x5b, 0x00]),
    ("red", [0xe6, 0x2d, 0x42]),
    ("pink", [0xd5, 0x61, 0x99]),
    ("purple", [0x91, 0x41, 0xac]),
    ("slate", [0x6f, 0x83, 0x96]),
];

pub fn accent_rgb(value: &str) -> Result<Option<[u8; 3]>, String>
```

`Default` adds `accent_color: String::new()` and `contrast: Contrast::Desktop`. `accent` (the
deserializer, beside `speed` at `appearance.rs:38-46`) accepts `""`, a name in `ACCENTS`, or a
seven-character `#rrggbb`, and rejects anything else with "accent-color must be empty, a color name or
#rrggbb". `animation-speed` is reused for `reduced-motion` (D6) and has no new field. Generated
artifacts: `data/config.default.toml`, `data/config.commented.toml`, `data/config.schema.json`
(`just gen-config-schema config`, `just gen-config-commented config`, `justfile:69-75`).

### 5.4 Keys and the portal object

```rust
pub const APPEARANCE: &str = "org.freedesktop.appearance";
pub const PORTAL_BUS_NAME: &str = "me.aresa.Glimpse.Settings.Portal";
pub const PORTAL_OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Keys {
    pub color_scheme: Option<u32>,
    pub accent_color: Option<[f64; 3]>,
    pub contrast: Option<u32>,
    pub reduced_motion: Option<u32>,
}

pub struct Inputs {
    pub mode: ModeStatus,
    pub accent: Option<[u8; 3]>,
    pub contrast: Contrast,
    pub animations_off: bool,
}

impl Keys {
    pub fn derive(inputs: &Inputs) -> Self {
        let mode = inputs.mode.temporary.or(inputs.mode.requested).or(inputs.mode.observed);
        Self {
            color_scheme: mode.map(|mode| match mode { Mode::Dark => 1, Mode::Light => 2 }),
            accent_color: inputs.accent.map(|rgb| rgb.map(|c| f64::from(c) / 255.0)),
            contrast: match inputs.contrast {
                Contrast::Desktop => None,
                Contrast::Normal => Some(0),
                Contrast::High => Some(1),
            },
            reduced_motion: inputs.animations_off.then_some(1),
        }
    }
}

pub struct SettingsPortal {
    keys: watch::Receiver<Keys>,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Settings")]
impl SettingsPortal {
    #[zbus(property)]
    fn version(&self) -> u32 { 0 }
    async fn read_all(&self, namespaces: Vec<String>) -> HashMap<String, HashMap<String, OwnedValue>>;
    async fn read(&self, namespace: &str, key: &str) -> zbus::fdo::Result<OwnedValue>;
    #[zbus(signal)]
    async fn setting_changed(emitter: &SignalEmitter<'_>, namespace: &str, key: &str, value: Value<'_>) -> zbus::Result<()>;
}
```

Wire types: `color-scheme u`, `accent-color (ddd)`, `contrast u`, `reduced-motion u` (impl XML, §3.2).
`read` returns `org.freedesktop.portal.Error.NotFound` for any namespace other than `APPEARANCE` and
for an unpublished key. `ReadAll` returns an empty dict when nothing matches or nothing is published.
`version` is 0 like both real backends (§3.1). The mode-to-portal mapping is explicit in `derive`
because `Mode::code()` numbers the other way (`clients/settings.rs:26-32`). The neutral values of D12
are produced by the same diff: `old Some → new None` emits `0`, or `(-1,-1,-1)` for the accent.

### 5.5 Startup in `SettingsServices::start`

After the existing `provider::start` (`services.rs:54-56`) and `mode_pending.start` (`:57`), build the
`SettingsPortal` from `Inputs` and call `Exported::start(session, PORTAL_BUS_NAME, PORTAL_OBJECT_PATH,
portal, follow)`. On `Err` log and continue with `portal: None` (D11). `reconfigure`
(`:72-75`) also updates the `Inputs` sender. `shutdown` (`:83-91`) drops the portal runtime first, so the
name is released before services stop.

### 5.6 Packaging

New files, same pattern as the Wallpaper portal (`data/portals/glimpse-wallpaper.portal`,
`data/dbus-1/services/me.aresa.Glimpse.Wallpaper.Portal.service`):

```ini
# data/portals/glimpse-settings.portal
[portal]
DBusName=me.aresa.Glimpse.Settings.Portal
Interfaces=org.freedesktop.impl.portal.Settings
UseIn=glimpse
```

```ini
# data/dbus-1/services/me.aresa.Glimpse.Settings.Portal.service
[D-BUS Service]
Name=me.aresa.Glimpse.Settings.Portal
Exec=/usr/bin/glimpse-settings-daemon
SystemdService=glimpse-settings-daemon.service
```

The existing globs `data/portals/*.portal`, `data/dbus-1/services/*.service`
(`crates/glimpse-package/Cargo.toml:84-85, 174-175, 253-254`, `nix/package.nix:107-114`, `scripts/install.sh:38-40`) pick both up with no manifest edit.

Layering test. `scripts/portal-layers.sh <XDG_CURRENT_DESKTOP>` (new, beside `portal-routing.sh`):
it builds a scratch tree under `$XDG_RUNTIME_DIR` with `etc/` holding our three confs from
`data/portals/`, `usr/share/xdg-desktop-portal/` holding a copy of the **installed** compositor files
(`/usr/share/xdg-desktop-portal/{niri,hyprland,gnome,gtk}-portals.conf`) and every installed and shipped
`.portal` file, and starts `xdg-desktop-portal -v` on a private bus (`bus.conf` as in
`portal-routing.sh:14-34`) with `XDG_CONFIG_HOME`, `XDG_CONFIG_DIRS` and `XDG_DATA_*` pointed at the
scratch tree and **without** `XDG_DESKTOP_PORTAL_DIR` (which hides every layer, `portal-routing.sh:36`).
It parses the `XDP: Using <backend>.portal for <interface> (…)` lines and asserts, per interface,
the backend list in AC-17; the expectations for non-glimpse interfaces are read from the compositor's
own conf file at run time, never hard-coded, so a change upstream cannot silently pass. A conf of
ours that gains a `default` line makes it fail. It is wired into `just` beside the existing
`portal-routing.sh` recipe.

### 5.7 Resulting portal preferences

**Decided (Q1): our files keep only our own lines, plus the Settings line.** All three files stay
byte-identical, as today:

```ini
[preferred]
org.freedesktop.impl.portal.Inhibit=glimpse
org.freedesktop.impl.portal.Screenshot=glimpse-shot
org.freedesktop.impl.portal.Settings=glimpse-settings;gtk
org.freedesktop.impl.portal.Wallpaper=glimpse-wallpaper
```

Measured/derived effect on `glimpse:niri`: `Settings → glimpse-settings, gtk, gnome`; everything else
exactly as in §3.2. Hyprland (`XDG_CURRENT_DESKTOP=Hyprland`, `glimpsectl` leaves it alone,
`session.rs:29-46`): `/etc/xdg-desktop-portal/hyprland-portals.conf` is the first layer and
`/usr/share/xdg-desktop-portal/hyprland-portals.conf` (`default=hyprland;gtk`, present here) the second:
`Settings → glimpse-settings, gtk`. On Hyprland glimpse becomes the only source of `accent-color` and
`reduced-motion`, since gtk lacks both (§3.1). [assumed for Hyprland: the layering code is
compositor-independent; the Settings list there was not run live.]

`glimpse-portals.conf` cannot carry a `default`: it matches every `glimpse:<compositor>` session and
wins first. Copying the compositor's lines into our files is rejected (§9).

## 6. Flows

**Login (niri).** Compositor starts → `glimpse-session.target` wants the daemon (`glimpse-session.target:6`)
→ daemon reads config, starts the GSettings observer, waits for its first value (`services.rs:53`),
acquires `me.aresa.Glimpse.Settings`, builds `Inputs`, acquires `me.aresa.Glimpse.Settings.Portal`,
emits `SettingChanged` for every published key → `glimpsectl session declare` sets
`glimpse:niri` and restarts `xdg-desktop-portal.service` (`session.rs:29-46`) → the frontend resolves
the chain `glimpse-settings, gtk, gnome` and creates a proxy per name → apps `ReadAll` and get ours
merged over gtk/gnome. If the portal restarts before the daemon owns its name, the first `ReadAll` goes
to gtk/gnome and ours replaces it as soon as the name exists (§3.2); the daemon's startup emission
then updates apps that cached the earlier answer.

**Hold or policy change.** `glimpsectl appearance set dark` → `SetTemporaryMode(2)` (`provider.rs:45-52`)
→ `DesktopMode.apply` (`desktop_mode.rs:267-284`) sets `temporary` and dispatches the GSettings write
→ `ModeStatus` changes → `Inputs` → `Keys.color_scheme = Some(1)` differs → one
`SettingChanged(org.freedesktop.appearance, color-scheme, 1)`; independently dconf changes, gtk/gnome
emit the same value (§3.2: forwarded, duplicates are equal by D7). `Resume` and clean shutdown clear
the hold the same way.

**Config reload.** User edits `[appearance]` → `Reload::Changed` (`main.rs:61-64`) → `reconfigure` updates
`accent`, `contrast`, `animations_off` → `derive` → diff → per-key signal. A rejected file
(`Reload::Rejected`, `main.rs:66`) leaves `Inputs` unchanged and reports the reason (`services.report`).

**App read.** Client → `org.freedesktop.portal.Settings.ReadOne` → frontend asks backends in order
(`settings.c:211-230`): ours answers or returns `NotFound`, the next is asked → first success returned.

**Daemon crash.** Process dies → owner gone, frontend keeps its proxy (§3.2) → `ReadAll/Read` come from
gtk/gnome, whose `color-scheme` equals the last GSettings write (D7) → systemd restarts the unit after
2 s → name acquired, startup emission → ours again.

**Outside a glimpse session.** The `.portal` file's `UseIn=glimpse` and the conf lines name us only for
sessions that use glimpse's files; if the daemon unit cannot start (`Requisite=glimpse-session.target`),
activation fails, the frontend skips the backend and answers from gtk (§3.2).

## 7. Acceptance criteria

```
AC-1  As a user who runs `glimpsectl appearance set dark`, I want every portal client to see dark
      at once, so that apps follow without a GNOME backend.
      GIVEN the daemon runs, hold off, desktop light WHEN `glimpsectl appearance set dark` THEN
      `gdbus call --session -d org.freedesktop.portal.Desktop -o /org/freedesktop/portal/desktop -m
      org.freedesktop.portal.Settings.ReadOne org.freedesktop.appearance color-scheme` returns `1`, a
      client subscribed to `SettingChanged` receives `color-scheme 1`, and `gsettings get
      org.gnome.desktop.interface color-scheme` is `'prefer-dark'`.
      Verify by: unit test `portal::tests::hold_dark_derives_scheme_1` (derive), live `gdbus`/`gsettings` run.

AC-2  As a user with no GNOME portal backend installed, I want the appearance keys anyway.
      GIVEN `xdg-desktop-portal-gnome` not installed WHEN a client calls `ReadAll
      ["org.freedesktop.appearance"]` THEN `color-scheme` is present from the daemon; with
      `xdg-desktop-portal-gtk` also absent, the portal still exports Settings with only our backend.
      Verify by: private-bus script extending `scripts/portal-routing.sh` with the daemon and fake
      gtk absent (log shows `Providing portal org.freedesktop.portal.Settings`), then `ReadAll`.

AC-3  As a user who sets `accent-color = "teal"`, I want apps to use teal.
      GIVEN the daemon runs WHEN the config reloads with `accent-color = "teal"` THEN `ReadOne
      org.freedesktop.appearance accent-color` is `(0.1294, 0.5647, 0.6431)` within 1e-4 and a
      `SettingChanged accent-color` arrives once; `adw::StyleManager::accent_color()` in a glimpse
      surface becomes teal.
      Verify by: unit `portal::tests::named_accent_is_published`, `glimpse-config` test
      `accent_color_parses_names_and_hex`, live `gdbus` + panel visual check.

AC-4  As a user who mistypes the accent, I want the old value kept and the error shown.
      GIVEN a valid `accent-color` WHEN the file is saved with `accent-color = "tael"` or
      `"#12345"` THEN the reload is rejected (`main.rs:66`), the published accent does not change, and
      `glimpsectl` reports "accent-color must be empty, a color name or #rrggbb".
      Verify by: `glimpse-config` test `accent_color_rejects_unknown_names_and_bad_hex`; daemon test
      `rejected_reload_keeps_published_keys` (pattern `services/tests.rs`).

AC-5  As a user who never set an accent, I want my GNOME accent preserved.
      GIVEN `accent-color = ""` and gsettings accent `'green'` WHEN a client reads
      `accent-color` THEN it gets gnome's green; the daemon's own `Read accent-color` is `NotFound`;
      clearing a previously set accent emits `(-1,-1,-1)` once (D12).
      Verify by: `portal::tests::unset_accent_is_absent_then_neutral_on_clear`; live `gdbus` on the
      daemon's name `me.aresa.Glimpse.Settings.Portal` and on the frontend.

AC-6  As a low-vision user, I want `contrast = "high"` to reach apps, and `desktop` to leave my
      GNOME setting alone.
      GIVEN `contrast = "high"` THEN `ReadOne … contrast` is `1`; with `"normal"` it is `0` even if
      gsettings `high-contrast` is true; with `"desktop"` the daemon returns `NotFound` and the
      frontend returns the gnome/gtk value.
      Verify by: `portal::tests::contrast_publication_rules`; live `gdbus`.

AC-7  As a user who turns glimpse animations off, I want the portal to say reduced motion.
      GIVEN `animation-speed = 0` THEN `reduced-motion` is `1`; at `1.0` the key is absent from the
      daemon and the frontend returns gnome's value.
      Verify by: `portal::tests::animation_speed_zero_publishes_reduced_motion`; live `gdbus`.

AC-8  As an app author, I want a correct impl, so that clients get standard behavior.
      GIVEN the daemon WHEN called directly on `me.aresa.Glimpse.Settings.Portal` THEN `ReadAll([])`,
      `ReadAll([""])`, `ReadAll(["*"])`, `ReadAll(["org.freedesktop.*"])` and
      `ReadAll(["org.freedesktop.appearance"])` return the appearance namespace;
      `ReadAll(["org.gnome.*"])` returns an empty dict; `Read` of an unknown namespace or an
      unpublished key fails with `org.freedesktop.portal.Error.NotFound`; `version` is 0. A
      subscribed client receives a `SettingChanged` emitted from the first startup, repeated live on
      `dbus-broker` 5 of 5 starts.
      Verify by: `portal::tests::read_all_patterns`, `portal::tests::read_unknown_is_not_found`; `busctl
      --user introspect me.aresa.Glimpse.Settings.Portal /org/freedesktop/portal/desktop`; live
      restart loop `systemctl --user restart glimpse-settings-daemon` with `gdbus monitor`.

AC-9  As a user outside a glimpse session, I want portal answers unchanged.
      GIVEN `glimpse-session.target` inactive and the daemon stopped WHEN a client calls the
      portal's `ReadAll` THEN it answers from gtk/gnome without a hang longer than the activation
      failure and the daemon is not running afterward.
      Verify by: `systemctl --user stop glimpse-session.target`, then `gdbus call … ReadAll`, `systemctl
      --user is-active glimpse-settings-daemon`.

AC-10 As a user whose daemon dies, I want apps to keep a sensible value and recover alone.
      GIVEN a hold of dark is active WHEN the daemon is killed with SIGKILL THEN `ReadOne color-scheme`
      still returns `1` (from gtk/gnome, same GSettings write) and after the 2 s restart returns `1`
      from our backend again, with no portal restart; a startup `SettingChanged` is emitted for every
      published key.
      Verify by: `kill -9 $(pidof glimpse-settings-daemon)`, `gdbus call` loop, `gdbus monitor
      --session --dest org.freedesktop.portal.Desktop`.

AC-11 As a user who starts the portal before glimpse, I want ours picked up when it appears.
      GIVEN the daemon stopped, the portal running WHEN the daemon starts THEN `ReadOne accent-color`
      (configured) returns our value without restarting `xdg-desktop-portal`.
      Verify by: private-bus script with the real daemon after the portal (§3.2 pattern, with
      `--config-dir` and a temporary `XDG_CONFIG_HOME`, never `~/.config/glimpse`).

AC-12 As a user, I want D-Bus activation to start the daemon on first portal call.
      GIVEN the session target active, the daemon unit stopped WHEN a client calls `ReadAll` THEN
      activation of `me.aresa.Glimpse.Settings.Portal` starts `glimpse-settings-daemon.service` and
      the call returns our keys.
      Verify by: `systemctl --user stop glimpse-settings-daemon`, `gdbus call … ReadAll`,
      `systemctl --user is-active glimpse-settings-daemon`; packaging test asserting the activation
      file text (pattern `packaging.rs:327-345`).

AC-13 As a user, I want one daemon.
      GIVEN the daemon runs WHEN a second `glimpse-settings-daemon` is started THEN it exits non-zero
      at `me.aresa.Glimpse.Settings` (`services.rs:54-56`) before touching GSettings or the portal name.
      Verify by: daemon test `second_instance_does_not_take_the_portal_name`; live second start.

AC-14 As a user, I want a squatter on the portal name not to break color-scheme.
      GIVEN another process owns `me.aresa.Glimpse.Settings.Portal` WHEN the daemon starts THEN it
      logs a warning, Settings1 and the GSettings write work, the portal serves no keys of ours.
      Verify by: daemon test `portal_name_taken_degrades` (private bus); `journalctl --user -u
      glimpse-settings-daemon`.

AC-15 As a GTK3 user, I want the dual write to stay.
      GIVEN any hold or policy change THEN `gsettings get org.gnome.desktop.interface color-scheme`
      equals the portal value mapped back (1 → `prefer-dark`, 2 → `prefer-light`); with dconf
      unavailable the portal still publishes `requested`.
      Verify by: existing `desktop_mode` tests unchanged; daemon test
      `portal_follows_requested_when_gsettings_is_unavailable` (extends
      `services/tests.rs:290`).

AC-16 As a Qt or Firefox user, I want gtk's other namespaces to remain.
      GIVEN the shipped conf WHEN a client calls `ReadAll []` THEN `org.gnome.desktop.interface` (45
      keys on this machine), fontconfig and cursor keys are present, from gtk/gnome.
      Verify by: live `gdbus call … ReadAll '[]'` count of namespaces equals the §3.1 list plus appearance.

AC-17 As a packager, I want the routing proven across every config layer and the files installed.
      GIVEN a built package WHEN `scripts/portal-layers.sh glimpse:niri` and `scripts/portal-layers.sh
      Hyprland` run THEN each prints `ok` for: `Settings` → `glimpse-settings, gtk` (niri also `gnome`;
      Hyprland no `gnome`), `Inhibit` → `glimpse`, `Screenshot` → `glimpse-shot`, `Wallpaper` →
      `glimpse-wallpaper`, and for every other interface the backend the compositor's own file picks
      (niri: `Access` → `gtk`, `Notification` → `gtk`, `Secret` → `gnome-keyring`, `FileChooser` →
      `gnome`; Hyprland: `default` → `hyprland`, gtk fallback); and `glimpse-settings.portal` plus the
      activation file reach deb, rpm, openSUSE and Nix; every conf contains the Settings line.
      Verify by: new `scripts/portal-layers.sh` (§5.6); `crates/glimpse-package/tests/packaging.rs` new
      test `settings_portal_assets_reach_every_package` beside `:278`; `nix/test.nix` grep beside `:34-39`;
      `scripts/portal-routing.sh` keeps its single-directory check for our own interfaces.

AC-18 As a hostile app, I cannot stall the daemon.
      GIVEN `ReadAll` with 100 000 patterns or a 1 MiB pattern WHEN called THEN the call returns
      within 50 ms with the same result as the first 64 patterns, no panic, nothing logged from the input.
      Verify by: `portal::tests::oversized_patterns_are_bounded`.

AC-19 As a user, I want the docs to match.
      GIVEN the change WHEN reading `website/start/first-session.md:90-93`,
      `website/config/services/appearance.md`, `website/developers/files.md:34-45` THEN they name the
      daemon as the portal backend, the two new fields and the new portal row; the settings README's
      closing paragraph states the keys and the dual write.
      Verify by: `just verify` (docs lint) and inspection.
```

## 8. Rollout and slicing

Each step merges alone and leaves the desktop working. One builder per crate. Tasks carry
`Source: docs/plans/settings-portal.md §N`.

| Step | Task | Crates | Depends on |
| --- | --- | --- | --- |
| 1 | `accent-color`, `contrast`, `Contrast`, `ACCENTS`, `accent_rgb`, validation tests, generated artifacts (§5.3) | glimpse-config, data | — |
| 2 | `portal.rs`, `portal/keys.rs`, tests, startup and shutdown wiring, degrade on name taken (§5.4-5.5); daemon README paragraph | glimpse-settings-daemon | step 1 |
| 3 | `glimpse-settings.portal`, activation file, Settings line in all three confs (§5.6-5.7), packaging/Nix assertions, the layering test (§5.6), `scripts/portal-routing.sh` list form, `scripts.md` testing reference line | data, glimpse-package, nix, scripts | step 2 |
| 4 | Docs: `first-session.md`, `appearance.md`, `files.md` (new row, conf text) | website | step 3 |
| 5 | Live verification AC-1..AC-17 on niri and on a Hyprland VM | — | steps 1-4 |

Step 3 and the khzm8 bead touch the same three confs; the khzm8 work reduces to the layering test in §5.6 plus a bead note, so there is no second edit of their contents.

## 9. Rejected

- ~~Name the backend `glimpse` and add Settings to `glimpse.portal`~~: that file's `DBusName` is Idle's (D1).
- ~~Mirror `org.gnome.desktop.*`, fonts and cursor from GSettings in the daemon~~: no policy of ours, a second view of dconf, and the gtk backend already serves it (D9).
- ~~Put `none` for gnome on the Settings line to remove the dependency~~: `none` drops the interface for the whole session (§3.2).
- ~~Write GSettings `accent-color` as the nearest name~~: unsandboxed libadwaita apps already use the portal when present (assumed, §3.2); the extra write adds a second source for the same fact (D7).
- ~~Publish `reduced-motion` 0 and `contrast` 0 unconditionally~~: overrides a GNOME-side preference glimpse was not asked to own (D2).
- ~~Filter shadowed backends' `SettingChanged`~~: the frontend forwards them and glimpse cannot intercept; the dual write keeps `color-scheme` equal (D13).
- ~~A runtime `SetContrast` hold on `Settings1`~~ for this plan: persistence belongs to config (D15, Q3).
- ~~Put a `default` in `glimpse-portals.conf`~~: it is compositor-agnostic (§5.7).
- ~~Copy upstream compositor lines (`default`, `Access`, `Notification`, `Secret`) into `niri-portals.conf` and `hyprland-portals.conf` (khzm8 as written)~~: layering already applies the compositor's file below ours (§3.2 measured); the copy pins lines the compositor package owns, has no effect on `glimpse:niri` sessions because `/etc/.../niri-portals.conf` is not read there (`glimpse-portals.conf` wins the directory), reverses `packaging.rs:296, 365` and `nix/test.nix:39`, and yields the same Settings chain (Q1).
- ~~Require xdg-desktop-portal ≥ 1.21 in packages~~: older frontends degrade to GSettings (D16).

## 10. Decisions on the former open questions

- ~~**Q1. khzm8 part 1: its premise is false on 1.22.1, so what is the fix?**~~ **Decided: (a).** Our three confs keep only our own lines, plus the Settings line (§5.7). A layering test loads every config layer, not `XDG_DESKTOP_PORTAL_DIR` (§5.6, AC-17). The finding goes on khzm8 as a bead note, outside this plan. Options (b) copy upstream lines and (c) close part 1 as invalid are not taken; (c)'s verification of part 2 (Notification) rides on the same test.
- ~~**Q2. Should `animation-speed = 0` also publish `reduced-motion = 1`?**~~ **Decided: (a).** Derived (D6). The side effect on other apps is stated in the field's doc comment (§5.3).
- ~~**Q3. How is `contrast` changed at runtime?**~~ **Decided: (a).** Config only (D15); no `SetContrast` method.

No open questions remain.

## 11. Review record

Draft by SettingsPortalPlan, 2026-10-26.

Reviewed by the orchestrator, 2026-10-26: `Keys`/`Inputs`/`Contrast`/`ACCENTS` match the prose and
`ModeStatus` (`clients/settings.rs:44-50`); the Dark→1/Light→2 swap in `derive` is correct against
`Mode::code()`. `temporary` before `requested` in D3 is redundant (the daemon already folds the hold
into `requested`) but harmless. No changes. Open questions go to the user.

Decisions recorded by the orchestrator with the user, 2026-10-26: Q1 (a), Q2 (a), Q3 (a) (§10). Applied
by SettingsPortalPlan the same day: Reading A moved to §9, the layering test made concrete in §5.6,
§8 step 3 and AC-17, the `animation-speed` doc comment extended in §5.3, status set to Final. A
self-consistency walk found every `Keys`/`Inputs`/`Contrast`/`ACCENTS`/`SettingsPortal` name used in
prose present in the §5.3-5.4 code blocks and the reverse; no mismatch.
