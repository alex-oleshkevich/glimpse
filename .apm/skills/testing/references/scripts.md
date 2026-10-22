# Manual fixtures in scripts/

Each entry names what the script drives, how to run it, and what to watch. Read the script's own
header before running it; the notes here are the traps that header does not cover.

## Safety nets

- **`net-guard.sh`** (`just net-guard`) — keeps the machine online while the network applet is
  tested by hand. `baseline` before the first change, `arm [seconds]` to self-heal in the
  background, `disarm` when done, `restore` to bring the recorded profile back once.
- **`glimpse-lock-rescue-pam.sh` / `glimpse-lock-restore-pam.sh`** — break-glass for a lock that
  refuses every password: rescue swaps `/etc/pam.d/glimpse-lock` for `pam_permit`, backing the
  original up to `glimpse-lock.glimpse-rescue.bak` and resetting faillock; restore puts that backup
  back. Both need sudo, so the user runs them; never leave the rescue file installed.

## Manual tests

- **`a_real_drm_change_event_reaches_the_hotplug_stream`** — waits 60s for a real DRM uevent.
  `just test-compositor` names it as left out; run it with `just test-one glimpse-services
  services::brightness::tests::a_real_drm_change_event_reaches_the_hotplug_stream` and, in another
  terminal, `sudo udevadm trigger --subsystem-match=drm --action=change`.

## Input and windows

- **`click.py`** (`just click`) — clicks a point on a niri output through a wlr virtual pointer:
  `just click output=eDP-1 x=40 y=12 [button=right]`. The `live-testing` skill covers it.
- **`urgency-test.sh [ID] [--clear] [--list]`** — marks a niri window urgent so the workspace pager
  paints it. Mark a window on another workspace: focusing it clears urgency. niri only.

## Notifications

- **`notify-command [--no-app-name|--no-icon|--no-image|--no-body|--no-actions|--no-hints]
  [--urgency=LEVEL] [SUMMARY [BODY]]`** — sends the fullest notification `notify-send` can carry
  (app name, icon, `image-path` hint, category, desktop-entry, three actions); each `--no-*` strips
  one part, to check how the popup degrades.

## Media, calendar, storage

- **`mpris-fake-players.py [--plain-names]`** — three fake MPRIS players with long text, generated
  artwork and different playback states on the session bus. Needs `uv`. Ctrl-C removes them.
- **`calendar-fake-event.py [--in MIN] [--duration MIN] [--title T] [--location L] [--name F]
  [clear]`** — writes an `.ics` event for the `next_event` applet and prints the
  `[[calendar.sources]]` block to add. It defaults to `~/.config/glimpse/calendars`: always pass
  `--dir "$SCRATCH/calendars" --config "$SCRATCH/cfg/config.toml"`. Its unit test is
  `python3 scripts/calendar_fake_event_test.py`.
- **`removable-test.sh up [--parts N] [--size MB] [--optical] [--readonly] | down | status`** —
  attaches a RAM-backed `scsi_debug` drive with `Removable` set, the only way to make UDisks2 show
  the removable applet a drive without hardware. Needs root (`GLIMPSE_SUDO`, default `sudo`); the
  user runs it.
- **`docs_udisks2_mock.py`** — a python-dbusmock UDisks2 template. Only `docs-screenshots.py` loads
  it, on its private system bus.

## Network and Bluetooth

- **`network-test-fixtures.sh`** — creates a veth wired connection and a WireGuard VPN profile in
  NetworkManager, prints their state and holds them until Ctrl-C, which deletes both. Interface
  names, addresses and `GLIMPSE_TEST_VPN_UP=0` (saved but inactive) come from `GLIMPSE_TEST_*`
  variables. Needs `ip`, `nmcli`, `wg` and sudo; arm `net-guard` first.
- **`bt-discovery-clients.sh`** — prints each BlueZ adapter's `Discovering` flag and the processes
  that look like Bluetooth clients, to check that closing the bluetooth popover ends discovery.

## Printing

- **`printing-mock-cups.py [--auto N]`** — replaces CUPS with an IPP server on port 16631
  (`GLIMPSE_MOCK_PORT`) that steps through printer and job states on Enter. Point a test panel at it
  with `[printing] server-url = "http://localhost:16631/"`. Use it for widget states.
- **`printing-network-printer.sh run | up | jobs [N] | status | down`** — publishes a real IPP
  Everywhere printer that CUPS discovers through cups-browsed. Use it for discovery, job control
  and anything that must hold for real CUPS. Low toner and out-of-paper cannot reach the applet.

## Privacy indicators

Each opens one device for `SECONDS` (default 10) so the privacy applet lights up. Screen sharing is
read from compositor casts, the camera from a `/proc` fd scan, the microphone from PipeWire.

- **`privacy-test-camera-v4l2 [SECONDS] [DEVICE]`** — opens `/dev/video0` with ffmpeg. The camera
  test that lights the indicator.
- **`privacy-test-camera-pipewire`** — consumes the PipeWire camera node; checks PipeWire's own
  bookkeeping, not the path glimpse watches.
- **`privacy-test-mic-pipewire`** — records with `pw-record`; the microphone test.
- **`privacy-test-mic-alsa`** — records with `arecord` on the default PCM, which `pipewire-alsa`
  routes through PipeWire; it only bypasses PipeWire with an explicit `-D hw:N`.
- **`privacy-test-screencast-wlr`** — starts wayvnc, a wlr-screencopy cast (`niri msg -j casts`
  shows `WlrScreencopy`). The screen-sharing test.
- **`privacy-test-screencast-pipewire`** — the same through `wf-recorder`, when it is installed.
- **`privacy-test-screencast-ffmpeg`** — x11grab; refuses to run on Wayland.
- **`privacy-test-location`** — holds a GeoClue client with `where-am-i`. `InUse` may already be
  true from glimpse-weather or glimpse-sunset; read it with `gdbus` before trusting a transition.

## Applets, portals, rendering

- **`exec-applet-demo.sh install [args…] | remove`** — installs a desktop entry
  (`me.aresa.GlimpseExecDemo`, `Implements=me.aresa.Glimpse.Applet1`) that runs
  `exec-applet-demo.py`, a protocol-speaking exec applet. `--mode garbage|huge-line|flood|no-read|
  exit-before-hello|crash-after SECONDS|many-nodes [--nodes N]|churn` makes it misbehave on purpose;
  `exec-applet-demo.py --self-test` checks the demo itself.
- **`portal-routing.sh <XDG_CURRENT_DESKTOP>`** — starts xdg-desktop-portal on a private bus with
  `XDG_DESKTOP_PORTAL_DIR` holding the host's backends plus `data/portals/*`, and fails unless
  Inhibit resolves to `glimpse` and Screenshot to `glimpse-shot`. Run it for `niri` and
  `glimpse:niri` after any change under `data/portals/`.
- **`blur-wire.sh [-v]`** — reads `WAYLAND_DEBUG=1` output on stdin and prints each surface's
  background-blur regions; a surface holding more than one blur object is `BAD` and fails the run.
  `WAYLAND_DEBUG=1 glimpse-panel --config-dir "$SCRATCH/cfg" 2>&1 | scripts/blur-wire.sh`.
