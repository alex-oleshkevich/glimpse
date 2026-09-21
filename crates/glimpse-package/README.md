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

## Themes

Themes are the one asset whose directory structure is load-bearing: `themes/<name>/panel.css` is
found by name, so a flat glob into a single destination would collapse every theme's sheets on top of
one another. `scripts/install.sh` and `scripts/package-binary.sh` walk `data/themes/*/*.css` and
rebuild `<name>/` from the source path, and generalise to any number of themes; the two static asset
lists cannot compute a destination, so each shipped theme needs its own line in both. Only `adwaita`
ships today.

`data/pam.d` is still an empty placeholder, so its contents are not in the asset lists yet; add them
once something real lands there.
