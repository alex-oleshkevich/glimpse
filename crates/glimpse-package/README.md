# glimpse-package

The suite's packaging manifest, and nothing else. `src/lib.rs` is empty on purpose: cargo-deb and
cargo-generate-rpm each need one crate to invoke against.

## Contents

- `Cargo.toml` — `[package.metadata.deb]`, `[package.metadata.generate-rpm]`, `requires`, the
  conffile declaration, and the asset lists naming every binary, script, config, wallpaper, theme,
  unit, D-Bus activation file, the GeoClue policy, the license and one line per language
- `tests/packaging.rs` — the guard that keeps the two asset lists in step with `po/LINGUAS`

## Rules

Packaging lives in its own crate so it is never tied to a binary that might be renamed or deleted.
**The build is invoked as `cargo deb -p glimpse-package` and `cargo generate-rpm -p
crates/glimpse-package`** — cargo-deb takes a package *name* and resolves `assets` relative to this
manifest, so `../../data/...` prefixes count from here; cargo-generate-rpm takes a *path* and runs
from the workspace root, so its `source` fields have no prefix. Two spellings of one list is why
`tests/packaging.rs` compares them rather than trusting either. **`$auto` scans the executables named
in `assets`, not this crate's** (which links nothing); the explicit GTK/libadwaita/layer-shell/
libheif/PAM/GeoClue entries stay anyway, since a partial scan would silently drop what the bundled
binaries actually link.

**The two manifests list one `.mo` asset per language, never a glob** — cargo-deb flattens a glob
onto the destination directory, so one over `target/locale` ships one arbitrary language at the
wrong path and exits 0 anyway. `tests/packaging.rs` fails when a language reaches `po/LINGUAS`
without reaching both asset lists, or when a glob is reintroduced, reading the manifest with
comments stripped so the prose describing the hazard cannot trip the guard. `package-deb`,
`package-rpm` and `package-binary` all depend on `build-translations` so the `.mo` files exist first.

**`just package-aur` builds through `makepkg` from the same `PKGBUILD` the AUR carries** —
`render-pkgbuild.sh --local` swaps the source for the local tarball and skips the checksum,
`--release` keeps the published URL and its b2sum; one `PKGBUILD` is the point, since a second copy
would drift on `depends` first, and `makepkg --nodeps` is right because `package()` only copies an
already-built tree. **`install-aur`/`uninstall-aur` elevate with `sudo`, never `pkexec`** — pkexec
drops the working directory, so a relative package path never resolves, and a correct password can
still fail with `No session for cookie` when its agent cannot tie the process to a session; use
`GLIMPSE_SUDO=pkexec` plus an absolute path for a launcher with no terminal. **Installing also
changes which binary the session bus activates** — D-Bus service files name `/usr/bin/`, so an
installed package wins over a `target/` build for anything started by activation.

**A dependency is declared because something links it, checked with `ldd`.** Everything an applet
merely *talks to* — UPower, power-profiles-daemon, NetworkManager, BlueZ, UDisks2, CUPS, KDE Connect,
xdg-desktop-portal, `ddcutil` — is `optdepends`, since its absence degrades an applet rather than
breaking the install. **`options=('!debug')`**, since `strip = true` in the release profile leaves no
symbols to split, so the debug package is nothing but `.build-id` links pacman never removes with
its parent.

**Packages seed no user configuration** — an install runs as root with no home to write, so none
carries a `postinst`. Each UI binary calls `glimpse_config::seed_user_config` before `load`, and
whichever starts first writes `~/.config/glimpse/config.toml` with `File::create_new`, so
simultaneous starts cannot race; `scripts/install.sh` seeds the same file for a source install, using
`SUDO_USER`'s passwd entry for the real destination and owner (root's `$HOME` names the wrong
person), and skips entirely under `DESTDIR`. **The seeded file is the fully commented copy, and every
value in it is inert**, so an uncommented default still updates on the next release; its `#:schema`
first line points at the installed `config.schema.json`, giving a TOML language server completion,
hover docs and diagnostics.

**Theme directory structure is load-bearing** — `themes/<name>/panel.css` is found by name, so a
flat glob into one destination would collapse every theme's sheets together; `install.sh` and
`package-binary.sh` walk `data/themes/*/*.css` and rebuild `<name>/`, since the static asset lists
cannot compute a destination and each theme needs its own line in both. **Each package installs its
own PAM stack at `/etc/pam.d/glimpse-lock`**: `data/pam.d/glimpse-lock` (Arch/binary tarball, on
`system-auth`), `data/pam.d/fedora/…` (rpm, on `password-auth`, since
authselect can put `pam_fprintd` in Fedora's `system-auth`), and `data/pam.d/debian/…` (deb, on
`common-auth`/`common-account`) — the openSUSE rpm variant ships the Debian stack too, since it has
neither `system-auth` nor `password-auth`. Every manifest marks the stack a configuration file, so
an edit survives an upgrade, and the install scripts copy files out of `data/pam.d/` only, so the
per-distribution directories never land in `/etc/pam.d`.
