# glimpse-package

The suite's packaging manifest, and nothing else. `src/lib.rs` is empty on purpose: cargo-deb and
cargo-generate-rpm each need one crate to invoke against, and this is that crate rather than a
binary that happens to be convenient.

## Contents

- `Cargo.toml` — `[package.metadata.deb]`, `[package.metadata.generate-rpm]`, the `requires` table
  and the conffile declaration; the asset lists name every binary plus config, wallpapers, themes,
  units, D-Bus activation files, the GeoClue policy, the license and one line per language
- `tests/packaging.rs` — the guard that keeps the two asset lists in step with `po/LINGUAS`

## Why a crate with no code

Hosting the suite's packaging on one of its binaries ties the product's manifest to a crate that may
be renamed, split or deleted, and makes that binary's manifest carry a comment apologising for
packaging everything else. A crate whose only job is packaging cannot be surprised by any of that.

**The build is invoked as `cargo deb -p glimpse-package` and `cargo generate-rpm -p
crates/glimpse-package`** — note the asymmetry, which is the tools' and not ours: cargo-deb takes a
package *name* and resolves `assets` relative to this manifest, so the `../../data/...` prefixes are
counted from `crates/glimpse-package/`; cargo-generate-rpm takes a *path* and runs from the workspace
root, so its `source` fields have no prefix at all. Two spellings of one asset list is the reason
`tests/packaging.rs` compares them rather than trusting either.

**`$auto` scans the asset binaries, not this crate's.** An empty lib has nothing to link, which
sounds like it should break dpkg-shlibdeps — measured, it does not: cargo-deb resolves `$auto`
against the executables in `assets`. The explicit GTK/libadwaita/layer-shell/libheif/PAM/GeoClue
entries stay anyway, because those are linked by the *bundled* binaries and a partial scan would
silently drop them.

## Translation catalogs

The two manifests list one `.mo` asset **per language**, never a glob. cargo-deb flattens an asset
glob onto the destination directory, so a glob over `target/locale` ships one arbitrary language at
the wrong path and still exits 0. `tests/packaging.rs` reads `po/LINGUAS` and fails when a language
reaches it without reaching both asset lists; a second test fails if a glob is ever reintroduced.
Both read the manifest with comment lines stripped, because the prose describing the hazard
otherwise trips the guard against it.

`just package-deb`, `just package-rpm` and `just package-binary` all depend on
`build-translations`, so the `.mo` files exist before any of them resolves its assets. The binary
tarball is what the AUR package unpacks, so a language missing there is missing from every Arch
install as well — `package-binary.sh` walks `target/locale/*/LC_MESSAGES` and rebuilds `<lang>/`
from the source path, the same way it walks the themes below.

## Arch

`just package-aur` builds `dist/glimpse-desktop-bin-<version>-1-x86_64.pkg.tar.zst` from the
tarball `package-binary` just wrote, through `makepkg`. It is the same `PKGBUILD` the AUR carries:
`scripts/render-pkgbuild.sh --local` only swaps the source for the tarball sitting beside it and
skips the checksum, because there is nothing downloaded to check. `--release`, used by
`just release-pkgbuild` from the release workflow, keeps the published URL and its real b2sum.
Keeping one `PKGBUILD` is the point — a second copy would drift on `depends` first. `makepkg` runs
`--nodeps` because `package()` only copies an already-built tree; the declared dependencies are
what the package needs to run, not what building it needs.

`just install-aur` builds it and hands it to `pacman -U`; `just uninstall-aur` removes it again.
Both go through the `elevate` variable, which is `sudo` — these are run from a terminal that has
one. **`pkexec` is the wrong default here**: it drops the working directory, so a relative package
path never resolves, and its own polkit path is the fragile one — a correct password still fails
with `No session for cookie` when the agent cannot tie the calling process to a session. Set
`GLIMPSE_SUDO=pkexec` from a launcher with no terminal, and pass the package by absolute path,
which these recipes do anyway. Neither passes `--noconfirm`: a transaction that writes to `/usr`
is worth reading first.

**Installing changes which binary the session bus activates** — the D-Bus service files name
`/usr/bin/`, so an installed package wins over a `target/` build for anything started by
activation rather than by hand.

**A dependency is declared because something links it, not because it sounds right.** `ldd` over
every shipped binary is the check; it is what retired `libheif` from all three manifests, left over
from an implementation that did link it. `glimpse-lock` links `libpam`, so `pam` is declared. Everything the applets merely
*talk to* over D-Bus — UPower, power-profiles-daemon, NetworkManager, BlueZ, PackageKit,
xdg-desktop-portal — is an `optdepends`, because each one absent is a degraded applet rather than a
broken install.

**`options=('!debug')`.** `profile.release` sets `strip = true`, so there are no symbols left to
split: the debug package comes out as nothing but `.build-id` links, and pacman does not remove it
with its parent, so it lingers after an uninstall.

## The user's own configuration

The packages ship `/usr/share/glimpse/config.commented.toml` and seed nothing. A `.deb`, `.rpm` or
pacman install runs as root and has no user to write a home directory for, which is why none of the
three carries a `postinst`: the four UI binaries call `glimpse_config::seed_user_config` before
`load`, and whichever starts first writes `~/.config/glimpse/config.toml`. It is written with
`File::create_new`, so simultaneous starts cannot race and a symlink cannot be followed onto an
existing file, and it returns nothing — a lock screen that refused to start over a template file is
the worse bargain.

`scripts/install.sh` seeds the same file for a source install, where the invoking user is known. It
skips entirely under `DESTDIR`, because a packaging build must not touch anybody's home, and takes
the destination and owner from `SUDO_USER`'s passwd entry — root's `$HOME` names the wrong person,
and a root-owned `config.toml` is one its owner cannot edit.

**What is seeded is the commented copy, and every value in it is inert.** A user uncomments what
they want to change, so a later release's changed default still reaches them. Its first line is a
`#:schema` directive pointing at the installed `config.schema.json`, which is what gives a TOML
language server — taplo, or an editor extension built on it — completion for every table and key,
each setting's documentation on hover, and a diagnostic on a value the schema refuses.

## Themes

Themes are the one asset whose directory structure is load-bearing: `themes/<name>/panel.css` is
found by name, so a flat glob into a single destination would collapse every theme's sheets on top of
one another. `scripts/install.sh` and `scripts/package-binary.sh` walk `data/themes/*/*.css` and
rebuild `<name>/` from the source path, and generalise to any number of themes; the two static asset
lists cannot compute a destination, so each shipped theme needs its own line in both. Only `adwaita`
ships today.

`data/pam.d/glimpse-lock` is the locker's PAM stack for Arch and the binary tarball, built on
`system-auth`. Every package installs its own stack at the same `/etc/pam.d/glimpse-lock`:

- the rpm ships `data/pam.d/fedora/glimpse-lock`, built on `password-auth`, because authselect can
  put `pam_fprintd` in Fedora's `system-auth` and the prompt only ever sends a password;
- the deb ships `data/pam.d/debian/glimpse-lock`, built on `common-auth` and `common-account`;
- the `opensuse` rpm variant ships that same Debian stack, since openSUSE has neither `system-auth`
  nor `password-auth` and `@include` is upstream Linux-PAM syntax. A variant replaces `assets` and
  `requires` whole, so its asset list repeats the base one and its `requires` names only `geoclue2`
  and `pam`, the two Fedora names openSUSE shares; auto-req covers the linked libraries by soname.
  `just package-rpm` builds both rpms, and the variant's `1.opensuse` release keeps their file names
  apart.

Every manifest marks the stack a configuration file, so an edited stack survives an upgrade. The
install scripts copy regular files out of `data/pam.d/` only, so the per-distribution directories
never land in `/etc/pam.d`.
